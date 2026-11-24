//! Ctrl+K palette, label / move / go pickers and toasts. Keys live in
//! `crate::actions`; overlays use `AppState::push_overlay`. The four pickers
//! share one list widget ([`Picker`]): filter input, up/down/enter, mouse.

mod filter;
pub mod toast;

use std::{collections::HashSet, rc::Rc};

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::types::ActionKind;

use crate::{
    actions::{self, *},
    features::shell::nav,
    format,
    preset::Preset,
    settings::{ThemePref, PREFETCH_CHOICES},
    state::{AppState, Nav, Overlay},
    theme::colors,
    ui,
};

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

const ROW_HEIGHT: f32 = 32.;
/// Rows shown before the list scrolls; the half row shows there is more.
const VISIBLE_ROWS: f32 = 10.5;

/// One row of a picker.
pub struct Item {
    label: SharedString,
    /// Muted text at the right, also searched.
    detail: Option<SharedString>,
    keys: Vec<&'static str>,
    /// Current value (theme, layout, label already on the thread).
    checked: bool,
    dot: Option<Hsla>,
    run: Run,
}

impl Item {
    fn new(label: impl Into<SharedString>, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self {
            label: label.into(),
            detail: None,
            keys: Vec::new(),
            checked: false,
            dot: None,
            run: Rc::new(run),
        }
    }

    fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    fn dot(mut self, color: Option<Hsla>) -> Self {
        self.dot = color;
        self
    }

    fn detail(mut self, text: impl Into<SharedString>) -> Self {
        self.detail = Some(text.into());
        self
    }

    fn keys(mut self, keys: Vec<&'static str>) -> Self {
        self.keys = keys;
        self
    }
}

pub struct Picker {
    state: Entity<AppState>,
    overlay: Overlay,
    input: Entity<InputState>,
    items: Vec<Item>,
    /// Indices into `items` that match the filter, best first.
    filtered: Vec<usize>,
    /// Index into `filtered`.
    selected: usize,
    scroll: ScrollHandle,
    _subscription: Subscription,
}

impl Picker {
    fn new(
        overlay: Overlay,
        placeholder: &'static str,
        items: Vec<Item>,
        state: Entity<AppState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        input.update(cx, |i, cx| i.focus(window, cx));
        let sub = cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refilter(cx);
            }
        });
        let mut picker = Self {
            state,
            overlay,
            input,
            items,
            filtered: Vec::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            _subscription: sub,
        };
        picker.refilter(cx);
        picker
    }

    /// Replace the entries (the label picker learns the thread's labels late).
    fn set_items(&mut self, items: Vec<Item>, cx: &mut Context<Self>) {
        self.items = items;
        self.refilter(cx);
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value();
        let haystacks: Vec<String> = self
            .items
            .iter()
            .map(|i| format!("{} {}", i.label, i.detail.as_deref().unwrap_or("")))
            .collect();
        self.filtered = filter::rank(&query, haystacks.iter().map(String::as_str));
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn close(&self, cx: &mut Context<Self>) {
        let overlay = self.overlay.clone();
        self.state.update(cx, |s, cx| s.close_overlay(&overlay, cx));
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let last = self.filtered.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    /// Close, then run the entry (so focus is back on the shell first).
    fn run(&mut self, filtered_ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(&i) = self.filtered.get(filtered_ix) else {
            return;
        };
        let run = self.items[i].run.clone();
        self.close(cx);
        run(window, cx);
    }

    fn render_item(&self, ix: usize, item: &Item, cx: &Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        div()
            .id(("pick", ix as u64))
            .h(px(ROW_HEIGHT))
            .flex_none()
            .mx_1()
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(6.))
            .cursor_pointer()
            .when(ix == self.selected, |d| d.bg(c.selection))
            .hover(|d| d.bg(c.hover))
            .child(div().size(px(14.)).flex_none().when(item.checked, |d| {
                d.child(ui::icon("icons/check.svg", 14., c.accent))
            }))
            .when_some(item.dot, |d, color| d.child(ui::dot(Some(color), 8.)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(item.label.clone()),
            )
            .when_some(item.detail.clone(), |d, text| {
                d.child(
                    div()
                        .flex_none()
                        .text_size(px(ui::TEXT_XS))
                        .text_color(c.muted_fg)
                        .child(text),
                )
            })
            .when(!item.keys.is_empty(), |d| {
                d.child(ui::key_hint(&item.keys, "", cx))
            })
            .on_click(cx.listener(move |this, _, window, cx| this.run(ix, window, cx)))
    }
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let rows: Vec<_> = self
            .filtered
            .iter()
            .enumerate()
            .map(|(ix, &i)| self.render_item(ix, &self.items[i], cx))
            .collect();
        let panel = div()
            .id("picker")
            .w(px(560.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(10.))
            .border_1()
            .border_color(c.border)
            .bg(c.bg)
            .shadow_lg()
            .text_color(c.fg)
            // Clicks inside must not reach the backdrop, which closes.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px_3()
                    .py_1()
                    .border_b_1()
                    .border_color(c.border)
                    .child(Input::new(&self.input).appearance(false).prefix(ui::icon(
                        "icons/search.svg",
                        14.,
                        c.muted_fg,
                    ))),
            )
            .child(
                div()
                    .id("picker-items")
                    // The list itself is capped: a `max_h` on the panel did not
                    // limit it, and the last row drew past the panel's edge.
                    .max_h(px(ROW_HEIGHT * VISIBLE_ROWS + 8.))
                    .py_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(rows)
                    .when(self.filtered.is_empty(), |d| {
                        d.child(
                            div()
                                .px_4()
                                .py_3()
                                .text_color(c.muted_fg)
                                .child("No matches"),
                        )
                    }),
            );
        deferred(
            div()
                .id("picker-backdrop")
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .justify_center()
                // Not stretched: the panel is as tall as its items.
                .items_start()
                .pt(px(72.))
                .bg(black().opacity(0.25))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.close(cx)),
                )
                .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.move_selection(1, cx)))
                .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.move_selection(-1, cx)))
                .on_action(cx.listener(|this, _: &OpenThread, window, cx| {
                    this.run(this.selected, window, cx)
                }))
                .child(panel),
        )
    }
}

/// The action a `SHORTCUTS` entry runs, by its key text.
fn shortcut_action(keys: &str) -> Option<Box<dyn Action>> {
    Some(match keys {
        "j" => Box::new(SelectNext),
        "k" => Box::new(SelectPrev),
        "Enter" => Box::new(OpenThread),
        "e" => Box::new(Archive),
        "#" => Box::new(Trash),
        "s" => Box::new(ToggleStar),
        "u" => Box::new(ToggleRead),
        "l" => Box::new(PickLabel),
        "v" => Box::new(PickMove),
        "/" => Box::new(FocusSearch),
        "Ctrl+K" => Box::new(CommandPalette),
        "g i" => Box::new(GoInbox),
        "g l" => Box::new(GoLabel),
        "c" => Box::new(ComposeNew),
        "Esc" => Box::new(CloseReader),
        "Ctrl+Z" => Box::new(Undo),
        _ => return None,
    })
}

fn palette_items(state: &Entity<AppState>, cx: &App) -> Vec<Item> {
    let settings = &state.read(cx).settings;
    let mut items: Vec<Item> = actions::SHORTCUTS
        .iter()
        .filter_map(|&(keys, text)| {
            let action = shortcut_action(keys)?;
            Some(
                Item::new(text, move |window, cx| {
                    window.dispatch_action(action.boxed_clone(), cx)
                })
                .keys(keys.split(['+', ' ']).collect()),
            )
        })
        .collect();
    for (pref, name) in [
        (ThemePref::System, "System"),
        (ThemePref::Light, "Light"),
        (ThemePref::Dark, "Dark"),
    ] {
        let state = state.clone();
        items.push(
            Item::new(format!("Theme: {name}"), move |window, cx| {
                state.update(cx, |s, cx| s.set_theme(pref, window, cx))
            })
            .checked(settings.theme == pref),
        );
    }
    for (label, fonts) in [
        ("Theme…", false),
        ("Accent colour…", false),
        ("Fonts…", true),
    ] {
        let state = state.clone();
        items.push(Item::new(label, move |_, cx| {
            state.update(cx, |s, cx| s.push_overlay(Overlay::Appearance { fonts }, cx))
        }));
    }
    {
        let state = state.clone();
        let accounts = state.clone();
        items.push(Item::new("Accounts and sync…", move |_, cx| {
            accounts.update(cx, |s, cx| s.push_overlay(Overlay::Accounts, cx))
        }));
        items.push(Item::new("Add account…", move |_, cx| {
            state.update(cx, |s, cx| s.add_account(cx))
        }));
    }
    for a in &state.read(cx).accounts {
        let (id, state) = (a.id, state.clone());
        items.push(
            Item::new(format!("Sign out: {}", a.email), move |_, cx| {
                state.update(cx, |s, cx| s.sign_out(id, cx))
            })
            .dot(Some(format::parse_color(&a.color))),
        );
    }
    for n in PREFETCH_CHOICES {
        let state = state.clone();
        items.push(
            Item::new(format!("Prefetch: {n}"), move |_, cx| {
                state.update(cx, |s, cx| s.set_prefetch(n, cx))
            })
            .checked(settings.prefetch == n),
        );
    }
    for (preset, name) in [
        (Preset::Tide, "Tide"),
        (Preset::Command, "Command"),
        (Preset::Paper, "Paper"),
    ] {
        let state = state.clone();
        items.push(
            Item::new(format!("Layout: {name}"), move |window, cx| {
                state.update(cx, |s, cx| s.set_preset(preset, window, cx))
            })
            .checked(settings.preset == preset),
        );
    }
    items
}

/// User labels of the thread's account; those in `on` are on the thread now.
fn label_items(
    state: &Entity<AppState>,
    thread_id: i64,
    account_id: i64,
    on: &HashSet<String>,
    cx: &App,
) -> Vec<Item> {
    let s = state.read(cx);
    s.labels
        .iter()
        .filter(|l| !l.is_system && l.account_id == account_id)
        .map(|l| {
            let has = on.contains(&l.label_key);
            let (kind, text) = if has {
                (
                    ActionKind::RemoveLabel {
                        label_key: l.label_key.clone(),
                    },
                    format!("Removed label {}", l.name),
                )
            } else {
                (
                    ActionKind::AddLabel {
                        label_key: l.label_key.clone(),
                    },
                    format!("Added label {}", l.name),
                )
            };
            let state = state.clone();
            Item::new(l.name.clone(), move |_, cx| {
                state.update(cx, |s, cx| {
                    s.apply(kind.clone(), vec![thread_id], Some(text.clone().into()), cx)
                })
            })
            .checked(has)
            .dot(l.color.as_deref().map(format::parse_color))
        })
        .collect()
}

fn move_items(state: &Entity<AppState>, thread_id: i64, account_id: i64, cx: &App) -> Vec<Item> {
    let s = state.read(cx);
    s.labels
        .iter()
        .filter(|l| !l.is_system && l.account_id == account_id)
        .map(|l| {
            let (key, text) = (l.label_key.clone(), format!("Moved to {}", l.name));
            let state = state.clone();
            Item::new(l.name.clone(), move |_, cx| {
                state.update(cx, |s, cx| {
                    let kind = ActionKind::Move {
                        label_key: key.clone(),
                    };
                    s.apply(kind, vec![thread_id], Some(text.clone().into()), cx)
                })
            })
            .dot(l.color.as_deref().map(format::parse_color))
        })
        .collect()
}

/// Every mailbox and label, unified entries first as in the navigation.
fn go_items(state: &Entity<AppState>, cx: &App) -> Vec<Item> {
    let s = state.read(cx);
    let all = nav::items(s);
    let go = |nav: Nav, label: String, active: bool, dot: Option<Hsla>| {
        let state = state.clone();
        Item::new(label, move |_, cx| {
            state.update(cx, |s, cx| s.navigate(nav.clone(), cx))
        })
        .checked(active)
        .dot(dot)
    };
    let mut items: Vec<Item> = all
        .mailboxes
        .into_iter()
        .map(|i| go(i.nav.clone(), i.nav.title.to_string(), i.active, None))
        .collect();
    for (key, title) in [("TRASH", "Trash"), ("SPAM", "Spam")] {
        if s.labels.iter().any(|l| l.label_key == key) {
            let nav = nav::nav(key, None, title);
            let active = s.nav == nav;
            items.push(go(nav, title.into(), active, None));
        }
    }
    for a in all.accounts {
        items.push(go(
            a.item.nav.clone(),
            format!("Inbox: {}", a.item.nav.title),
            a.item.active,
            a.item.color,
        ));
    }
    for l in all.labels {
        let account = l
            .nav
            .view
            .account_id
            .and_then(|id| s.account(id))
            .map(|a| a.email.clone());
        let item = go(l.nav.clone(), l.nav.title.to_string(), l.active, l.color);
        items.push(match account {
            Some(email) => item.detail(email),
            None => item,
        });
    }
    items
}

/// Build the picker for an overlay; `None` for overlays that are not pickers.
pub fn open(
    overlay: &Overlay,
    state: &Entity<AppState>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<Picker>> {
    let (placeholder, items) = match overlay {
        Overlay::Palette => ("Type a command…", palette_items(state, cx)),
        Overlay::LabelPicker {
            thread_id,
            account_id,
        } => (
            "Label this thread…",
            label_items(state, *thread_id, *account_id, &HashSet::new(), cx),
        ),
        Overlay::MovePicker {
            thread_id,
            account_id,
        } => ("Move to…", move_items(state, *thread_id, *account_id, cx)),
        Overlay::GoPicker => ("Go to label…", go_items(state, cx)),
        Overlay::Toast | Overlay::Appearance { .. } | Overlay::Accounts => return None,
    };
    let picker = cx.new(|cx| {
        Picker::new(
            overlay.clone(),
            placeholder,
            items,
            state.clone(),
            window,
            cx,
        )
    });

    if let Overlay::LabelPicker {
        thread_id,
        account_id,
    } = *overlay
    {
        // Which labels are on the thread now: marks them, makes a pick remove.
        let task = state.read(cx).db.read(cx, move |conn| {
            let mut st =
                conn.prepare_cached("SELECT label_key FROM thread_labels WHERE thread_id = ?1")?;
            let mut keys = HashSet::new();
            for key in st.query_map([thread_id], |r| r.get::<_, String>(0))? {
                keys.insert(key?);
            }
            Ok(keys)
        });
        let (state, picker) = (state.clone(), picker.clone());
        cx.spawn(async move |cx| match task.await {
            Ok(on) => {
                picker.update(cx, |p, cx| {
                    let items = label_items(&state, thread_id, account_id, &on, cx);
                    p.set_items(items, cx)
                });
            }
            Err(e) => eprintln!("tidemail: reading thread labels failed: {e}"),
        })
        .detach();
    }
    Some(picker)
}

#[cfg(test)]
mod tests {
    use super::shortcut_action;
    use crate::actions::SHORTCUTS;

    #[test]
    fn every_shortcut_has_an_action() {
        for (keys, _) in SHORTCUTS {
            assert!(shortcut_action(keys).is_some(), "no action for {keys}");
        }
    }
}
