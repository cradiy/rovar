/// The primary shortcut modifier accepted by the editor on this platform.
pub(crate) fn primary_modifier() -> &'static str {
    if gpui::Modifiers::secondary_key().platform {
        "⌘"
    } else {
        "Ctrl"
    }
}

/// Expand the platform-neutral `Mod+` prefix for menus and tooltips.
pub(crate) fn label(shortcut: &str) -> String {
    match shortcut.strip_prefix("Mod+") {
        Some(keys) => format!("{}+{keys}", primary_modifier()),
        None => shortcut.to_owned(),
    }
}
