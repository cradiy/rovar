use anyhow::Context as _;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) fn bundled() -> String {
    include_str!("../../../../skills/rovar-design/SKILL.md")
        .replace("\r\n", "\n")
        .replace("__ROVAR_VERSION__", env!("CARGO_PKG_VERSION"))
}

pub(crate) fn fingerprint() -> String {
    hex::encode(Sha256::digest(bundled().as_bytes()))
}

pub(crate) fn default_directory() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".agents").join("skills"))
}

pub(crate) struct Status {
    pub installed: bool,
    pub version: Option<String>,
    pub current: bool,
}

pub(crate) fn status(directory: &Path) -> anyhow::Result<Status> {
    let path = directory.join("rovar-design").join("SKILL.md");
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Status {
                installed: false,
                version: None,
                current: false,
            });
        }
        Err(error) => return Err(error).with_context(|| format!("Cannot read {}", path.display())),
    };
    let mut source = String::new();
    file.take(256 * 1024).read_to_string(&mut source)?;
    let header_source = source.replace("\r\n", "\n");
    let version = header_source
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .and_then(|(header, _)| {
            header.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("rovar-version:")
                    .map(|version| version.trim().trim_matches('"').to_owned())
            })
        });
    Ok(Status {
        installed: true,
        version,
        current: source == bundled(),
    })
}

/// Replace only the owned entrypoint; keep other skills and user files intact.
pub(crate) fn install(directory: &Path) -> anyhow::Result<Status> {
    let destination = directory.join("rovar-design");
    std::fs::create_dir_all(&destination)
        .with_context(|| format!("Cannot create {}", destination.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(&destination)?;
    file.write_all(bundled().as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(destination.join("SKILL.md"))?;
    status(directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installing_updates_stale_instructions_and_preserves_other_files() {
        let root = tempfile::tempdir().unwrap();
        assert!(!status(root.path()).unwrap().installed);
        let installed = install(root.path()).unwrap();
        assert!(installed.current);
        assert_eq!(
            installed.version.as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        let file = root.path().join("rovar-design/SKILL.md");
        let bytes = std::fs::read(&file).unwrap();
        assert_eq!(hex::encode(Sha256::digest(&bytes)), fingerprint());
        std::fs::write(root.path().join("rovar-design/notes.txt"), "keep").unwrap();
        std::fs::create_dir(root.path().join("other-skill")).unwrap();
        std::fs::write(root.path().join("other-skill/SKILL.md"), "other").unwrap();
        std::fs::write(&file, bundled().replace(env!("CARGO_PKG_VERSION"), "0.0.1")).unwrap();
        let old = status(root.path()).unwrap();
        assert_eq!(old.version.as_deref(), Some("0.0.1"));
        assert!(!old.current);
        std::fs::write(&file, bundled().replace('\n', "\r\n")).unwrap();
        let crlf = status(root.path()).unwrap();
        assert_eq!(crlf.version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
        assert!(!crlf.current);
        std::fs::write(&file, format!("{}\nChanged", bundled())).unwrap();
        assert!(!status(root.path()).unwrap().current);
        assert!(install(root.path()).unwrap().current);
        assert_eq!(
            std::fs::read_to_string(root.path().join("rovar-design/notes.txt")).unwrap(),
            "keep"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("other-skill/SKILL.md")).unwrap(),
            "other"
        );
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        assert!(install(root.path()).is_err());
        assert!(file.is_dir());
    }
}
