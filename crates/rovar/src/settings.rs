use std::{
    io::Write,
    path::{Path, PathBuf},
};
use uic::desktop::TitleBarMode;

pub(crate) fn path() -> Option<PathBuf> {
    #[cfg(target_family = "wasm")]
    return Some("/workspace/settings.json".into());
    #[cfg(not(target_family = "wasm"))]
    std::env::var_os("ROVAR_CONFIG")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|dir| dir.join("rovar/settings.json")))
}

pub(crate) fn read(path: Option<&Path>) -> std::io::Result<serde_json::Value> {
    let Some(path) = path else {
        return Ok(serde_json::json!({}));
    };
    match rovar_storage::fs::read_to_string(path) {
        Ok(source) => {
            let config: serde_json::Value = serde_json::from_str(&source)?;
            if !config.is_object() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Settings must be a JSON object",
                ));
            }
            Ok(config)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(error) => Err(error),
    }
}

pub(crate) fn write(path: &Path, config: &serde_json::Value) -> std::io::Result<()> {
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    rovar_storage::fs::create_dir_all(directory)?;
    let mut file = rovar_storage::tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer_pretty(&mut file, config)?;
    writeln!(file)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn titlebar(
    path: &Path,
    detect: impl FnOnce() -> TitleBarMode,
) -> std::io::Result<TitleBarMode> {
    let mut config = read(Some(path))?;
    if let Some(value) = config.get("titlebar") {
        return serde_json::from_value(value.clone()).map_err(Into::into);
    }
    let mode = detect();
    config["titlebar"] = serde_json::to_value(mode)?;
    write(path, &config)?;
    Ok(mode)
}

pub(crate) fn set(key: &str, value: serde_json::Value) -> std::io::Result<()> {
    let path =
        path().ok_or_else(|| std::io::Error::other("Could not locate settings directory"))?;
    let mut config = read(Some(&path))?;
    config[key] = value;
    write(&path, &config)
}

pub(crate) fn ui_font(path: &Path, detect: impl FnOnce() -> String) -> std::io::Result<String> {
    let mut config = read(Some(path))?;
    if let Some(value) = config.get("ui_font") {
        return value
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "UI font must be a nonempty family name",
                )
            });
    }
    let family = detect();
    config["ui_font"] = family.clone().into();
    write(path, &config)?;
    Ok(family)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_font_is_initialized_once_without_replacing_other_settings() {
        let root = rovar_storage::tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        write(
            &path,
            &serde_json::json!({"titlebar":"hide", "language":"en-US"}),
        )
        .unwrap();
        assert_eq!(
            ui_font(&path, || "Desktop Sans".into()).unwrap(),
            "Desktop Sans"
        );
        assert_eq!(
            ui_font(&path, || panic!("must retain the configured font")).unwrap(),
            "Desktop Sans"
        );
        let config = read(Some(&path)).unwrap();
        assert_eq!(config["titlebar"], "hide");
        assert_eq!(config["language"], "en-US");
        for invalid in [serde_json::json!(""), serde_json::json!(true)] {
            let mut config = config.clone();
            config["ui_font"] = invalid;
            write(&path, &config).unwrap();
            let before = rovar_storage::fs::read(&path).unwrap();
            assert!(ui_font(&path, || panic!("must not overwrite invalid settings")).is_err());
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn titlebar_is_detected_once_and_preserves_other_preferences() {
        let root = rovar_storage::tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        write(
            &path,
            &serde_json::json!({"language":"zh-CN", "language_directories":["locales"]}),
        )
        .unwrap();
        assert_eq!(
            titlebar(&path, || TitleBarMode::Hide).unwrap(),
            TitleBarMode::Hide
        );
        assert_eq!(
            titlebar(&path, || panic!("must not detect again")).unwrap(),
            TitleBarMode::Hide
        );
        let mut config = read(Some(&path)).unwrap();
        assert_eq!(config["language"], "zh-CN");
        assert_eq!(config["language_directories"][0], "locales");
        for mode in ["compact", "system", "hide"] {
            config["titlebar"] = mode.into();
            write(&path, &config).unwrap();
            assert_eq!(
                titlebar(&path, || panic!("explicit setting")).unwrap(),
                mode.parse().unwrap()
            );
        }
        config["titlebar"] = "invalid".into();
        write(&path, &config).unwrap();
        let before = rovar_storage::fs::read(&path).unwrap();
        assert!(titlebar(&path, || TitleBarMode::Compact).is_err());
        assert_eq!(rovar_storage::fs::read(&path).unwrap(), before);
        let first = root.path().join("fresh/settings.json");
        titlebar(&first, || TitleBarMode::Compact).unwrap();
        assert_eq!(read(Some(&first)).unwrap()["titlebar"], "compact");
    }
}
