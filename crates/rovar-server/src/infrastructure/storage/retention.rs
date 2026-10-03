use super::*;
use serde::Deserialize;
use std::{
    fs::{self, File},
    time::{Duration, SystemTime},
};

pub(crate) const BATCH: usize = 256;
const SCAN_BATCH: usize = 1024;

#[derive(Default)]
pub(super) struct Scan {
    entries: Option<tokio::fs::ReadDir>,
    pending: Vec<String>,
}

impl Scan {
    async fn collect(&mut self, directory: &Path, policy: Policy, now: SystemTime) -> Result<()> {
        // Keep candidates when the store is busy or a previous database call
        // failed. Restarting at the directory's beginning can starve later files.
        if !self.pending.is_empty() {
            return Ok(());
        }
        if self.entries.is_none() {
            self.entries = Some(tokio::fs::read_dir(directory).await?);
        }
        for _ in 0..SCAN_BATCH {
            let Some(entry) = self.entries.as_mut().unwrap().next_entry().await? else {
                self.entries = None;
                break;
            };
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| valid_blob(name)) else {
                continue;
            };
            if eligible(&entry.path(), policy, now).await? {
                self.pending.push(name.to_owned());
                if self.pending.len() == BATCH {
                    break;
                }
            }
        }
        Ok(())
    }
}

async fn eligible(path: &Path, policy: Policy, now: SystemTime) -> Result<bool> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(metadata.is_file()
        && now.duration_since(metadata.modified()?).unwrap_or_default()
            >= Duration::from_secs(u64::from(policy.orphan_days) * 86400))
}

#[derive(Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub history_days: u32,
    pub history_versions: u32,
    pub orphan_days: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            history_days: 30,
            history_versions: 100,
            orphan_days: 7,
        }
    }
}

impl Policy {
    pub fn validate(self) -> Result<()> {
        anyhow::ensure!(
            self.history_days > 0 && self.history_versions > 0 && self.orphan_days > 0,
            "Storage retention days and version count must be positive"
        );
        Ok(())
    }
}

pub(super) fn open_lock(path: &Path) -> std::io::Result<File> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

impl ContentStore {
    pub fn start_retention(self: &std::sync::Arc<Self>, pool: sqlx::PgPool, policy: Policy) {
        let weak = std::sync::Arc::downgrade(self);
        tokio::spawn(async move {
            while let Some(store) = weak.upgrade() {
                let delay = match store.reclaim(&pool, policy, SystemTime::now()).await {
                    Ok(Some(false)) => Duration::from_secs(3600),
                    Ok(_) => Duration::from_secs(60),
                    Err(error) => {
                        eprintln!("Content retention failed: {error}");
                        Duration::from_secs(3600)
                    }
                };
                drop(store);
                tokio::time::sleep(delay).await;
            }
        });
    }

    /// None means an active publisher/reader (possibly in another process) owns
    /// the store. Never wait for it or delete based on a stale reference set.
    pub(crate) async fn reclaim(
        &self,
        pool: &sqlx::PgPool,
        policy: Policy,
        now: SystemTime,
    ) -> Result<Option<bool>> {
        policy.validate()?;
        let Ok(mut scan) = self.retention_scan.try_lock() else {
            return Ok(None);
        };
        // Directory traversal never holds the cross-process content lease.
        // Bound examined entries as well as candidates, including live stores
        // where most files are referenced or too recent to be collected.
        scan.collect(&self.directory, policy, now).await?;
        let activity = self.activity.clone();
        let guard = tokio::task::spawn_blocking(move || -> Result<Option<File>> {
            let file = open_lock(&activity)?;
            match file.try_lock() {
                Ok(()) => Ok(Some(file)),
                Err(fs::TryLockError::WouldBlock) => Ok(None),
                Err(fs::TryLockError::Error(error)) => Err(error.into()),
            }
        })
        .await??;
        let Some(_guard) = guard else { return Ok(None) };
        let timestamp = i64::try_from(now.duration_since(SystemTime::UNIX_EPOCH)?.as_secs())?;
        let more =
            crate::infrastructure::postgres::retention::expire(pool, policy, timestamp).await?;
        if !scan.pending.is_empty() {
            self.remove_orphans(pool, &scan.pending, policy, now)
                .await?;
        }
        scan.pending.clear();
        Ok(Some(more || scan.entries.is_some()))
    }

    async fn remove_orphans(
        &self,
        pool: &sqlx::PgPool,
        batch: &[String],
        policy: Policy,
        now: SystemTime,
    ) -> Result<()> {
        let referenced =
            crate::infrastructure::postgres::retention::references(pool, batch).await?;
        for blob in batch.iter().filter(|blob| !referenced.contains(*blob)) {
            let path = self.directory.join(blob);
            // Scanning happened without a lease, possibly on an earlier tick.
            // Recheck the current file under the exclusive lease before deletion.
            if !eligible(&path, policy, now).await? {
                continue;
            }
            match tokio::fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

fn valid_blob(name: &str) -> bool {
    uuid::Uuid::parse_str(name).is_ok_and(|id| id.to_string() == name)
}

#[cfg(test)]
mod tests;
