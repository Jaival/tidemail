//! System tray icon: left-click shows the window; menu has Show and Quit.

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};

use crate::events::{AppEvent, EventTx};

/// Keep the returned icon alive; dropping it removes it from the tray.
pub fn install(tx: EventTx) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let show = MenuItem::new("Show Tidemail", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append(&show)?;
    menu.append(&quit)?;
    // Resource 1 (`assets/tidemail.rc`) at the small-icon size for this DPI.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16) as u32;
    let icon = Icon::from_resource(1, Some((size, size)))?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("Tidemail")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?;

    let (show_id, quit_id) = (show.id().clone(), quit.id().clone());
    let menu_tx = tx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == show_id {
            menu_tx.send(AppEvent::ShowWindow);
        } else if e.id == quit_id {
            menu_tx.send(AppEvent::Quit);
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = e
        {
            tx.send(AppEvent::ShowWindow);
        }
    }));
    Ok(tray)
}
