use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: Server,
    pub database: Database,
    pub storage: Storage,
    #[serde(default)]
    pub registration: Registration,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub bind: String,
    pub public_origin: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Database {
    pub url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Storage {
    pub directory: PathBuf,
    #[serde(default)]
    pub retention: crate::infrastructure::storage::retention::Policy,
}
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Registration {
    pub personal: bool,
    pub teams: bool,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("Read {}", path.display()))?;
        let mut config: Self = toml::from_str(&text).context("Invalid server configuration")?;
        config.storage.retention.validate()?;
        let url = url::Url::parse(&config.server.public_origin)
            .context("Invalid server.public_origin")?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
                && url.username().is_empty()
                && url.password().is_none(),
            "server.public_origin must be an HTTP(S) origin"
        );
        config.server.public_origin = url.origin().ascii_serialization();
        config
            .server
            .bind
            .parse::<std::net::SocketAddr>()
            .context("Invalid server.bind")?;
        let database = url::Url::parse(&config.database.url).context("Invalid database.url")?;
        ensure!(
            matches!(database.scheme(), "postgres" | "postgresql"),
            "database.url must use PostgreSQL"
        );
        if config.storage.directory.is_relative() {
            config.storage.directory = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(&config.storage.directory);
        }
        Ok(config)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_resolves_storage_and_rejects_unsafe_origins_and_unknown_keys() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("server.toml");
        let example = include_str!("../../rovar-server.example.toml");
        std::fs::write(&path, example).unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.storage.directory, root.path().join("./data"));
        assert!(!config.registration.personal && !config.registration.teams);
        for invalid in [
            example.replace("http://127.0.0.1:8699", "https://example.com/path"),
            example.replace("http://127.0.0.1:8699", "https://user:password@example.com"),
            example.replace("personal = false", "personnal = true"),
            example.replace("history_versions = 100", "history_versions = 0"),
            example.replace("history_days = 30", "history_days = 0"),
            example.replace("orphan_days = 7", "orphan_days = 0"),
            example.replace("127.0.0.1:8699\"", "not-an-address\""),
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert!(Config::load(&path).is_err());
        }
    }
}
