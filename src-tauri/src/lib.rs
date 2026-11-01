pub mod commands;
pub mod events;
mod tray;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use tauri::Manager;
use tauri_specta::Event;
use tidemail_db::Connection;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub db_path: PathBuf,
    pub engine: tidemail_sync::Engine,
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

pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Tauri crate must live in the workspace")
        .join("src/bindings.ts");
    specta_builder().export(specta_typescript::Typescript::default(), output)?;
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

pub fn run() {
    let specta = specta_builder();

    #[cfg(debug_assertions)]
    specta
        .export(
            specta_typescript::Typescript::default(),
            "../src/bindings.ts",
        )
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
            if let Some(window) = app.get_webview_window("main") {
                // The UI calls report_first_paint after its first populated frame.
                let _ = window.hide();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tidemail");
}
