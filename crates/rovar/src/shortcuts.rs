/// The primary shortcut modifier accepted by the editor on this platform.
pub(crate) const PRIMARY_MODIFIER: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl"
};

/// Expand the platform-neutral `Mod+` prefix for menus and tooltips.
pub(crate) fn label(shortcut: &str) -> String {
    match shortcut.strip_prefix("Mod+") {
        Some(keys) => format!("{PRIMARY_MODIFIER}+{keys}"),
        None => shortcut.to_owned(),
    }
}
