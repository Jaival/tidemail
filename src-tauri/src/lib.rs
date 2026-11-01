pub mod commands;
pub mod events;
mod tray;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};

use tauri::{http, webview::NewWindowResponse, AppHandle, Manager, Url};
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;
use tidemail_db::Connection;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub db_path: PathBuf,
    pub engine: tidemail_sync::Engine,
}

/// Set at the top of `run()`; cold launch is measured from here.
static PROCESS_START: OnceLock<Instant> = OnceLock::new();

/// Milliseconds since `run()` started.
pub fn ms_since_start() -> f64 {
    PROCESS_START
        .get()
        .map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0)
}

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            commands::list_threads,
            commands::get_thread,
            commands::get_body,
            commands::list_accounts,
            commands::list_labels,
            commands::report_first_paint,
            commands::apply_action,
            commands::undo_action,
            commands::retry_failed,
            commands::dismiss_failed,
            commands::search,
            commands::get_sync_status,
            commands::add_account,
            commands::remove_account,
            commands::prefetch_bodies,
            commands::download_attachment
        ])
        .events(tauri_specta::collect_events![
            events::DataChanged,
            events::SyncStatusChanged
        ])
        .dangerously_cast_bigints_to_number()
}

fn bindings_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Tauri crate must live in the workspace")
        .join("src/bindings.ts")
}

pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    specta_builder().export(specta_typescript::Typescript::default(), bindings_path())?;
    Ok(())
}

fn database_path() -> PathBuf {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--db" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
    base.join("Tidemail").join("mail.db")
}

struct AppNotifier(tauri::AppHandle);

impl tidemail_sync::Notifier for AppNotifier {
    fn data_changed(&self, thread_ids: Vec<i64>, label_keys: Vec<String>) {
        let _ = events::DataChanged {
            thread_ids,
            label_keys,
        }
        .emit(&self.0);
    }
    fn status_changed(&self, status: tidemail_db::types::SyncStatus) {
        let _ = events::SyncStatusChanged { status }.emit(&self.0);
    }
}

/// The app's own pages: the bundled frontend, or the dev server in dev.
fn is_app_url(url: &Url, dev_url: Option<&Url>) -> bool {
    url.host_str() == Some("tauri.localhost")
        || url.scheme() == "about"
        || dev_url.is_some_and(|d| d.origin() == url.origin())
}

/// Open a link from mail in the default app. Other schemes are dropped.
fn open_external(app: &AppHandle, url: &Url) {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        let _ = app.opener().open_url(url.as_str(), None::<&str>);
    }
}

/// `http://tidemail-cid.localhost/<message_id>/<percent-encoded content_id>`.
fn cid_response(app: &AppHandle, path: &str) -> http::Response<Vec<u8>> {
    let not_found = || {
        http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .body(Vec::new())
            .expect("static response")
    };
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    let (Some(msg), Some(cid)) = (parts.next(), parts.next()) else {
        return not_found();
    };
    let Ok(message_id) = msg.parse::<i64>() else {
        return not_found();
    };
    let Ok(content_id) = percent_encoding::percent_decode_str(cid).decode_utf8() else {
        return not_found();
    };
    let Some(state) = app.try_state::<AppState>() else {
        return not_found();
    };
    let found = {
        let Ok(db) = state.db.lock() else {
            return not_found();
        };
        tidemail_render::load_cid(&db, message_id, &content_id)
    };
    match found {
        Ok(Some((mime, data))) => http::Response::builder()
            .header(http::header::CONTENT_TYPE, mime)
            .header(http::header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(data)
            .expect("valid response"),
        _ => not_found(),
    }
}

pub fn run() {
    PROCESS_START.get_or_init(Instant::now);
    let specta = specta_builder();

    #[cfg(debug_assertions)]
    specta
        .export(specta_typescript::Typescript::default(), bindings_path())
        .expect("failed to export TypeScript bindings");

    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(specta.invoke_handler())
        .register_asynchronous_uri_scheme_protocol(
            tidemail_render::CID_SCHEME,
            |ctx, req, responder| {
                let app = ctx.app_handle().clone();
                let path = req.uri().path().to_owned();
                tauri::async_runtime::spawn_blocking(move || {
                    responder.respond(cid_response(&app, &path))
                });
            },
        )
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            let path = database_path();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let db = tidemail_db::open(&path)?;
            let app_handle = app.handle().clone();
            let notifier: Arc<dyn tidemail_sync::Notifier> = Arc::new(AppNotifier(app_handle));
            let engine = tidemail_sync::Engine::start(path.clone(), notifier);
            app.manage(AppState {
                db: Mutex::new(db),
                db_path: path,
                engine,
            });
            specta.mount_events(app);
            tray::install(app)?;

            // The main window is created here (not from config) so link
            // handling can be attached. It starts hidden; the UI calls
            // report_first_paint after its first populated frame.
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .expect("main window in tauri.conf.json")
                .clone();
            let dev_url = if cfg!(dev) {
                app.config().build.dev_url.clone()
            } else {
                None
            };
            let nav_app = app.handle().clone();
            let popup_app = app.handle().clone();
            tauri::WebviewWindowBuilder::from_config(app, &config)?
                .on_navigation(move |url| {
                    if is_app_url(url, dev_url.as_ref()) {
                        return true;
                    }
                    open_external(&nav_app, url);
                    false
                })
                .on_new_window(move |url, _features| {
                    open_external(&popup_app, &url);
                    NewWindowResponse::Deny
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tidemail");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_urls_stay_in_app() {
        let dev: Url = "http://127.0.0.1:1420".parse().unwrap();
        let ok = |s: &str, d: Option<&Url>| is_app_url(&s.parse().unwrap(), d);
        assert!(ok("http://tauri.localhost/index.html", None));
        assert!(ok("about:blank", None));
        assert!(ok("http://127.0.0.1:1420/src/main.tsx", Some(&dev)));
        assert!(!ok("http://127.0.0.1:1420/", None));
        assert!(!ok("https://evil.example/", Some(&dev)));
        assert!(!ok("http://tauri.localhost.evil.example/", None));
    }
}
