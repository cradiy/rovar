use super::*;
use std::collections::BTreeSet;

const RETENTION: u64 = 7 * 24 * 60 * 60;

pub(super) fn now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn namespace(root: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".namespace.lock"))?;
    #[cfg(not(target_family = "wasm"))]
    file.lock()?;
    #[cfg(target_family = "wasm")]
    file.try_lock()?;
    Ok(file)
}

fn updated(root: &Path, id: &str) -> Result<u64> {
    let checkpoint = root.join(format!("{id}.json"));
    let state = (|| -> Result<Checkpoint> {
        let mut bytes = Vec::new();
        fs::File::open(&checkpoint)?
            .take(4097)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 4096, "Invalid checkpoint");
        Ok(serde_json::from_slice(&bytes)?)
    })();
    if let Ok(state) = state {
        return Ok(state.updated);
    }
    // Uncheckpointed fragments still get a grace period; metadata is a fallback
    // only. Normal checkpoints persist their age across browser restarts.
    let path = root.join(format!("{id}.part"));
    let metadata = fs::metadata(path)
        .or_else(|_| fs::metadata(&checkpoint))
        .or_else(|_| fs::metadata(checkpoint.with_extension("tmp")))?;
    Ok(metadata
        .modified()?
        .duration_since(web_time::SystemTime::UNIX_EPOCH)?
        .as_secs())
}

fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn sweep(root: &Path, now: u64, budget: u64) -> Result<()> {
    if !rovar_storage::exists(root) {
        return Ok(());
    }
    // Every opener holds this lock until it has locked the data file. This
    // prevents unlink/recreate races without accumulating per-entry lock files.
    let _namespace = namespace(root)?;
    let mut ids = BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path
            .extension()
            .is_none_or(|ext| ext != "part" && ext != "json" && ext != "tmp")
        {
            continue;
        }
        if let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|id| {
            id.len() == 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) {
            ids.insert(id.to_owned());
        }
    }
    let mut entries = Vec::new();
    let mut total = 0u64;
    for id in ids {
        let length = fs::metadata(root.join(format!("{id}.part")))
            .map(|m| m.len())
            .unwrap_or(0);
        total = total.saturating_add(length);
        entries.push((updated(root, &id)?, id, length));
    }
    entries.sort_unstable();
    for (last_used, id, length) in entries {
        if now.saturating_sub(last_used) < RETENTION && total <= budget {
            continue;
        }
        let part = root.join(format!("{id}.part"));
        let file = match fs::OpenOptions::new().read(true).write(true).open(&part) {
            Ok(file) => {
                if file.try_lock().is_err() {
                    continue;
                }
                Some(file)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        // A writer may have finished between enumeration and acquiring its
        // lock. Leave the freshly updated entry to the next sweep.
        if updated(root, &id)? != last_used {
            continue;
        }
        drop(file);
        remove(&root.join(format!("{id}.json")))?;
        remove(&root.join(format!("{id}.tmp")))?;
        remove(&part)?;
        total = total.saturating_sub(length);
    }
    Ok(())
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    fn seed(root: &Path, name: &str, time: u64) -> Cache {
        let client = Client::new("http://localhost").unwrap();
        let item = rovar_api::Media {
            hash: hex::encode(Sha256::digest(b"abcdef")),
            length: 6,
        };
        let mut cache = Cache::open(root, &client, name, &item).unwrap();
        cache.file.write_all(b"abc").unwrap();
        cache.hash.update(b"abc");
        cache.offset = 3;
        cache.save().unwrap();
        let mut state: Checkpoint =
            serde_json::from_slice(&fs::read(&cache.checkpoint).unwrap()).unwrap();
        state.updated = time;
        fs::write(&cache.checkpoint, serde_json::to_vec(&state).unwrap()).unwrap();
        cache
    }

    #[test]
    fn eviction_expires_idle_entries_and_preserves_active_downloads_under_pressure() {
        let root = tempfile::tempdir().unwrap();
        let time = now();
        let old = seed(root.path(), "old", time - RETENTION - 1);
        let old_path = old.checkpoint.clone();
        drop(old);
        let active = seed(root.path(), "active", time - RETENTION - 1);
        let active_path = active.checkpoint.clone();
        let fresh = seed(root.path(), "fresh", time);
        let fresh_path = fresh.checkpoint.clone();
        drop(fresh);
        let orphan = seed(root.path(), "orphan", time - RETENTION - 1);
        let orphan_path = orphan.checkpoint.clone();
        drop(orphan);
        fs::remove_file(orphan_path.with_extension("part")).unwrap();
        let temporary = root.path().join(format!("{}.tmp", "f".repeat(64)));
        fs::write(&temporary, b"interrupted checkpoint").unwrap();
        fs::File::options()
            .write(true)
            .open(&temporary)
            .unwrap()
            .set_modified(
                std::time::SystemTime::UNIX_EPOCH
                    + std::time::Duration::from_secs(time - RETENTION - 1),
            )
            .unwrap();
        fs::write(root.path().join("unrelated"), b"keep").unwrap();

        sweep(root.path(), time, 6).unwrap();
        assert!(!old_path.exists() && !old_path.with_extension("part").exists());
        assert!(!orphan_path.exists());
        assert!(!temporary.exists());
        assert!(fresh_path.exists() && active_path.exists());
        sweep(root.path(), time, 0).unwrap();
        assert!(!fresh_path.exists() && !fresh_path.with_extension("part").exists());
        assert!(
            active_path.exists(),
            "An active transfer may temporarily exceed the budget"
        );
        assert_eq!(
            fs::read(active_path.with_extension("part")).unwrap(),
            b"abc"
        );
        drop(active);
        sweep(root.path(), time, 6).unwrap();
        assert!(!active_path.exists());
        assert_eq!(fs::read(root.path().join("unrelated")).unwrap(), b"keep");
        let client = Client::new("http://localhost").unwrap();
        let item = rovar_api::Media {
            hash: hex::encode(Sha256::digest(b"abcdef")),
            length: 6,
        };
        assert_eq!(
            Cache::open(root.path(), &client, "active", &item)
                .unwrap()
                .offset,
            0
        );
    }

    #[test]
    fn capacity_eviction_uses_persisted_last_access_and_keeps_recent_resume_state() {
        let root = tempfile::tempdir().unwrap();
        let time = now();
        let older = seed(root.path(), "older", time - 60);
        let old_path = older.checkpoint.clone();
        drop(older);
        let recent = seed(root.path(), "recent", time);
        let recent_path = recent.checkpoint.clone();
        drop(recent);
        sweep(root.path(), time, 3).unwrap();
        assert!(!old_path.exists());
        assert!(recent_path.exists());
        let client = Client::new("http://localhost").unwrap();
        let item = rovar_api::Media {
            hash: hex::encode(Sha256::digest(b"abcdef")),
            length: 6,
        };
        assert_eq!(
            Cache::open(root.path(), &client, "recent", &item)
                .unwrap()
                .offset,
            3
        );
    }
}
