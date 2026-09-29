use super::Catalog;
use fluent_bundle::{FluentArgs, FluentResource};
use fluent_langneg::{LanguageIdentifier, NegotiationStrategy, negotiate_languages};
use std::{collections::BTreeMap, path::PathBuf};

pub(super) struct Catalogs {
    pub languages: BTreeMap<String, Catalog>,
    english: Catalog,
}

impl Catalogs {
    pub fn load(directories: &[PathBuf]) -> Self {
        let mut resources: BTreeMap<String, Vec<FluentResource>> = [
            ("en-US", include_str!("../../locales/en-US.ftl")),
            ("zh-CN", include_str!("../../locales/zh-CN.ftl")),
        ]
        .into_iter()
        .map(|(id, source)| {
            (
                id.into(),
                vec![
                    FluentResource::try_new(source.into())
                        .expect("Invalid bundled Fluent resource"),
                ],
            )
        })
        .collect();
        for directory in directories {
            let entries = match rovar_storage::fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    eprintln!(
                        "Could not read language directory {}: {error}",
                        directory.display()
                    );
                    continue;
                }
            };
            let mut paths: Vec<_> = entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .collect();
            paths.sort();
            for path in paths {
                if path.extension().and_then(|ext| ext.to_str()) != Some("ftl") {
                    continue;
                }
                let Some(language) = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .and_then(|id| id.parse::<LanguageIdentifier>().ok())
                else {
                    continue;
                };
                match rovar_storage::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|source| {
                        FluentResource::try_new(source).map_err(|(_, errors)| format!("{errors:?}"))
                    }) {
                    Ok(resource) => resources
                        .entry(language.to_string())
                        .or_default()
                        .push(resource),
                    Err(error) => eprintln!("Skipping language file {}: {error}", path.display()),
                }
            }
        }
        Self {
            languages: resources
                .into_iter()
                .map(|(id, resources)| {
                    let catalog = Catalog::from_resources(&id, resources);
                    (id, catalog)
                })
                .collect(),
            english: Catalog::new("en-US", include_str!("../../locales/en-US.ftl")),
        }
    }

    pub fn resolve(&self, requested: &[&str]) -> &str {
        let requested: Vec<LanguageIdentifier> = requested
            .iter()
            .filter_map(|id| {
                id.split('.')
                    .next()
                    .unwrap_or(id)
                    .replace('_', "-")
                    .parse()
                    .ok()
            })
            .collect();
        let available: Vec<LanguageIdentifier> = self
            .languages
            .keys()
            .map(|id| id.parse().unwrap())
            .collect();
        let english = "en-US".parse().unwrap();
        let language = negotiate_languages(
            &requested,
            &available,
            Some(&english),
            NegotiationStrategy::Lookup,
        )
        .first()
        .map(|id| id.to_string())
        .unwrap_or_else(|| "en-US".into());
        self.languages
            .get_key_value(&language)
            .map_or("en-US", |(id, _)| id.as_str())
    }

    fn chain(&self, language: &str) -> impl Iterator<Item = &Catalog> {
        self.languages
            .get(language)
            .into_iter()
            .chain(self.languages.get("en-US"))
            .chain(std::iter::once(&self.english))
    }

    pub fn text<'a>(&'a self, language: &str, key: &'a str) -> &'a str {
        self.chain(language)
            .find_map(|catalog| catalog.labels.get(key))
            .map_or(key, String::as_str)
    }

    pub fn format(&self, language: &str, key: &str, args: &FluentArgs<'_>) -> String {
        self.chain(language)
            .find_map(|catalog| catalog.format(key, Some(args)))
            .unwrap_or_else(|| key.into())
    }
}
