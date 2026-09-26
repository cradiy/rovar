use gpui::{App, Global, SharedString};

pub(crate) const SYSTEM: &str = ".SystemUIFont";

struct UiFont(SharedString);
impl Global for UiFont {}

pub(crate) fn family(cx: &App) -> SharedString {
    cx.try_global::<UiFont>()
        .map(|font| font.0.clone())
        .unwrap_or_else(|| SYSTEM.into())
}

pub(crate) fn init(cx: &mut App) -> std::io::Result<()> {
    if !cfg!(target_os = "linux") {
        cx.set_global(UiFont(SYSTEM.into()));
        return Ok(());
    }
    let path = crate::settings::path()
        .ok_or_else(|| std::io::Error::other("Could not locate settings directory"))?;
    let family = crate::settings::ui_font(&path, detect)?;
    cx.set_global(UiFont(family.into()));
    Ok(())
}

pub(crate) fn set(family: SharedString, cx: &mut App) -> std::io::Result<()> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    crate::settings::set("ui_font", serde_json::Value::String(family.to_string()))?;
    cx.set_global(UiFont(family));
    cx.refresh_windows();
    Ok(())
}

pub(crate) fn detect() -> String {
    #[cfg(target_os = "linux")]
    if let Some(family) = linux::detect() {
        return family;
    }
    SYSTEM.into()
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    fn ini_value(source: &str, section: &str, key: &str) -> Option<String> {
        let mut active = false;
        for line in source.lines().map(str::trim) {
            if line.starts_with('[') {
                active = line == format!("[{section}]");
            } else if active
                && let Some((name, value)) = line.split_once('=')
                && name.trim() == key
                && !value.trim().is_empty()
            {
                return Some(value.trim().into());
            }
        }
        None
    }

    fn pango_family(value: &str) -> Option<String> {
        let value = value.trim().trim_matches('\'');
        let (family, size) = value.rsplit_once(' ')?;
        size.trim_end_matches("px").parse::<f32>().ok()?;
        let mut family = family.trim();
        while let Some((head, tail)) = family.rsplit_once(' ') {
            if [
                "regular", "bold", "italic", "oblique", "medium", "semibold", "light",
            ]
            .contains(&tail.to_ascii_lowercase().as_str())
            {
                family = head.trim();
            } else {
                break;
            }
        }
        (!family.is_empty()).then(|| family.to_owned())
    }

    fn gsettings() -> Option<String> {
        let mut child = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "font-name"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => {
                    let output = child.wait_with_output().ok()?;
                    return pango_family(&String::from_utf8(output.stdout).ok()?);
                }
                Ok(Some(_)) => return None,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        }
    }

    pub(super) fn detect() -> Option<String> {
        let config = dirs::config_dir()?;
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase();
        if desktop.split(':').any(|name| name == "kde")
            && let Ok(source) = std::fs::read_to_string(config.join("kdeglobals"))
            && let Some(font) = ini_value(&source, "General", "font")
            && let Some(family) = font
                .split(',')
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            return Some(family.into());
        }
        for path in ["gtk-4.0/settings.ini", "gtk-3.0/settings.ini"] {
            if let Ok(source) = std::fs::read_to_string(config.join(path))
                && let Some(font) = ini_value(&source, "Settings", "gtk-font-name")
                && let Some(family) = pango_family(&font)
            {
                return Some(family);
            }
        }
        gsettings()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn desktop_font_values_keep_family_names_and_drop_size_and_style() {
            for (source, expected) in [
                ("'Adwaita Sans 11'", "Adwaita Sans"),
                ("Noto Sans CJK SC 10.5", "Noto Sans CJK SC"),
                ("Source Sans 3 Bold 12px", "Source Sans 3"),
            ] {
                assert_eq!(pango_family(source).as_deref(), Some(expected));
            }
            assert_eq!(pango_family(""), None);
            assert_eq!(
                ini_value(
                    "[Other]\nfont=wrong\n[General]\nfont=Noto Sans,10,-1",
                    "General",
                    "font"
                )
                .as_deref(),
                Some("Noto Sans,10,-1")
            );
        }
    }
}
