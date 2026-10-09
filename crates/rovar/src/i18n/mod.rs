use crate::settings::read as read_config;
use fluent_bundle::{FluentArgs, FluentResource, concurrent::FluentBundle};
use fluent_syntax::ast::Entry;
use std::{cell::RefCell, collections::HashMap, path::PathBuf, sync::OnceLock};
mod catalog;
use catalog::Catalogs;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Language {
    #[default]
    System,
    English,
    Chinese,
    External(&'static str),
}

impl Language {
    pub fn available() -> Vec<Self> {
        [Self::System, Self::English, Self::Chinese]
            .into_iter()
            .chain(
                catalogs()
                    .languages
                    .keys()
                    .filter(|id| id.as_str() != "en-US" && id.as_str() != "zh-CN")
                    .map(|id| Self::External(id.as_str())),
            )
            .collect()
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::English => "en-US",
            Self::Chinese => "zh-CN",
            Self::External(id) => id,
        }
    }

    pub fn label(self) -> &'static str {
        if let Self::External(id) = self {
            return catalogs().languages[id]
                .labels
                .get("language-name")
                .map_or(id, String::as_str);
        }
        t(match self {
            Self::System => "language-system",
            Self::English => "language-en",
            Self::Chinese => "language-zh",
            Self::External(_) => unreachable!(),
        })
    }

    fn parse(value: &str) -> Self {
        match value.trim() {
            "en-US" => Self::English,
            "zh-CN" => Self::Chinese,
            "system" | "auto" | "" => Self::System,
            value => catalogs()
                .languages
                .get_key_value(value)
                .map_or(Self::English, |(id, _)| Self::External(id)),
        }
    }

    fn resolve(self, system: &[&str]) -> &'static str {
        let explicit = [self.id()];
        catalogs().resolve(if self == Self::System {
            system
        } else {
            &explicit
        })
    }
}

struct Catalog {
    bundle: FluentBundle<FluentResource>,
    labels: HashMap<String, String>,
}

impl Catalog {
    fn new(locale: &str, source: &str) -> Self {
        let resource =
            FluentResource::try_new(source.to_owned()).expect("Invalid bundled Fluent resource");
        Self::from_resources(locale, vec![resource])
    }

    fn from_resources(locale: &str, resources: Vec<FluentResource>) -> Self {
        let keys: Vec<_> = resources
            .iter()
            .flat_map(|resource| resource.entries())
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some(message.id.name.to_owned()),
                _ => None,
            })
            .collect();
        let mut bundle =
            FluentBundle::new_concurrent(vec![locale.parse().expect("Invalid locale")]);
        bundle.set_use_isolating(false);
        for resource in resources {
            bundle.add_resource_overriding(resource);
        }
        // Resolve platform keys before caching labels, including fallback catalogs.
        bundle.add_resource_overriding(
            FluentResource::try_new(format!(
                "-primary-modifier = {}\n",
                crate::ui::shortcuts::primary_modifier()
            ))
            .expect("Invalid shortcut modifier term"),
        );
        let mut catalog = Self {
            bundle,
            labels: HashMap::new(),
        };
        for key in keys {
            if let Some(value) = catalog.format(&key, None) {
                catalog.labels.insert(key, value);
            }
        }
        catalog
    }

    fn format(&self, key: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
        let pattern = self.bundle.get_message(key)?.value()?;
        let mut errors = Vec::new();
        let value = self.bundle.format_pattern(pattern, args, &mut errors);
        errors.is_empty().then(|| value.into_owned())
    }
}

static CATALOGS: OnceLock<Catalogs> = OnceLock::new();
fn catalogs() -> &'static Catalogs {
    CATALOGS.get_or_init(|| Catalogs::load(&[]))
}

#[derive(Default)]
struct Settings {
    preference: Language,
    resolved: &'static str,
    path: Option<PathBuf>,
}

thread_local! {
    static SETTINGS: RefCell<Settings> = RefCell::new(Settings::default());
}

pub(crate) fn init() {
    load_settings(crate::settings::path());
}

fn load_settings(path: Option<PathBuf>) {
    let config = read_config(path.as_deref()).unwrap_or_else(|error| {
        eprintln!("Could not read Rovar settings: {error}");
        serde_json::json!({})
    });
    let mut directories = Vec::new();
    if let Some(parent) = path.as_deref().and_then(|path| path.parent()) {
        directories.push(parent.join("locales"));
        if let Some(paths) = config
            .get("language_directories")
            .and_then(|paths| paths.as_array())
        {
            directories.extend(
                paths
                    .iter()
                    .filter_map(|path| path.as_str())
                    .map(|path| parent.join(path)),
            );
        }
    }
    CATALOGS.get_or_init(|| Catalogs::load(&directories));
    let preference = Language::parse(
        config
            .get("language")
            .and_then(|value| value.as_str())
            .unwrap_or("system"),
    );
    let system: Vec<_> = sys_locale::get_locales().collect();
    SETTINGS.with_borrow_mut(|settings| {
        settings.path = path;
        settings.preference = preference;
        settings.resolved =
            preference.resolve(&system.iter().map(String::as_str).collect::<Vec<_>>());
    });
}

pub(crate) fn preference() -> Language {
    SETTINGS.with_borrow(|settings| settings.preference)
}

pub(crate) fn set_language(language: Language) -> std::io::Result<()> {
    SETTINGS.with_borrow_mut(|settings| {
        if let Some(path) = &settings.path {
            let mut config = read_config(Some(path))?;
            config["language"] = language.id().into();
            crate::settings::write(path, &config)?;
        }
        settings.preference = language;
        let system: Vec<_> = sys_locale::get_locales().collect();
        settings.resolved =
            language.resolve(&system.iter().map(String::as_str).collect::<Vec<_>>());
        Ok(())
    })
}

pub(crate) fn t(key: &'static str) -> &'static str {
    catalogs().text(SETTINGS.with_borrow(|settings| settings.resolved), key)
}

pub(crate) fn message(key: &str, values: &[(&str, String)]) -> String {
    let mut args = FluentArgs::new();
    for (name, value) in values {
        args.set(*name, value.as_str());
    }
    catalogs().format(
        SETTINGS.with_borrow(|settings| settings.resolved),
        key,
        &args,
    )
}

pub(crate) fn count(key: &str, count: usize) -> String {
    let mut args = FluentArgs::new();
    args.set("count", count);
    catalogs().format(
        SETTINGS.with_borrow(|settings| settings.resolved),
        key,
        &args,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_resolution_and_explicit_override() {
        for locale in ["zh", "zh-CN", "zh_SG.UTF-8", "zh-Hans", "zh-Hans-CN"] {
            assert_eq!(Language::System.resolve(&[locale]), "zh-CN");
            assert_eq!(Language::English.resolve(&[locale]), "en-US");
        }
        for locale in ["en-GB", "de-DE", "C"] {
            assert_eq!(Language::System.resolve(&[locale]), "en-US");
            assert_eq!(Language::Chinese.resolve(&[locale]), "zh-CN");
        }
        assert_eq!(Language::System.resolve(&["de-DE", "zh-CN"]), "zh-CN");
    }

    #[test]
    fn catalogs_format_all_messages_and_support_plural_counts() {
        let mut args = FluentArgs::new();
        for name in ["id", "name", "kind", "label", "error"] {
            args.set(name, "Sample");
        }
        args.set("count", 2);
        args.set("start", 1);
        args.set("end", 6);
        args.set("total", 11);
        args.set("revision", 3);
        let keys = |source: &str| {
            FluentResource::try_new(source.into())
                .unwrap()
                .entries()
                .filter_map(|entry| match entry {
                    Entry::Message(message) => Some(message.id.name.to_owned()),
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
        };
        let english = keys(include_str!("../../locales/en-US.ftl"));
        let chinese = keys(include_str!("../../locales/zh-CN.ftl"));
        assert_eq!(english, chinese);
        for key in english {
            assert!(
                catalogs().languages["en-US"]
                    .format(&key, Some(&args))
                    .is_some(),
                "English: {key}"
            );
            assert!(
                catalogs().languages["zh-CN"]
                    .format(&key, Some(&args))
                    .is_some(),
                "Chinese: {key}"
            );
        }
        set_language(Language::English).unwrap();
        assert_eq!(count("selection-count", 1), "1 object selected");
        assert_eq!(count("selection-count", 3), "3 objects selected");
        set_language(Language::Chinese).unwrap();
        assert_eq!(count("selection-count", 3), "已选中 3 个对象");
        assert_eq!(
            message("copy-name", &[("name", "My 图层".into())]),
            "My 图层 副本"
        );
        set_language(Language::English).unwrap();
    }

    #[test]
    fn language_preference_round_trips_and_bad_settings_fall_back() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rovar/settings.json");
        load_settings(Some(path.clone()));
        assert_eq!(preference(), Language::System);
        for language in Language::available() {
            set_language(language).unwrap();
            assert_eq!(read_config(Some(&path)).unwrap()["language"], language.id());
            load_settings(Some(path.clone()));
            assert_eq!(preference(), language);
        }
        std::fs::write(&path, "not json").unwrap();
        load_settings(Some(path.clone()));
        assert_eq!(preference(), Language::System);
        assert!(set_language(Language::English).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json");
        std::fs::write(
            &path,
            r#"{"language":"system","language_directories":["custom"],"titlebar":"hide","other":42}"#,
        )
        .unwrap();
        set_language(Language::Chinese).unwrap();
        let config = read_config(Some(&path)).unwrap();
        assert_eq!(config["language_directories"][0], "custom");
        assert_eq!(config["other"], 42);
        assert_eq!(config["titlebar"], "hide");
        load_settings(None);
        set_language(Language::English).unwrap();
    }

    #[test]
    fn external_packs_extend_override_and_fall_back_safely() {
        let first = tempfile::tempdir().unwrap();
        let last = tempfile::tempdir().unwrap();
        std::fs::write(first.path().join("zh-CN.ftl"), "fill = 第一份\n").unwrap();
        std::fs::write(
            last.path().join("zh-CN.ftl"),
            "fill = 自定义填充\nundo = { missing }\n",
        )
        .unwrap();
        std::fs::write(last.path().join("en-US.ftl"), "undo = { also-missing }\n").unwrap();
        std::fs::write(last.path().join("ja-JP.ftl"), "language-name = {\n").unwrap();
        std::fs::write(
            last.path().join("fr-FR.ftl"),
            "language-name = Français\nfill = Remplissage\nselection-count = { $count } objets\n",
        )
        .unwrap();
        let packs = Catalogs::load(&[first.path().into(), last.path().into()]);
        assert_eq!(packs.text("zh-CN", "fill"), "自定义填充");
        assert_eq!(packs.text("zh-CN", "undo"), "Undo");
        assert_eq!(packs.text("fr-FR", "fill"), "Remplissage");
        assert_eq!(packs.text("fr-FR", "redo"), "Redo");
        assert_eq!(packs.text("fr-FR", "language-name"), "Français");
        assert_eq!(packs.resolve(&["fr-CA"]), "fr-FR");
        assert!(!packs.languages.contains_key("ja-JP"));
        let mut args = FluentArgs::new();
        args.set("count", 3);
        assert_eq!(packs.format("fr-FR", "selection-count", &args), "3 objets");
    }
}
