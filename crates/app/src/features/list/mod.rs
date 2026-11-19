//! Virtualized thread list for the current view. Pages through the database
//! with keyset cursors; j/k move the selection (and open per the preset).

pub(crate) mod row;

use std::{ops::Range, time::Duration};

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use tidemail_db::{
    read,
    types::{Cursor, ThreadRow, View},
};

use crate::{
    data::PAGE,
    format,
    state::{AppState, StateEvent},
};

/// Load the next page when the visible range comes this close to the end.
const LOAD_AHEAD: usize = 30;

pub struct ThreadList {
    state: Entity<AppState>,
    view: View,
    pub rows: Vec<ThreadRow>,
    next: Option<Cursor>,
    loading: Option<Task<()>>,
    refresh: Option<Task<()>>,
    pub error: Option<SharedString>,
    pub scroll: UniformListScrollHandle,
    /// Highest row index painted so far in this view (bench: blank rows).
    pub painted_to: usize,
    _subscription: Subscription,
}

impl ThreadList {
    pub fn new(
        state: Entity<AppState>,
        first: tidemail_db::types::ThreadPage,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = state.read(cx).nav.view.clone();
        let sub = cx.subscribe(&state, |this, _, event, cx| match event {
            StateEvent::NavChanged => this.reset(cx),
            StateEvent::DataChanged {
                thread_ids,
                label_keys,
            } => {
                let hit = label_keys.contains(&this.view.label_key)
                    || label_keys.is_empty() && thread_ids.is_empty()
                    || thread_ids
                        .iter()
                        .any(|id| this.rows.iter().any(|r| r.id == *id));
                if hit {
                    this.schedule_refresh(cx);
                }
            }
            StateEvent::SelectionChanged
            | StateEvent::AccountsChanged
            | StateEvent::SettingsChanged
            | StateEvent::ReadingChanged => cx.notify(),
            _ => {}
        });
        Self {
            state,
            view,
            rows: first.items,
            next: first.next,
            loading: None,
            refresh: None,
            error: None,
            scroll: UniformListScrollHandle::new(),
            painted_to: 0,
            _subscription: sub,
        }
    }

    /// Row height of the active preset.
    pub fn row_height(&self, cx: &App) -> f32 {
        self.state.read(cx).spec().row_height
    }

    /// Another page exists beyond the loaded rows.
    pub fn has_more(&self) -> bool {
        self.next.is_some()
    }

    fn selected_index(&self, cx: &App) -> Option<usize> {
        let id = self.state.read(cx).selected?;
        self.rows.iter().position(|r| r.id == id)
    }

    /// Open the row at `ix`, scroll to it and prefetch the next N threads.
    pub fn open_index(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.move_to(ix, true, cx);
    }

    /// Select (or open) the row at `ix` and scroll to it. Opening also
    /// prefetches the next N threads.
    fn move_to(&mut self, ix: usize, open: bool, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(ix).cloned() else {
            return;
        };
        self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        if !open {
            self.state.update(cx, |s, cx| s.select(&row, cx));
            if ix + LOAD_AHEAD >= self.rows.len() {
                self.load_more(cx);
            }
            cx.notify();
            return;
        }
        let n = self.state.read(cx).settings.prefetch as usize;
        let next: Vec<i64> = self
            .rows
            .iter()
            .skip(ix + 1)
            .take(n)
            .map(|r| r.id)
            .collect();
        self.state.update(cx, |s, cx| {
            s.open(&row, cx);
            s.prefetch(next, cx);
        });
        if ix + LOAD_AHEAD >= self.rows.len() {
            self.load_more(cx);
        }
        cx.notify();
    }

    /// j/k open the thread with `open_on_move` or while reading; else they
    /// only move the selection.
    fn opens_on_move(&self, cx: &App) -> bool {
        let s = self.state.read(cx);
        s.spec().open_on_move || s.reading
    }

    pub fn select_next(&mut self, cx: &mut Context<Self>) {
        let ix = self
            .selected_index(cx)
            .map_or(0, |i| (i + 1).min(self.rows.len().saturating_sub(1)));
        let open = self.opens_on_move(cx);
        self.move_to(ix, open, cx);
    }

    pub fn select_prev(&mut self, cx: &mut Context<Self>) {
        let ix = self.selected_index(cx).map_or(0, |i| i.saturating_sub(1));
        let open = self.opens_on_move(cx);
        self.move_to(ix, open, cx);
    }

    /// Open the current thread, or the first row if none.
    pub fn open_current(&mut self, cx: &mut Context<Self>) {
        let ix = self.selected_index(cx).unwrap_or(0);
        self.open_index(ix, cx);
    }

    /// Thread ids the list actions apply to: the current thread.
    pub fn current(&self, cx: &App) -> Option<ThreadRow> {
        self.selected_index(cx).map(|i| self.rows[i].clone())
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.view = self.state.read(cx).nav.view.clone();
        self.rows.clear();
        self.next = None;
        self.error = None;
        self.painted_to = 0;
        self.refresh = None;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.loading = None;
        self.load_more(cx);
        cx.notify();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        if self.loading.is_some() || (!self.rows.is_empty() && self.next.is_none()) {
            return;
        }
        let (view, cursor) = (self.view.clone(), self.next);
        let task = self
            .state
            .read(cx)
            .db
            .read(cx, move |c| read::list_threads(c, &view, cursor, PAGE));
        self.loading = Some(cx.spawn(async move |this, cx| {
            let page = task.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = None;
                match page {
                    Ok(page) => {
                        this.rows.extend(page.items);
                        this.next = page.next;
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        }));
    }

    /// Re-read the loaded range once per burst of changes. If the opened
    /// thread left the view (archive, trash), open the row that took its place.
    fn schedule_refresh(&mut self, cx: &mut Context<Self>) {
        if self.refresh.is_some() {
            return;
        }
        self.refresh = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let Ok((task, old_ix, old_id)) = this.update(cx, |this, cx| {
                let (view, want) = (this.view.clone(), this.rows.len().max(PAGE as usize));
                let task = this.state.read(cx).db.read(cx, move |c| {
                    let mut rows = Vec::with_capacity(want);
                    let mut cursor = None;
                    loop {
                        let page = read::list_threads(c, &view, cursor, PAGE)?;
                        rows.extend(page.items);
                        cursor = page.next;
                        if cursor.is_none() || rows.len() >= want {
                            return Ok((rows, cursor));
                        }
                    }
                });
                (task, this.selected_index(cx), this.state.read(cx).selected)
            }) else {
                return;
            };
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.refresh = None;
                let Ok((rows, next)) = result else { return };
                this.rows = rows;
                this.next = next;
                let gone = old_id.is_some_and(|id| !this.rows.iter().any(|r| r.id == id));
                if let (true, Some(ix)) = (gone, old_ix) {
                    if this.rows.is_empty() {
                        this.state.update(cx, |s, cx| s.clear_selection(cx));
                    } else {
                        this.open_index(ix.min(this.rows.len() - 1), cx);
                    }
                }
                cx.notify();
            });
        }));
    }

    fn render_row(&self, row: &ThreadRow, dots: bool, cx: &Context<Self>) -> AnyElement {
        let state = self.state.read(cx);
        let spec = state.spec();
        let account = dots
            .then(|| {
                state
                    .account(row.account_id)
                    .map(|a| format::parse_color(&a.color))
            })
            .flatten();
        let r = row::Row {
            thread: row,
            selected: state.selected == Some(row.id),
            account,
            display_font: spec.display_font,
            snippet: None,
        };
        let id = row.id;
        row::render(spec.row, spec.row_height, &r, cx)
            .id(("row", id as u64))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(ix) = this.rows.iter().position(|r| r.id == id) {
                    this.open_index(ix, cx)
                }
            }))
            .into_any_element()
    }
}

impl Render for ThreadList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        if let Some(error) = &self.error {
            return empty("Could not open mailbox", error.clone(), theme).into_any_element();
        }
        if self.rows.is_empty() && self.loading.is_none() {
            return empty(
                "Nothing here",
                "Messages will appear here after sync starts.".into(),
                theme,
            )
            .into_any_element();
        }
        let dots = self.view.account_id.is_none();
        uniform_list(
            "threads",
            self.rows.len(),
            cx.processor(move |this, range: Range<usize>, _window, cx| {
                this.painted_to = this.painted_to.max(range.end);
                if range.end + LOAD_AHEAD >= this.rows.len() {
                    this.load_more(cx);
                }
                range
                    .map(|ix| this.render_row(&this.rows[ix], dots, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full()
        .into_any_element()
    }
}

fn empty(title: &'static str, body: SharedString, theme: &gpui_kit::component::Theme) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .text_color(theme.muted_foreground)
        .child(div().text_lg().text_color(theme.foreground).child(title))
        .child(body)
}
