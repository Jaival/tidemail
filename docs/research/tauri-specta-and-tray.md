# tauri-specta versions and Tauri 2 tray (Windows)

Researched 2026-10-31. Primary sources only.

## Summary

- No stable tauri-specta 2.x exists. Latest is `2.0.0-rc.26` (2026-10-21). Stable releases are only 1.0.x [S1].
- Latest specta is `2.0.0-rc.27` (2026-10-21). Latest specta-typescript is `0.0.14` (2026-10-21) [S2][S3].
- tauri-specta rc.27 is NOT on crates.io yet (only on `main`) [S1][S4]. Newest installable set is rc.26.
- Recommended triple (tauri-specta rc.26 crate docs): `specta =2.0.0-rc.26`, `tauri-specta =2.0.0-rc.26`, `specta-typescript 0.0.13` [S5].
- Do not mix specta rc.27 or specta-typescript 0.0.14 with tauri-specta rc.26. rc.26 requires `specta =2.0.0-rc.26` and `specta-typescript ^0.0.13` [S6].
- Commands (`collect_commands!`) and events (`collect_events!` + `derive` feature + `Event` derive) are supported [S5].
- i64/u64/usize etc. are a hard export error by default. Opt-out: `Builder::dangerously_cast_bigints_to_number()`, per-field `#[specta(type = specta_typescript::Number)]`, or serialize as String [S7][S8].
- Tray: enable `tauri` feature `tray-icon`; use `TrayIconBuilder`; hide-on-close via `WindowEvent::CloseRequested` + `prevent_close()` + `hide()`; quit via `app.exit(0)` [S9][S10].
- Single-instance: `tauri-plugin-single-instance` 2.5.5 (2026-10-28), register first [S11][S12]. Autostart: `tauri-plugin-autostart` 2.7.0 (2026-10-01) exists [S13][S14].
- Tauri 3.0.0-alpha.5 exists on crates.io (2026-10-14). Latest stable `tauri` is 2.12.5 (2026-10-21). Stay on 2.x [S15].

## Q1. tauri-specta versioning

### Latest published versions (crates.io API)

| Crate | Latest | Date | Notes |
|---|---|---|---|
| tauri-specta | 2.0.0-rc.26 | 2026-10-21 | rc.25 = 2026-05-08, rc.24 = 2026-03-30 [S1] |
| specta | 2.0.0-rc.27 | 2026-10-21 | rc.26 same day [S2] |
| specta-typescript | 0.0.14 | 2026-10-21 | 0.0.13 same day [S3] |

- Stable (non-RC) tauri-specta 2.x: none. Highest non-prerelease is 1.0.2 (2023-05-18), which targets Tauri v1 [S1][S4 README table].
- `main` Cargo.toml is `tauri-specta 2.0.0-rc.27` with `specta =2.0.0-rc.27` and `specta-typescript 0.0.14` [S4]. It is unpublished as of 2026-10-31 [S1]. Whether it will be published: UNVERIFIED.
- GitHub tags/releases stop at v2.0.0-rc.25. rc.26 has no GitHub release/tag in the API output [S16]. The crate is on crates.io [S1].

### Compatible triple

crates.io dependency metadata for tauri-specta 2.0.0-rc.26 [S6]:
- `specta =2.0.0-rc.26`
- `specta-typescript ^0.0.13` (optional; for 0.0.x this means exactly 0.0.13)
- `specta-serde ^0.0.13`, `specta-util ^0.0.13`, `tauri ^2`
- `tauri-specta-macros =2.0.0-rc.26` (optional, `derive` feature)

specta-typescript 0.0.13 depends on `specta =2.0.0-rc.26` [S17]. The rc.26 crate docs give this install command [S5]:

```sh
cargo add tauri@2.0 specta@=2.0.0-rc.26 specta-typescript@0.0.13
cargo add tauri-specta@=2.0.0-rc.26 --features derive,typescript,javascript
```

Pin with `=`. The docs say: "it is really important you use `=` before your Specta version" [S5]. The maintainer also warned that Tauri breaking changes can break builds, so lock versions [S16, rc.20 notes].

Suggested Cargo.toml:

```toml
specta = "=2.0.0-rc.26"
specta-typescript = "=0.0.13"
tauri-specta = { version = "=2.0.0-rc.26", features = ["derive", "typescript"] }
```

### Feature support (rc.26)

Source: tauri-specta 2.0.0-rc.26 `src/lib.rs` docs [S5].

- Typed commands: annotate with `#[tauri::command]` and `#[specta::specta]`. Register with `Builder::new().commands(collect_commands![...])` and `builder.invoke_handler()`.
- Typed events: `#[derive(Serialize, Deserialize, Debug, Clone, Type, Event)]` (needs `derive` feature). Register with `.events(collect_events![DemoEvent])`. Call `builder.mount_events(app)` in `setup`. Frontend: `events.demoEvent.listen(...)`.
- Export: `builder.export(Typescript::default(), "../src/bindings.ts")`, guarded by `#[cfg(debug_assertions)]`.

```rust
let builder = tauri_specta::Builder::<tauri::Wry>::new()
    .commands(tauri_specta::collect_commands![hello_world])
    .events(tauri_specta::collect_events![DemoEvent]);
#[cfg(debug_assertions)]
builder
    .export(specta_typescript::Typescript::default(), "../src/bindings.ts")
    .expect("export failed");
// tauri::Builder::default()
//     .invoke_handler(builder.invoke_handler())
//     .setup(move |app| { builder.mount_events(app); Ok(()) })
```

### i64 / u64 handling

- Default: specta-typescript forbids `usize, isize, i64, u64, u128, i128, f128`. Export returns an error. Reason: `JSON.parse` loses precision [S8].
- Options in order of preference [S8]: smaller ints (`u32`, `f64`); serialize as `String` via `#[specta(type = String)]` plus serde `with`; per-field `#[specta(type = specta_typescript::Number)]` (unsafe).
- tauri-specta has `Builder::dangerously_cast_bigints_to_number()`. It remaps all bigint-style primitives to TS `number` [S7]. The same function exists in the downloaded rc.26 crate source `src/builder.rs` [S6b].
- Builder also has `semantic_types(...)` for BigInt/Date/Uint8Array/URL mapping [S7][S18]. Not tested here (UNVERIFIED in practice).

### Breaking changes between recent RCs

- tauri-specta rc.24: needs specta rc.24 + specta-typescript 0.0.11. Adds phase-specific serde types (`specta_serde::apply_phases`). specta rc.23/24 have breaking changes [S19].
- specta rc.24: stricter untagged enum output (`{a} & {b?: never}`) [S20].
- tauri-specta rc.25: "Upgrade to latest Specta", semantic types [S16]. specta rc.25 breaking: everything is a `NamedDataType`; `Format` moved to a trait [S20].
- specta rc.27 (2026-10-21) notes: fix for internally-tagged enum regressions from rc.25 [S21]. No tauri-specta release notes for rc.26 found (UNVERIFIED).
- `main` moved to Rust edition 2024 and depends on `specta-serde` [S4].
- Generated `bindings.ts` can differ between RCs. Commit it. Review diffs after each bump.

## Q2. System tray on Windows (Tauri 2)

### Tray icon, menu, left-click

Cargo feature [S9]:

```toml
tauri = { version = "2", features = ["tray-icon"] }
```

The menu shows on left and right click by default. Disable left-click menu with `show_menu_on_left_click(false)` [S9]. The click-event pattern (left button, `Up`, show and focus window) is from the official docs [S9]. The snippet below combines the doc snippets.

```rust
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

// inside .setup(|app| { ... })
let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
let menu = Menu::with_items(app, &[&show, &quit])?;

TrayIconBuilder::new()
    .icon(app.default_window_icon().unwrap().clone())
    .menu(&menu)
    .show_menu_on_left_click(false)
    .on_menu_event(|app, event| match event.id.as_ref() {
        "show" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            if let Some(w) = tray.app_handle().get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    })
    .build(app)?;
```

The docs bind the result to `let tray = ...` [S9]. Whether dropping the `TrayIcon` handle removes the icon: UNVERIFIED.

`TrayIconEvent::DoubleClick` is Windows-only [S22].

### Close window hides instead of quitting

No dedicated official docs page found for this pattern (UNVERIFIED as an official recipe). The API is documented in Tauri source [S10]:
- `WindowEvent::CloseRequested { api }` with `api.prevent_close()`.
- `RunEvent::ExitRequested { code, api }` with `api.prevent_exit()`. `code` is `None` for user-driven exit and `Some(_)` when requested via `AppHandle::exit` or `restart`.

Preferred (window-level, assembled from the API above):

```rust
tauri::Builder::default()
    .on_window_event(|window, event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            if window.label() == "main" {
                api.prevent_close();
                let _ = window.hide();
            }
        }
    })
```

Alternative (app-level, keeps process alive when all windows are gone):

```rust
let app = builder.build(tauri::generate_context!())?;
app.run(|_app, event| {
    if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
        if code.is_none() {
            api.prevent_exit(); // let app.exit(0) through
        }
    }
});
```

`app.exit(0)` also raises `ExitRequested` with `code = Some(0)` per the source doc comment [S10]. So filter on `code.is_none()`, or tray Quit could be blocked. Not run or tested here (UNVERIFIED by test).

Tauri issue #14088 "Application crashes after all windows are hidden closed" is open [S23]. Content not read (UNVERIFIED).

### Really quit

`app.exit(0)` in `on_menu_event` for the `quit` item. Shown in official docs [S9].

### Single instance

- Crate `tauri-plugin-single-instance`: latest stable 2.5.5 (2026-10-28). 3.0.0-alpha.3 is the Tauri 3 line [S11].
- Install: `cargo tauri add single-instance`, or `cargo add tauri-plugin-single-instance --target 'cfg(any(target_os = "macos", windows, target_os = "linux"))'`. Requires Rust >= 1.90 [S12].
- Docs: "must be the first one to be registered" [S12].

```rust
let mut builder = tauri::Builder::default();
#[cfg(desktop)]
{
    builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }));
}
```

The official example only calls `set_focus()` [S12]. `show()` and `unminimize()` are my addition, because the window may be hidden in the tray.

### Autostart (note only)

`tauri-plugin-autostart` 2.7.0 (2026-10-01). 3.0.0-alpha.3 is the Tauri 3 line [S13]. Init: `tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--flag"]))`. Needs Rust >= 1.90. JS package: `@tauri-apps/plugin-autostart`. Capability permissions: `autostart:allow-enable`, `allow-disable`, `allow-is-enabled` [S14]. Windows mechanism (registry key): UNVERIFIED.

### Windows gotchas

- Tray icon format on Windows (ICO vs PNG): UNVERIFIED. `tauri` has Cargo features `tray-icon`, `image-png`, `image-ico` [S24]. If you load an icon from bytes, enable the matching `image-*` feature (my inference from [S24]).
- Icon disappearing after explorer.exe restart: the tray-icon crate registers the `TaskbarCreated` message and re-adds icons [S25]. Its changelog lists re-registering on TaskbarCreated and a fix for the context menu appearing behind the taskbar [S26]. Which tray-icon version `tauri 2.12.5` bundles: UNVERIFIED (dev Cargo.toml says `tray-icon 0.25`) [S24].
- Changelog also lists `TrayIconBuilder::with_guid` so the "always show" setting survives exe updates [S26]. Exposure in Tauri's own builder: UNVERIFIED.
- WebView2 memory while window is hidden: UNVERIFIED. No primary source found. Not measured.
- Tray click events are unsupported on Linux only. Docs list no Windows-specific tray notes [S9].

## Sources

- [S1] https://crates.io/api/v1/crates/tauri-specta/versions
- [S2] https://crates.io/api/v1/crates/specta/versions
- [S3] https://crates.io/api/v1/crates/specta-typescript/versions
- [S4] https://github.com/specta-rs/tauri-specta (README; `main` Cargo.toml https://raw.githubusercontent.com/specta-rs/tauri-specta/main/Cargo.toml)
- [S5] tauri-specta 2.0.0-rc.26 `src/lib.rs` docs, from https://crates.io/api/v1/crates/tauri-specta/2.0.0-rc.26/download (rendered: https://docs.rs/tauri-specta/2.0.0-rc.26/tauri_specta/)
- [S6] https://crates.io/api/v1/crates/tauri-specta/2.0.0-rc.26/dependencies
- [S6b] same downloaded rc.26 crate: `Cargo.toml`, `src/builder.rs`, `src/lang/js_ts.rs`
- [S7] https://github.com/specta-rs/tauri-specta/blob/main/src/builder.rs
- [S8] https://docs.rs/specta-typescript/latest/specta_typescript/struct.Error.html#bigint-forbidden (source https://github.com/specta-rs/specta/blob/main/specta-typescript/src/error.rs)
- [S9] https://v2.tauri.app/learn/system-tray/
- [S10] https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/src/app.rs
- [S11] https://crates.io/api/v1/crates/tauri-plugin-single-instance/versions
- [S12] https://v2.tauri.app/plugin/single-instance/
- [S13] https://crates.io/api/v1/crates/tauri-plugin-autostart/versions
- [S14] https://v2.tauri.app/plugin/autostart/
- [S15] https://crates.io/api/v1/crates/tauri/versions
- [S16] https://github.com/specta-rs/tauri-specta/releases
- [S17] https://crates.io/api/v1/crates/specta-typescript/0.0.13/dependencies
- [S18] https://docs.rs/specta-typescript/latest/specta_typescript/semantic/index.html
- [S19] https://github.com/specta-rs/tauri-specta/releases/tag/v2.0.0-rc.24
- [S20] https://github.com/specta-rs/specta/releases
- [S21] https://github.com/specta-rs/specta/releases/tag/v2.0.0-rc.27
- [S22] https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/src/tray/mod.rs
- [S23] https://github.com/tauri-apps/tauri/issues/14088
- [S24] https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/Cargo.toml
- [S25] https://github.com/tauri-apps/tray-icon/blob/dev/src/platform_impl/windows/mod.rs
- [S26] https://github.com/tauri-apps/tray-icon/blob/dev/CHANGELOG.md
