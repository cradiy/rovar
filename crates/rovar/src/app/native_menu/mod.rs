use super::*;
use gpui::{
    Action, App, Div, KeyBinding, Keystroke, Menu, MenuItem, OsAction, SystemMenuType, actions,
};

actions!(
    rovar,
    [
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        NewDocument,
        OpenDocument,
        SaveDocument,
        ExportDocument,
        Settings,
        CloseDocument,
        CloseWindow,
        Undo,
        Redo,
        Cut,
        Copy,
        Paste,
        SelectAll,
        Minimize,
        Zoom,
        ToggleFullscreen,
    ]
);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullscreen, None),
        KeyBinding::new("cmd-shift-w", CloseWindow, None),
    ]);
    // These bindings supply native menu key equivalents only. The editor handles
    // these keys itself, and TextInput has its own focus-specific bindings.
    // An inactive context lets menu clicks reuse that dispatch without recursion
    // or replacing the focused input's clipboard and IME behavior.
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewDocument, Some("NativeMenu")),
        KeyBinding::new("cmd-o", OpenDocument, Some("NativeMenu")),
        KeyBinding::new("cmd-s", SaveDocument, Some("NativeMenu")),
        KeyBinding::new("cmd-shift-e", ExportDocument, Some("NativeMenu")),
        KeyBinding::new("cmd-,", Settings, Some("NativeMenu")),
        KeyBinding::new("cmd-w", CloseDocument, Some("NativeMenu")),
        KeyBinding::new("cmd-z", Undo, Some("NativeMenu")),
        KeyBinding::new("cmd-shift-z", Redo, Some("NativeMenu")),
        KeyBinding::new("cmd-x", Cut, Some("NativeMenu")),
        KeyBinding::new("cmd-c", Copy, Some("NativeMenu")),
        KeyBinding::new("cmd-v", Paste, Some("NativeMenu")),
        KeyBinding::new("cmd-a", SelectAll, Some("NativeMenu")),
    ]);
    cx.on_action(|_: &Quit, cx| cx.defer(quit));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    refresh(cx);
}

pub(crate) fn refresh(cx: &mut App) {
    cx.set_menus([
        Menu::new("Rovar").items([
            MenuItem::action(t("settings"), Settings),
            MenuItem::separator(),
            MenuItem::os_submenu(t("menu-services"), SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(t("menu-hide-app"), Hide),
            MenuItem::action(t("menu-hide-others"), HideOthers),
            MenuItem::action(t("menu-show-all"), ShowAll),
            MenuItem::separator(),
            MenuItem::action(t("menu-quit"), Quit),
        ]),
        Menu::new(t("menu-file")).items([
            MenuItem::action(t("new-document"), NewDocument),
            MenuItem::action(t("open-document"), OpenDocument),
            MenuItem::separator(),
            MenuItem::action(t("save-document"), SaveDocument),
            MenuItem::action(t("export-document"), ExportDocument),
            MenuItem::separator(),
            MenuItem::action(t("menu-close-document"), CloseDocument),
            MenuItem::action(t("menu-close-window"), CloseWindow),
        ]),
        Menu::new(t("menu-edit")).items([
            MenuItem::os_action(t("undo"), Undo, OsAction::Undo),
            MenuItem::os_action(t("redo"), Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action(t("cut"), Cut, OsAction::Cut),
            MenuItem::os_action(t("copy"), Copy, OsAction::Copy),
            MenuItem::os_action(t("paste"), Paste, OsAction::Paste),
            MenuItem::os_action(t("menu-select-all"), SelectAll, OsAction::SelectAll),
        ]),
        Menu::new(t("menu-window")).items([
            MenuItem::action(t("menu-minimize"), Minimize),
            MenuItem::action(t("menu-zoom"), Zoom),
            MenuItem::action(t("menu-fullscreen"), ToggleFullscreen),
        ]),
    ]);
}

fn quit(cx: &mut App) {
    let windows: Vec<_> = cx
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<Studio>())
        .collect();
    if windows.is_empty() {
        cx.quit();
        return;
    }
    for handle in windows {
        let _ = handle.update(cx, |studio, window, cx| {
            // Preserve every window's tabs in the shared session during app quit.
            studio.quit_requested = true;
            studio.begin_close(window, cx);
        });
    }
    // The existing on_window_closed callback quits after the last save/close.
    // A failed save leaves its window open and cancels that window's close.
}

fn shortcut<A: Action>(element: Div, key: &'static str) -> Div {
    element.on_action(move |_: &A, window, cx| {
        let handle = window.window_handle();
        // Avoid re-entering Studio while its action listener is being dispatched.
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx);
            });
        });
    })
}

impl Studio {
    pub(super) fn native_menu_actions(&self, element: Div, cx: &mut Context<Self>) -> Div {
        let available = !self.closing
            && self.open_errors.is_empty()
            && self.deleting_document.is_none()
            && self.server_action.is_none()
            && self.renaming.is_none()
            && self.preferences.is_none();
        let editing = available && self.active_editor().is_some();
        let exporting = self
            .tabs
            .iter()
            .any(|tab| Some(tab.token) == self.active && tab.exporting);
        element
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &Zoom, window, _| window.zoom_window())
            .on_action(|_: &ToggleFullscreen, window, _| window.toggle_fullscreen())
            .on_action(
                cx.listener(|this, _: &CloseWindow, window, cx| this.begin_close(window, cx)),
            )
            .map(|el| shortcut::<Cut>(el, "cmd-x"))
            .map(|el| shortcut::<Copy>(el, "cmd-c"))
            .map(|el| shortcut::<Paste>(el, "cmd-v"))
            .map(|el| shortcut::<SelectAll>(el, "cmd-a"))
            .when(available, |el| {
                el.map(|el| shortcut::<NewDocument>(el, "cmd-n"))
                    .map(|el| shortcut::<OpenDocument>(el, "cmd-o"))
                    .map(|el| shortcut::<Settings>(el, "cmd-,"))
                    .map(|el| shortcut::<CloseDocument>(el, "cmd-w"))
            })
            .when(editing, |el| {
                el.map(|el| shortcut::<SaveDocument>(el, "cmd-s"))
                    .map(|el| shortcut::<Undo>(el, "cmd-z"))
                    .map(|el| shortcut::<Redo>(el, "cmd-shift-z"))
                    .when(!exporting, |el| {
                        shortcut::<ExportDocument>(el, "cmd-shift-e")
                    })
            })
    }
}

#[cfg(test)]
mod tests;
