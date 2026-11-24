//! Reader pane: native header, toolbar, message strip and attachments; the
//! message body is one warm WebView2 (`host.rs`) with scripts off.

mod host;
mod page;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use futures::StreamExt;
use gpui_kit::component::ActiveTheme;
use gpui_kit::{prelude::FluentBuilder, *};
use tidemail_db::{
    read, types::Body, types::MessageMeta, types::SendState, types::ThreadDetail, write,
};

use crate::{
    actions::{Archive, PickLabel, PickMove, ToggleStar, Trash},
    fonts::Fonts,
    format,
    preset::ReaderPlacement,
    state::{AppState, StateEvent},
    theme::colors,
    ui,
};
use host::Host;

/// Widest the message column gets, in px.
const COLUMN_MAX: f32 = 760.0;

/// Body of the expanded message, with what the header needs to know.
struct BodyView {
    body: Option<Body>,
    remote_images: bool,
}

pub struct Reader {
    state: Entity<AppState>,
    thread: Option<ThreadDetail>,
    /// Message shown in full (default: the last).
    expanded: Option<i64>,
    body: Option<BodyView>,
    host: Option<Host>,
    /// The webview could not be created: bodies show as plain text.
    host_failed: bool,
    /// The body area shows the webview (else native text).
    html_mode: bool,
    /// What the webview holds: message, images allowed, hash of the HTML.
    key: Option<(i64, bool, u64)>,
    /// Nonce of the last `load_html`.
    nonce: u64,
    /// Document being loaded: nonce and thread.
    pending: Option<(u64, i64)>,
    /// Thread whose content is on screen.
    shown: Option<i64>,
    /// Attachment progress or error.
    notice: Option<SharedString>,
    loading: Option<Task<()>>,
    download: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Reader {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sub = cx.subscribe(&state, |this, _, event, cx| match event {
            StateEvent::SelectionChanged => this.load(cx),
            StateEvent::DataChanged { thread_ids, .. }
                if this
                    .state
                    .read(cx)
                    .selected
                    .is_some_and(|id| thread_ids.contains(&id)) =>
            {
                this.load(cx)
            }
            StateEvent::ReadingChanged
            | StateEvent::OverlaysChanged
            | StateEvent::ComposeChanged => {
                this.sync_visibility(cx);
                cx.notify();
            }
            // Preset or theme: the page colours live in the document head.
            StateEvent::SettingsChanged => {
                if let Some(host) = &mut this.host {
                    host.images = None;
                }
                this.key = None;
                this.present(cx);
            }
            _ => {}
        });
        // WebView2 start costs; keep it off the cold-launch path: after frame 2.
        let this = cx.weak_entity();
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                let _ = this.update(cx, |this, cx| this.create_host(window, cx));
            })
        });
        Self {
            state,
            thread: None,
            expanded: None,
            body: None,
            host: None,
            host_failed: false,
            html_mode: false,
            key: None,
            nonce: 0,
            pending: None,
            shown: None,
            notice: None,
            loading: None,
            download: None,
            _subscriptions: vec![sub],
        }
    }

    /// Thread whose content is on screen: the page finished loading (bench:
    /// open-to-paint includes the body).
    pub fn shown(&self) -> Option<i64> {
        self.shown
    }

    fn create_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let db = self.state.read(cx).db.clone();
        let paper = colors(cx).paper;
        let rgb: gpui_kit::Rgba = paper.into();
        let background = [rgb.r, rgb.g, rgb.b].map(|v| (v * 255.0).round() as u8);
        let parent = crate::win::hwnd(window);
        let executor = cx.background_executor().clone();
        // WebView2 creation pumps messages: build it outside any App borrow
        // (see `host.rs`), then attach it in an update.
        cx.spawn_in(window, async move |this, cx| {
            let built = host::build(parent, db, background, executor);
            let _ = this.update_in(cx, |this, window, cx| this.attach_host(built, window, cx));
        })
        .detach();
    }

    fn attach_host(
        &mut self,
        built: wry::Result<(wry::WebView, host::LoadedTx, host::Loaded)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (webview, tx, mut loaded) = match built {
            Ok(built) => built,
            Err(e) => {
                eprintln!("tidemail: reader webview unavailable: {e}");
                self.host_failed = true;
                self.present(cx);
                return;
            }
        };
        let webview = gpui_webview::WebView::new(webview, window, cx);
        let host = Host {
            view: cx.new(|_| webview),
            applied: Default::default(),
            loaded: tx,
            images: None,
        };
        let _ = host.view.update(cx, |v, _| v.set_visible(false));
        // Page-load events arrive on the UI thread; turn them into entity updates.
        cx.spawn(async move |this, cx| {
            while let Some(nonce) = loaded.next().await {
                let _ = this.update(cx, |this, cx| {
                    if let Some((n, thread)) = this.pending.filter(|(n, _)| *n == nonce) {
                        debug_assert_eq!(n, nonce);
                        this.pending = None;
                        this.shown = Some(thread);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        self.host = Some(host);
        // Content that arrived before the host existed.
        self.key = None;
        self.present(cx);
    }

    /// Load the selected thread and the body of the message to show.
    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.state.read(cx).selected else {
            self.thread = None;
            self.expanded = None;
            self.body = None;
            self.shown = None;
            self.pending = None;
            self.loading = None;
            self.html_mode = false;
            self.sync_visibility(cx);
            cx.notify();
            return;
        };
        let keep = self
            .thread
            .as_ref()
            .filter(|t| t.thread.id == id)
            .and(self.expanded);
        let engine = self.state.read(cx).engine.clone();
        let task = self.state.read(cx).db.read(cx, move |c| {
            let Some(detail) = read::get_thread(c, id)? else {
                return Ok(None);
            };
            let message = keep
                .filter(|m| detail.messages.iter().any(|x| x.id == *m))
                .or(detail.messages.last().map(|m| m.id));
            let body = match message {
                Some(m) => Some(load_body(c, m, &engine)?),
                None => None,
            };
            Ok(Some((detail, message, body)))
        });
        self.loading = Some(cx.spawn(async move |this, cx| {
            let loaded = task.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = None;
                if let Ok(Some((detail, message, body))) = loaded {
                    this.thread = Some(detail);
                    this.expanded = message;
                    this.body = body;
                    this.notice = None;
                    this.present(cx);
                }
            });
        }));
    }

    /// Show another message of the thread in full.
    fn expand(&mut self, message: i64, cx: &mut Context<Self>) {
        if self.expanded == Some(message) {
            return;
        }
        self.expanded = Some(message);
        self.notice = None;
        self.reload_body(cx);
    }

    fn reload_body(&mut self, cx: &mut Context<Self>) {
        let Some(message) = self.expanded else { return };
        let engine = self.state.read(cx).engine.clone();
        let task = self
            .state
            .read(cx)
            .db
            .read(cx, move |c| load_body(c, message, &engine));
        self.loading = Some(cx.spawn(async move |this, cx| {
            let loaded = task.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = None;
                if let (Ok(body), true) = (loaded, this.expanded == Some(message)) {
                    this.body = Some(body);
                    this.present(cx);
                }
            });
        }));
    }

    /// Put the loaded body on screen: into the webview, or as native text.
    fn present(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.thread.as_ref().map(|t| t.thread.id) else {
            return;
        };
        let (message, load_images, html) = match (&self.expanded, &self.body) {
            (Some(m), Some(b)) => (
                *m,
                b.body.as_ref().is_some_and(|b| b.load_images),
                b.body.as_ref().and_then(|b| b.html.clone()),
            ),
            _ => (0, false, None),
        };
        match html {
            Some(html) if !self.host_failed => {
                self.html_mode = true;
                let mut hasher = DefaultHasher::new();
                html.hash(&mut hasher);
                let key = (message, load_images, hasher.finish());
                // Same content again (a data-changed burst): keep what is shown.
                if self.key != Some(key) {
                    self.key = Some(key);
                    self.nonce += 1;
                    self.pending = Some((self.nonce, thread_id));
                    if let Some(host) = self.host.as_mut().filter(|h| h.images == Some(load_images)) {
                        let (tx, nonce) = (host.loaded.clone(), self.nonce);
                        let script = page::swap_script(&html, nonce);
                        let swapped = host.view.read(cx).raw().evaluate_script_with_callback(
                            &script,
                            move |title| {
                                // JSON of document.title: the nonce once the body is in.
                                if title.trim_matches('"') == nonce.to_string() {
                                    let _ = tx.unbounded_send(nonce);
                                }
                            },
                        );
                        if let Err(e) = swapped {
                            eprintln!("tidemail: reader body swap failed: {e}");
                            host.images = None;
                        }
                    }
                    if let Some(host) = self.host.as_mut().filter(|h| h.images != Some(load_images)) {
                        host.images = Some(load_images);
                        let c = colors(cx);
                        let code = Fonts::from_settings(&self.state.read(cx).settings).code();
                        let doc = page::document(
                            &html,
                            self.nonce,
                            load_images,
                            &page::css_color(c.paper),
                            &page::css_color(c.paper_fg),
                            ui::TEXT_LG,
                            code.family,
                        );
                        if let Err(e) = host.view.read(cx).load_html(&doc) {
                            eprintln!("tidemail: reader load_html failed: {e}");
                            self.html_mode = false;
                            self.key = None;
                            self.pending = None;
                            self.shown = Some(thread_id);
                        }
                    }
                }
            }
            _ => {
                self.html_mode = false;
                self.key = None;
                self.pending = None;
                self.shown = Some(thread_id);
            }
        }
        self.sync_visibility(cx);
        cx.notify();
    }

    /// The OS draws the webview above GPUI: show it only where the reader is
    /// on screen and nothing is drawn over it.
    fn sync_visibility(&self, cx: &mut Context<Self>) {
        let Some(host) = &self.host else { return };
        let state = self.state.read(cx);
        let on = self.html_mode
            && state.selected.is_some()
            && !state.overlay_open()
            && !state.composing
            && (state.spec().reader == ReaderPlacement::Split || state.reading);
        host.view.update(cx, |v, _| {
            if v.visible() != on {
                let _ = v.set_visible(on);
            }
        });
    }

    fn open_attachment(&mut self, id: i64, name: &str, cx: &mut Context<Self>) {
        let engine = self.state.read(cx).engine.clone();
        self.notice = Some(format!("Downloading {name}…").into());
        self.download = Some(cx.spawn(async move |this, cx| {
            let result = engine.download_attachment(id).await;
            let _ = this.update(cx, |this, cx| {
                this.notice = match result {
                    Ok(path) => {
                        cx.open_with_system(&path);
                        None
                    }
                    Err(e) => Some(format!("Download failed. {}", crate::format::user_error(&e)).into()),
                };
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Allow remote images from this sender, then show the body again.
    fn allow_images(&mut self, email: String, cx: &mut Context<Self>) {
        let task = self
            .state
            .read(cx)
            .db
            .read(cx, move |c| write::set_sender_images(c, &email, true));
        cx.spawn(async move |this, cx| {
            if task.await.is_ok() {
                let _ = this.update(cx, |this, cx| this.reload_body(cx));
            }
        })
        .detach();
    }
}

/// Body of a message and whether its HTML refers to remote images. Asks the
/// engine to fetch a missing body (it arrives as `DataChanged`).
fn load_body(
    c: &tidemail_db::Connection,
    message: i64,
    engine: &tidemail_sync::Engine,
) -> tidemail_db::Result<BodyView> {
    let body = read::get_body(c, message)?;
    if body.is_none() {
        engine.prefetch_bodies(&[message]);
    }
    let remote_images = body
        .as_ref()
        .and_then(|b| b.html.as_deref())
        .is_some_and(page::has_remote_images);
    Ok(BodyView {
        body,
        remote_images,
    })
}

fn sender(m: &MessageMeta) -> &str {
    if m.from_name.is_empty() {
        &m.from_email
    } else {
        &m.from_name
    }
}

impl Reader {
    fn toolbar(&self, starred: bool, cx: &mut Context<Self>) -> impl IntoElement {
        macro_rules! button {
            ($id:literal, $icon:expr, $tip:literal, $action:expr) => {
                ui::icon_button($id, $icon, $tip, cx).on_click(cx.listener(
                    |_, _: &ClickEvent, window, cx| window.dispatch_action(Box::new($action), cx),
                ))
            };
        }
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_0p5()
            .child(button!("archive", "icons/archive.svg", "Archive  e", Archive))
            .child(button!("trash", "icons/trash.svg", "Trash  #", Trash))
            .child(button!(
                "star",
                if starred {
                    "icons/star-fill.svg"
                } else {
                    "icons/star.svg"
                },
                "Star  s",
                ToggleStar
            ))
            .child(button!("label", "icons/tag.svg", "Label  l", PickLabel))
            .child(button!(
                "move",
                "icons/folder-input.svg",
                "Move to  v",
                PickMove
            ))
    }

    fn header(&self, detail: &ThreadDetail, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let subject: SharedString = if detail.thread.subject.is_empty() {
            "(no subject)".into()
        } else {
            detail.thread.subject.clone().into()
        };
        let chips: Vec<_> = detail
            .label_keys
            .iter()
            .filter_map(|k| state.labels.iter().find(|l| &l.label_key == k))
            .filter(|l| !l.is_system)
            .map(|l| {
                let color = match &l.color {
                    Some(c) => format::parse_color(c),
                    None => colors(cx).muted_fg,
                };
                ui::chip(l.name.clone(), color)
            })
            .collect();
        let display_font = state.display_font();
        div()
            .flex_none()
            .px_4()
            .pt_3()
            .pb_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(ui::TEXT_XL))
                            .font_weight(FontWeight::SEMIBOLD)
                            .when_some(display_font, |d, f| d.font_family(f))
                            .child(subject),
                    )
                    .child(self.toolbar(detail.thread.is_starred, cx)),
            )
            .when(!chips.is_empty(), |d| {
                d.child(div().flex().flex_wrap().gap_1().children(chips))
            })
    }

    /// One line per message other than the open one; click opens it.
    fn strip(&self, detail: &ThreadDetail, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        div()
            .id("strip")
            .flex_none()
            .max_h(relative(0.3))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .children(
                detail
                    .messages
                    .iter()
                    .filter(|m| Some(m.id) != self.expanded)
                    .map(|m| {
                        let id = m.id;
                        div()
                            .id(("msg", id as usize))
                            .h(px(28.))
                            .flex_none()
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded(cx.theme().radius)
                            .cursor_pointer()
                            .hover(|d| d.bg(c.hover))
                            .text_size(px(ui::TEXT_SM))
                            .on_click(cx.listener(move |this, _, _, cx| this.expand(id, cx)))
                            .child(
                                div()
                                    .w(px(140.))
                                    .flex_none()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(sender(m).to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(c.muted_fg)
                                    .child(m.snippet.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(c.muted_fg)
                                    .child(format::list_date(m.internal_date)),
                            )
                    }),
            )
    }

    /// Sender line, "Load images" and attachments of the open message.
    fn meta(&self, m: &MessageMeta, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let show_images = self
            .body
            .as_ref()
            .is_some_and(|b| b.remote_images && !b.body.as_ref().is_some_and(|b| b.load_images));
        let email = m.from_email.clone();
        let attachments: Vec<_> = m.attachments.iter().filter(|a| !a.is_inline).collect();
        div()
            .flex_none()
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(ui::avatar(sender(m), 28.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("{} <{}>", sender(m), m.from_email)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(ui::TEXT_SM))
                                    .text_color(c.muted_fg)
                                    .child(format!("to {}", m.to_text)),
                            ),
                    )
                    .when_some(m.send_state, |d, state| {
                        let (text, color) = match state {
                            SendState::Sending => ("Sending…", c.muted_fg),
                            SendState::Queued => ("Queued (offline)", c.warning),
                            SendState::Failed => ("Not sent", c.danger),
                        };
                        d.child(ui::chip(text, color))
                    })
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(ui::TEXT_SM))
                            .text_color(c.muted_fg)
                            .child(format::list_date(m.internal_date)),
                    ),
            )
            .when(show_images, |d| {
                d.child(
                    div()
                        .id("load-images")
                        .flex_none()
                        .self_start()
                        .px_2()
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(c.border)
                        .cursor_pointer()
                        .hover(|d| d.bg(c.hover))
                        .text_size(px(ui::TEXT_SM))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.allow_images(email.clone(), cx)
                        }))
                        .child("Load images from this sender"),
                )
            })
            .when(!attachments.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(attachments.into_iter().map(|a| {
                            let (id, name) = (a.id, a.filename.clone());
                            div()
                                .id(("att", id as usize))
                                .h(px(26.))
                                .px_2()
                                .flex()
                                .items_center()
                                .gap_1()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(c.border)
                                .cursor_pointer()
                                .hover(|d| d.bg(c.hover))
                                .text_size(px(ui::TEXT_SM))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_attachment(id, &name, cx)
                                }))
                                .child(ui::icon("icons/paperclip.svg", 14., c.muted_fg))
                                .child(a.filename.clone())
                                .child(
                                    div()
                                        .text_color(c.muted_fg)
                                        .child(page::size(a.size)),
                                )
                        })),
                )
            })
            .when_some(self.notice.clone(), |d, n| {
                d.child(
                    div()
                        .text_size(px(ui::TEXT_SM))
                        .text_color(c.muted_fg)
                        .child(n),
                )
            })
    }

    /// The webview, or native text when the message has no HTML.
    fn body_area(&self, cx: &App) -> impl IntoElement {
        let c = colors(cx);
        let area = div()
            .flex_1()
            .min_h_0()
            .rounded(cx.theme().radius)
            .overflow_hidden()
            // Paper is close to the light backgrounds (Paper light): an edge.
            .border_1()
            .border_color(c.border)
            .bg(c.paper)
            .text_color(c.paper_fg);
        match (&self.host, self.html_mode) {
            (Some(host), true) => {
                // Our own element instead of `gpui_webview`'s: that one sets the
                // WebView2 bounds and visibility on every frame (costly while the
                // list scrolls). Visibility is `sync_visibility`'s job.
                let (view, applied) = (host.view.clone(), host.applied.clone());
                area.child(
                    canvas(
                        move |bounds, _, cx| {
                            if applied.replace(bounds) != bounds {
                                let rect = wry::Rect {
                                    position: wry::dpi::LogicalPosition::new(
                                        f64::from(bounds.origin.x),
                                        f64::from(bounds.origin.y),
                                    )
                                    .into(),
                                    size: wry::dpi::LogicalSize::new(
                                        f64::from(bounds.size.width),
                                        f64::from(bounds.size.height),
                                    )
                                    .into(),
                                };
                                let _ = view.read(cx).set_bounds(rect);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .size_full(),
                )
            }
            _ => {
                let text = match self.body.as_ref().and_then(|b| b.body.as_ref()) {
                    Some(b) if !b.text.is_empty() => b.text.clone(),
                    Some(_) => String::new(),
                    None => "Loading message…".to_string(),
                };
                let settings = &self.state.read(cx).settings;
                // Plain-text mail is often code, logs or keys: the code font
                // (and with it "Hide secrets in code"), unless switched off.
                let mono = settings
                    .plain_text_mono
                    .then(|| Fonts::from_settings(settings).code().family);
                area.child(
                    div()
                        .id("text-body")
                        .size_full()
                        .overflow_y_scroll()
                        .p_4()
                        .when_some(mono, |d, family| {
                            d.font_family(family).text_size(px(ui::TEXT_MONO))
                        })
                        .when(mono.is_none(), |d| d.text_size(px(ui::TEXT_LG)))
                        .child(text),
                )
            }
        }
    }
}

impl Render for Reader {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(detail) = self.thread.clone() else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(colors(cx).muted_fg)
                .child("Select a thread")
                .into_any_element();
        };
        let open = detail
            .messages
            .iter()
            .find(|m| Some(m.id) == self.expanded)
            .cloned();
        // Header and body share one centred column (Command's reader is full width).
        div()
            .size_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .size_full()
                    .max_w(px(COLUMN_MAX + 32.))
                    .flex()
                    .flex_col()
                    .child(self.header(&detail, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .px_4()
                            .pb_4()
                            .flex()
                            .flex_col()
                            .child(self.strip(&detail, cx))
                            .children(open.map(|m| self.meta(&m, cx)))
                            .child(self.body_area(cx)),
                    ),
            )
            .into_any_element()
    }
}
