use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
    time::{Duration, SystemTime},
};

const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub(super) const INTERVAL: Duration = Duration::from_secs(60 * 60);

pub(super) fn namespace(root: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".namespace.lock"))?;
    file.lock()?;
    Ok(file)
}

/// Only unpublished upload directories are eligible. Published blobs and
/// database references are deliberately outside this collector's namespace.
pub(super) fn sweep(root: &Path, now: SystemTime) -> io::Result<()> {
    let _namespace = namespace(root)?;
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|id| {
            id.len() == 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) else {
            continue;
        };
        if path.extension().is_none_or(|ext| ext != "lock") || !path.is_file() {
            continue;
        }
        let file = OpenOptions::new().read(true).write(true).open(&path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => continue,
            Err(fs::TryLockError::Error(error)) => return Err(error),
        }
        if now
            .duration_since(file.metadata()?.modified()?)
            .unwrap_or_default()
            < RETENTION
        {
            continue;
        }
        match fs::remove_dir_all(root.join(id)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        // New openers are excluded by the namespace lock even after dropping
        // this handle, which also permits removal on Windows.
        drop(file);
        fs::remove_file(path)?;
    }
    #[cfg(unix)]
    File::open(root)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::ports::ContentStorage, infrastructure::storage::ContentStore};
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn cleanup_preserves_active_and_recent_uploads_and_removes_expired_locks() {
        let root = tempfile::tempdir().unwrap();
        let store = ContentStore::open(root.path()).unwrap();
        let item = rovar_api::Media {
            hash: format!("{:x}", Sha256::digest(b"abc")),
            length: 3,
        };
        let expected = crate::domain::document::Media {
            hash: item.hash,
            length: item.length,
        };
        let mut upload = store
            .media_upload("space/media/test".into(), expected.clone())
            .await
            .unwrap();
        upload.append(0, b"abc".to_vec()).await.unwrap();
        let uploads = root.path().join("uploads");
        let lock_path = fs::read_dir(&uploads)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.extension().is_some_and(|e| e == "lock")
                    && !p.file_name().unwrap().to_str().unwrap().starts_with('.')
            })
            .unwrap();
        let staging = lock_path.with_extension("");
        let now = SystemTime::now();
        File::options()
            .write(true)
            .open(&lock_path)
            .unwrap()
            .set_modified(now - RETENTION - Duration::from_secs(1))
            .unwrap();
        sweep(&uploads, now).unwrap();
        assert!(
            staging.join("0").exists(),
            "Active uploads must survive even when old"
        );
        let blob = upload.finish().await.unwrap();
        drop(upload);
        // An actual resumed request refreshes the retention window.
        let resumed = store
            .media_upload("space/media/test".into(), expected.clone())
            .await
            .unwrap();
        assert_eq!(resumed.offset(), 3);
        drop(resumed);
        sweep(&uploads, now).unwrap();
        assert!(staging.exists());
        sweep(&uploads, now + RETENTION + Duration::from_secs(60)).unwrap();
        assert!(!staging.exists());
        assert!(!lock_path.exists());
        assert!(root.path().join("blobs").join(blob).exists());
        assert_eq!(
            store
                .media_upload("space/media/test".into(), expected)
                .await
                .unwrap()
                .offset(),
            0
        );
    }
}
