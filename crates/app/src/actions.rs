//! Every keyboard action and its default key. The Ctrl+K palette lists this table.

use gpui_kit::{actions, App, KeyBinding};

actions!(
    tidemail,
    [
        SelectNext,
        SelectPrev,
        OpenThread,
        Archive,
        Trash,
        ToggleStar,
        ToggleRead,
        PickLabel,
        PickMove,
        FocusSearch,
        CommandPalette,
        GoInbox,
        GoLabel,
        CloseReader,
        Undo,
        Dismiss,
        ComposeNew,
        SendMail,
        FocusNextField,
        FocusPrevField,
    ]
);

/// Key context of the shell root; all bindings apply inside it.
pub const CONTEXT: &str = "Tidemail";
/// Shell root context while a text input (search, compose) has focus: no
/// single-key bindings, so letters reach the input.
pub const SEARCH_CONTEXT: &str = "TidemailSearch";
/// Shell root context while a picker is open: only the keys a picker uses,
/// so letters reach its filter input.
pub const PICKER_CONTEXT: &str = "TidemailPicker";
/// Added to the shell root's context while the reader is open.
pub const READING: &str = "reading";

/// An input inside the compose form.
const COMPOSE_INPUT: &str = "TidemailCompose > Input";

/// (keys as shown to the user, description) for the palette.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("j", "Next thread"),
    ("k", "Previous thread"),
    ("Enter", "Open thread"),
    ("e", "Archive"),
    ("#", "Trash"),
    ("s", "Star / unstar"),
    ("u", "Mark read / unread"),
    ("l", "Label…"),
    ("v", "Move to…"),
    ("/", "Search"),
    ("Ctrl+K", "Command palette"),
    ("g i", "Go to inbox"),
    ("g l", "Go to label…"),
    ("c", "Compose"),
    ("Esc", "Close reader"),
    ("Ctrl+Z", "Undo"),
];

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("j", SelectNext, c),
        KeyBinding::new("down", SelectNext, c),
        KeyBinding::new("k", SelectPrev, c),
        KeyBinding::new("up", SelectPrev, c),
        KeyBinding::new("enter", OpenThread, c),
        KeyBinding::new("e", Archive, c),
        KeyBinding::new("#", Trash, c),
        KeyBinding::new("shift-3", Trash, c),
        KeyBinding::new("s", ToggleStar, c),
        KeyBinding::new("u", ToggleRead, c),
        KeyBinding::new("l", PickLabel, c),
        KeyBinding::new("v", PickMove, c),
        KeyBinding::new("/", FocusSearch, c),
        KeyBinding::new("ctrl-k", CommandPalette, c),
        KeyBinding::new("g i", GoInbox, c),
        KeyBinding::new("ctrl-z", Undo, c),
        KeyBinding::new("g l", GoLabel, c),
        KeyBinding::new("c", ComposeNew, c),
        KeyBinding::new("escape", Dismiss, c),
        // After Dismiss: at equal depth the later binding wins.
        KeyBinding::new("escape", CloseReader, Some("Tidemail && reading")),
        KeyBinding::new("ctrl-k", CommandPalette, Some(SEARCH_CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(PICKER_CONTEXT)),
        KeyBinding::new("up", SelectPrev, Some(PICKER_CONTEXT)),
        KeyBinding::new("enter", OpenThread, Some(PICKER_CONTEXT)),
        KeyBinding::new("escape", Dismiss, Some(PICKER_CONTEXT)),
        KeyBinding::new("ctrl-k", CommandPalette, Some(PICKER_CONTEXT)),
        // Inside compose these replace the input's own Ctrl+Enter (newline)
        // and Tab (indent): same depth, registered later, so they win.
        KeyBinding::new("ctrl-enter", SendMail, Some(COMPOSE_INPUT)),
        KeyBinding::new("tab", FocusNextField, Some(COMPOSE_INPUT)),
        KeyBinding::new("shift-tab", FocusPrevField, Some(COMPOSE_INPUT)),
    ]);
}
