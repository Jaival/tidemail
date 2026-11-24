//! Tidemail: local-first Gmail client for Windows 11 (GPUI).
//!
//! Startup order is tuned for cold launch: the database preload and sync
//! engine start on a thread while GPUI starts; the window opens cloaked and
//! is revealed after its first frame (with list rows) is drawn.

#![windows_subsystem = "windows"]

mod actions;
mod assets;
mod bench;
mod data;
mod events;
mod features;
mod fonts;
mod format;
mod gmail_adapter;
mod preset;
mod settings;
mod single_instance;
mod state;
mod theme;
mod tray;
mod ui;
mod win;

use std::{path::PathBuf, sync::Arc};

use futures::StreamExt;
use gpui_kit::component::TitleBar;
use gpui_kit::*;
use tidemail_sync::{api::GmailProvider, Engine};

use crate::{events::AppEvent, features::shell::Shell, state::AppState};

struct Args {
    db: PathBuf,
    bench: Option<PathBuf>,
    preset: Option<preset::Preset>,
}

fn args() -> Args {
    let mut db = None;
    let mut bench = None;
    let mut preset = None;
    let mut it = std::env::args_os().skip(1);
    while let Some(arg) = it.next() {
        if arg == "--db" {
            db = it.next().map(PathBuf::from);
        } else if arg == "--bench" {
            bench = it.next().map(PathBuf::from);
        } else if arg == "--preset" {
            let name = it.next().unwrap_or_default();
            preset = name.to_str().and_then(preset::Preset::parse);
            if preset.is_none() {
                eprintln!("tidemail: unknown preset {name:?} (tide, command, paper)");
            }
        }
    }
    let db = db.unwrap_or_else(|| {
        let base =
            std::env::var_os("LOCALAPPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
        base.join("Tidemail").join("mail.db")
    });
    Args {
        db: std::path::absolute(&db).unwrap_or(db),
        bench,
        preset,
    }
}

fn main() {
    let args = args();
    let single_instance::Instance::First { pipe } = single_instance::acquire(&args.db) else {
        return;
    };
    let (tx, mut rx) = events::channel();

    let preload = {
        let (path, tx, bench) = (args.db.clone(), tx.clone(), args.bench.is_some());
        std::thread::spawn(move || {
            let pre = data::preload(path.clone())?;
            // The bench runs on a copy of the fake mailbox: no Gmail.
            let provider: Option<Arc<dyn GmailProvider>> =
                (!bench).then(|| Arc::new(gmail_adapter::Provider::new()) as _);
            let engine = Engine::start_with(path, Arc::new(tx), provider, Default::default());
            Ok::<_, String>((pre, engine))
        })
    };

    let bench_path = args.bench;
    let preset_override = args.preset;
    let settings_path = settings::path_for(&args.db);
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            actions::bind_keys(cx);

            let (pre, engine) = match preload.join().expect("preload thread") {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("tidemail: cannot open database: {e}");
                    cx.quit();
                    return;
                }
            };
            let state = cx.new(|_| {
                AppState::new(
                    pre.db,
                    Arc::new(engine),
                    pre.accounts,
                    pre.labels,
                    pre.unread,
                    settings_path,
                    preset_override,
                )
            });

            let options = WindowOptions {
                show: false,
                focus: true,
                window_bounds: Some(WindowBounds::centered(size(px(1360.), px(860.)), cx)),
                window_min_size: Some(size(px(900.), px(560.))),
                ..TitleBar::window_options()
            };
            let first_page = pre.first_page;
            let bench_path = bench_path.clone();
            let opened = gpui_kit::open_window(options, cx, |window, cx| {
                let s = state.read(cx).settings.clone();
                theme::apply(&s, window, cx);
                let theme_state = state.clone();
                window
                    .observe_window_appearance(move |window, cx| {
                        let s = theme_state.read(cx).settings.clone();
                        theme::apply(&s, window, cx)
                    })
                    .detach();

                // Hide instead of close; sync keeps running. Quit is in the tray.
                window.on_window_should_close(cx, |window, _| {
                    win::hide(win::hwnd(window));
                    false
                });

                // GPUI draws no frames for a hidden window: show it cloaked, reveal after frame 1.
                let hwnd = win::hwnd(window);
                win::set_cloaked(hwnd, true);
                win::show_no_activate(hwnd);
                let shell = cx.new(|cx| Shell::new(state.clone(), first_page, window, cx));
                let bench_shell = shell.clone();
                window.on_next_frame(move |window, cx| {
                    let cold = win::ms_since_process_start();
                    win::set_cloaked(hwnd, false);
                    window.activate_window();
                    eprintln!("tidemail: cold launch {cold:.0} ms");
                    if let Some(path) = bench_path {
                        bench::start(bench_shell, path, cold, window, cx);
                    }
                });
                shell
            });
            let window = match opened {
                Ok((window, _)) => window,
                Err(e) => {
                    eprintln!("tidemail: cannot open window: {e:#}");
                    cx.quit();
                    return;
                }
            };

            let tray = match tray::install(tx.clone()) {
                Ok((icon, quit)) => {
                    quit.set_text(tray::quit_text(state.read(cx).status.pending_sends));
                    Some((icon, quit))
                }
                Err(e) => {
                    eprintln!("tidemail: tray icon unavailable: {e}");
                    None
                }
            };
            let show_tx = tx.clone();
            single_instance::listen(pipe, move || show_tx.send(AppEvent::ShowWindow));

            cx.spawn(async move |cx| {
                let tray = tray;
                while let Some(event) = rx.next().await {
                    cx.update(|cx| match event {
                        AppEvent::DataChanged {
                            thread_ids,
                            label_keys,
                        } => {
                            state.update(cx, |s, cx| s.on_data_changed(thread_ids, label_keys, cx))
                        }
                        AppEvent::StatusChanged(status) => {
                            if let Some((_, quit)) = &tray {
                                quit.set_text(tray::quit_text(status.pending_sends));
                            }
                            state.update(cx, |s, cx| s.on_status(status, cx))
                        }
                        AppEvent::ShowWindow => {
                            let _ = window.update(cx, |_, window, _| win::show(window));
                        }
                        AppEvent::Quit => cx.quit(),
                    });
                }
            })
            .detach();
        });
}
