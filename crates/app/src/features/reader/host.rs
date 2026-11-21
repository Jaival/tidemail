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
use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};
use wry::{
    http::{header, Response},
    NewWindowResponse, PageLoadEvent, WebContext, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows,
};

use super::page;
use crate::data::Db;

pub struct Host {
    pub view: gpui_kit::Entity<gpui_webview::WebView>,
    pub loaded: LoadedTx,
    /// Image policy (CSP) of the document in the webview; `None` before the
    /// first navigation. Bodies with the same policy are swapped in place.
    pub images: Option<bool>,
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
/// Sends a nonce into [`Loaded`] (body swaps report through it too).
pub type LoadedTx = futures::channel::mpsc::UnboundedSender<u64>;

pub fn build(
    parent: HWND,
    db: Arc<Db>,
    background: [u8; 3],
    executor: BackgroundExecutor,
) -> wry::Result<(wry::WebView, LoadedTx, Loaded)> {
    let (tx, loaded) = unbounded();
    let swapped = tx.clone();
    // Title events come before the page-load event of the same document.
    let title = Rc::new(Cell::new(0u64));
    let title_seen = title.clone();

    // WebView2 user data (cache, cookies) under the app folder, not next to
    // the exe; kept between runs. One folder for every database.
    let data = std::env::var_os("LOCALAPPDATA")
        .map_or_else(|| std::path::PathBuf::from("."), std::path::PathBuf::from)
        .join("Tidemail")
        .join("WebView2");
    let mut context = WebContext::new(Some(data));
    let builder = WebViewBuilder::new_with_web_context(&mut context)
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
                // The code font for `code` / `pre` (page::document). The page
                // has an opaque origin and font loads are CORS requests.
                if let Some(family) = page::parse_font_path(request.uri().path()) {
                    // The first file is the regular weight (or the variable font).
                    let response = match crate::fonts::by_family(&family) {
                        Some(font) => Response::builder()
                            .header(header::CONTENT_TYPE, "font/ttf")
                            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .header(header::CACHE_CONTROL, "private, max-age=86400")
                            .body(font.files[0].to_vec()),
                        None => Response::builder().status(404).body(Vec::new()),
                    };
                    if let Ok(response) = response {
                        responder.respond(response);
                    }
                    return;
                }
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
    // j/k and the shortcuts keep working in GPUI. Not inside the handler:
    // WebView2 takes focus again after it returns (checked by screenshot), so
    // a zero-delay thread timer does it once the click is handled.
    FOCUS_PARENT.set(parent);
    let mut token = 0i64;
    unsafe {
        let handler = FocusChangedEventHandler::create(Box::new(move |_, _| {
            SetTimer(std::ptr::null_mut(), 0, 0, Some(refocus));
            Ok(())
        }));
        webview.controller().add_GotFocus(&handler, &mut token)?;
    }
    Ok((webview, swapped, loaded))
}

thread_local! {
    /// The GPUI window the reader lives in (one per process).
    static FOCUS_PARENT: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
}

/// Thread-timer callback: focus back to the GPUI window, once.
unsafe extern "system" fn refocus(_: HWND, _: u32, id: usize, _: u32) {
    unsafe {
        KillTimer(std::ptr::null_mut(), id);
        SetFocus(FOCUS_PARENT.get());
    }
}
