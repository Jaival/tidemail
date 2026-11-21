//! Search: the input in the search slot (placed by the shell) and the results
//! list that replaces the thread list while a query is active.
//!
//! Every change of the text runs `tidemail_search::search` on the background
//! executor. Only the newest query is kept: a new query drops the task of the
//! previous one, so a stale result is never applied.

mod snippet;

use std::ops::Range;

use gpui_kit::component::{
    input::{Escape, Input, InputEvent, InputState},
    ActiveTheme,
};
use gpui_kit::*;
use tidemail_db::types::{SearchHit, ThreadRow};

use crate::{
    features::list::row,
    format,
    state::{AppState, StateEvent},
    theme::colors,
    ui,
};

pub use snippet::Snippet;

/// Hits per query (`tidemail_search` caps it at one page).
const LIMIT: u32 = 100;

/// Emitted when focus should return to the shell.
pub enum SearchEvent {
    Leave,
}

impl EventEmitter<SearchEvent> for Search {}

pub struct Search {
    state: Entity<AppState>,
    input: Entity<InputState>,
    pub results: Entity<Results>,
    task: Option<Task<()>>,
    /// The query is not empty: the results list is shown.
    pub active: bool,
    pub focused: bool,
    _subscriptions: Vec<Subscription>,
}

impl Search {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search  /"));
        let results = cx.new(|cx| Results::new(state.clone(), cx));
        let subscriptions = vec![
            cx.subscribe_in(&input, window, |this, _, event, _, cx| match event {
                InputEvent::Change => this.run(true, cx),
                InputEvent::PressEnter { .. } if this.active => {
                    this.results.update(cx, |r, cx| r.open_current(cx));
                    cx.emit(SearchEvent::Leave);
                }
                InputEvent::Focus | InputEvent::Blur => {
                    this.focused = matches!(event, InputEvent::Focus);
                    cx.notify();
                }
                _ => {}
            }),
            // Archive, trash and sync change what a query finds.
            cx.subscribe(&state, |this, _, event, cx| {
                if matches!(event, StateEvent::DataChanged { .. }) && this.active {
                    this.run(false, cx);
                }
            }),
        ];
        Self {
            state,
            input,
            results,
            task: None,
            active: false,
            focused: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| i.focus(window, cx));
    }

    /// Empty the query and show the thread list again.
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
        self.run(true, cx);
    }

    fn run(&mut self, new_query: bool, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().trim().to_string();
        self.active = !query.is_empty();
        let active = self.active;
        self.state.update(cx, |s, _| s.searching = active);
        if !self.active {
            self.task = None;
            self.results.update(cx, |r, cx| r.set(Ok(Vec::new()), true, cx));
            cx.notify();
            return;
        }
        let task = self.state.read(cx).db.read(cx, move |c| {
            tidemail_search::search(c, &query, LIMIT).map_err(|e| match e {
                tidemail_search::Error::Db(e) => e,
            })
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let hits = task.await;
            let _ = this.update(cx, |this, cx| {
                this.results.update(cx, |r, cx| r.set(hits, new_query, cx));
            });
        }));
        cx.notify();
    }
}

impl Render for Search {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .w_full()
            .h(px(26.))
            .px_2()
            .flex()
            .items_center()
            .rounded(theme.radius)
            .bg(crate::theme::colors(cx).field)
            .border_1()
            .border_color(theme.input)
            .text_size(px(ui::TEXT_SM + 1.))
            // The input propagates Escape (it has nothing to dismiss).
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.clear(window, cx);
                cx.emit(SearchEvent::Leave);
            }))
            .child(Input::new(&self.input).appearance(false).cleanable(true))
    }
}

struct Hit {
    thread: ThreadRow,
    snippet: Snippet,
}

pub struct Results {
    state: Entity<AppState>,
    hits: Vec<Hit>,
    error: Option<SharedString>,
    scroll: UniformListScrollHandle,
    _subscription: Subscription,
}

impl Results {
    fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let sub = cx.subscribe(&state, |_, _, event, cx| {
            if matches!(
                event,
                StateEvent::SelectionChanged
                    | StateEvent::ReadingChanged
                    | StateEvent::SettingsChanged
            ) {
                cx.notify();
            }
        });
        Self {
            state,
            hits: Vec::new(),
            error: None,
            scroll: UniformListScrollHandle::new(),
            _subscription: sub,
        }
    }

    fn set(&mut self, hits: Result<Vec<SearchHit>, String>, top: bool, cx: &mut Context<Self>) {
        match hits {
            Ok(hits) => {
                self.hits = hits
                    .into_iter()
                    .map(|h| Hit {
                        snippet: snippet::parse(&h.snippet_html),
                        thread: h.thread,
                    })
                    .collect();
                self.error = None;
            }
            Err(e) => {
                self.hits.clear();
                self.error = Some(e.into());
            }
        }
        if top {
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    fn selected_index(&self, cx: &App) -> Option<usize> {
        let id = self.state.read(cx).selected?;
        self.hits.iter().position(|h| h.thread.id == id)
    }

    /// Thread the actions apply to: the current thread, if it is a result.
    pub fn current(&self, cx: &App) -> Option<ThreadRow> {
        self.selected_index(cx).map(|i| self.hits[i].thread.clone())
    }

    /// Select (or open) the result at `ix`, like the thread list.
    fn move_to(&mut self, ix: usize, open: bool, cx: &mut Context<Self>) {
        let Some(thread) = self.hits.get(ix).map(|h| h.thread.clone()) else {
            return;
        };
        self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        if open {
            let n = self.state.read(cx).settings.prefetch as usize;
            let next: Vec<i64> = self
                .hits
                .iter()
                .skip(ix + 1)
                .take(n)
                .map(|h| h.thread.id)
                .collect();
            self.state.update(cx, |s, cx| {
                s.open(&thread, cx);
                s.prefetch(next, cx);
            });
        } else {
            self.state.update(cx, |s, cx| s.select(&thread, cx));
        }
        cx.notify();
    }

    fn opens_on_move(&self, cx: &App) -> bool {
        let s = self.state.read(cx);
        s.spec().open_on_move || s.reading
    }

    pub fn select_next(&mut self, cx: &mut Context<Self>) {
        let ix = self
            .selected_index(cx)
            .map_or(0, |i| (i + 1).min(self.hits.len().saturating_sub(1)));
        let open = self.opens_on_move(cx);
        self.move_to(ix, open, cx);
    }

    pub fn select_prev(&mut self, cx: &mut Context<Self>) {
        let ix = self.selected_index(cx).map_or(0, |i| i.saturating_sub(1));
        let open = self.opens_on_move(cx);
        self.move_to(ix, open, cx);
    }

    /// Open the current result, or the first one if none.
    pub fn open_current(&mut self, cx: &mut Context<Self>) {
        let ix = self.selected_index(cx).unwrap_or(0);
        self.move_to(ix, true, cx);
    }

    fn render_rows(&self, range: Range<usize>, cx: &Context<Self>) -> Vec<AnyElement> {
        let state = self.state.read(cx);
        let spec = state.spec();
        range
            .map(|ix| {
                let hit = &self.hits[ix];
                let account = state
                    .account(hit.thread.account_id)
                    .map(|a| format::parse_color(&a.color));
                let r = row::Row {
                    thread: &hit.thread,
                    selected: state.selected == Some(hit.thread.id),
                    account,
                    display_font: state.display_font(),
                    snippet: Some(&hit.snippet),
                };
                let id = hit.thread.id;
                row::render(spec.row, spec.row_height, &r, cx)
                    .id(("hit", id as u64))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(ix) = this.hits.iter().position(|h| h.thread.id == id) {
                            this.move_to(ix, true, cx);
                        }
                    }))
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for Results {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let note = |text: SharedString| {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(c.muted_fg)
                .child(text)
        };
        let header = div()
            .h(px(32.))
            .flex_none()
            .px_4()
            .flex()
            .items_center()
            .text_size(px(ui::TEXT_SM))
            .text_color(c.muted_fg)
            .child(match self.hits.len() {
                1 => "1 result".to_string(),
                n => format!("{n} results"),
            });
        let body = if let Some(e) = &self.error {
            note(format!("Search failed: {e}").into()).into_any_element()
        } else if self.hits.is_empty() {
            note("No results".into()).into_any_element()
        } else {
            uniform_list(
                "results",
                self.hits.len(),
                cx.processor(|this, range: Range<usize>, _window, cx| this.render_rows(range, cx)),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
    }
}
