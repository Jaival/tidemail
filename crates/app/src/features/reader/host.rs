//! The WebView2 host: one instance, scripts off, locked down. Created once
//! after the first frame and reused for every message.
//!
//! WebView2 creation runs a nested message loop (`wait_with_pump`), so
//! [`build`] must run while no GPUI `App` borrow is held (in a spawned task,
//! between updates): a GPUI task dispatched by that loop would panic.

use std::{cell::Cell, rc::Rc, sync::Arc};

use futures::channel::mpsc::{unbounded, UnboundedReceiver};
use gpui_kit::BackgroundExecutor;
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use windows_sys::Win32::Foundation::HWND;
use webview2_com::FocusChangedEventHandler;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use wry::{
    http::{header, Response},
    NewWindowResponse, PageLoadEvent, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows,
};

use super::page;
use crate::data::Db;

pub struct Host {
    pub view: gpui_kit::Entity<gpui_webview::WebView>,
    /// Bounds last given to WebView2 (set only when they change).
    pub applied: std::rc::Rc<Cell<gpui_kit::Bounds<gpui_kit::Pixels>>>,
}

/// The main window, as the webview's parent.
struct Parent(HWND);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = std::num::NonZeroIsize::new(self.0 as isize).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(hwnd));
        // SAFETY: the main window outlives the webview (both live until exit).
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

/// Nonces (`<title>`) of documents whose page load finished.
pub type Loaded = UnboundedReceiver<u64>;

pub fn build(
    parent: HWND,
    db: Arc<Db>,
    background: [u8; 3],
    executor: BackgroundExecutor,
) -> wry::Result<(wry::WebView, Loaded)> {
    let (tx, loaded) = unbounded();
    // Title events come before the page-load event of the same document.
    let title = Rc::new(Cell::new(0u64));
    let title_seen = title.clone();

    let builder = WebViewBuilder::new()
        .with_javascript_disabled()
        .with_devtools(false)
        .with_hotkeys_zoom(false)
        .with_default_context_menus(false)
        .with_browser_accelerator_keys(false)
        .with_drag_drop_handler(|_| true)
        .with_background_color((background[0], background[1], background[2], 255))
        .with_navigation_handler(|url| {
            // The reader document itself (`NavigateToString`) and nothing else.
            if url.starts_with("data:") || url == "about:blank" {
                return true;
            }
            if page::is_external(&url) {
                let _ = tidemail_gmail::open_in_browser(&url);
            }
            false
        })
        .with_new_window_req_handler(|url, _| {
            if page::is_external(&url) {
                let _ = tidemail_gmail::open_in_browser(&url);
            }
            NewWindowResponse::Deny
        })
        .with_document_title_changed_handler(move |t| {
            title_seen.set(t.parse().unwrap_or(0));
        })
        .with_on_page_load_handler(move |event, _| {
            if matches!(event, PageLoadEvent::Finished) {
                let _ = tx.unbounded_send(title.get());
            }
        })
        .with_asynchronous_custom_protocol(
            tidemail_render::CID_SCHEME.into(),
            move |_, request, responder| {
                let parsed = page::parse_cid_path(request.uri().path());
                let db = db.clone();
                executor
                    .spawn(async move {
                        let found = parsed.and_then(|(message, cid)| {
                            db.with(|c| tidemail_render::load_cid(c, message, &cid))
                                .ok()
                                .flatten()
                        });
                        let response = match found {
                            Some((mime, data)) => Response::builder()
                                .header(header::CONTENT_TYPE, mime)
                                .header(header::CACHE_CONTROL, "private, max-age=3600")
                                .body(data),
                            None => Response::builder().status(404).body(Vec::new()),
                        };
                        if let Ok(response) = response {
                            responder.respond(response);
                        }
                    })
                    .detach();
            },
        );

    let webview = builder.build_as_child(&Parent(parent))?;
    // A click in the page moves Win32 focus into WebView2; give it back so
    // j/k and the shortcuts keep working in GPUI.
    let mut token = 0i64;
    unsafe {
        let handler = FocusChangedEventHandler::create(Box::new(move |_, _| {
            SetFocus(parent);
            Ok(())
        }));
        webview.controller().add_GotFocus(&handler, &mut token)?;
    }
    Ok((webview, loaded))
}
