use super::*;
use serde::Deserialize;
use std::{
    fs::{self, File},
    time::{Duration, SystemTime},
};

pub(crate) const BATCH: usize = 256;

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
        let mut more =
            crate::infrastructure::postgres::retention::expire(pool, policy, timestamp).await?;
        let mut entries = tokio::fs::read_dir(&self.directory).await?;
        let mut batch = Vec::new();
        let mut removed = 0;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| valid_blob(name)) else {
                continue;
            };
            if !entry.file_type().await?.is_file() {
                continue;
            }
            let age = now
                .duration_since(entry.metadata().await?.modified()?)
                .unwrap_or_default();
            if age < Duration::from_secs(u64::from(policy.orphan_days) * 86400) {
                continue;
            }
            batch.push(name.to_owned());
            if batch.len() == BATCH {
                removed += self.remove_orphans(pool, &batch, BATCH - removed).await?;
                batch.clear();
                if removed >= BATCH {
                    more = true;
                    break;
                }
            }
        }
        if !batch.is_empty() {
            self.remove_orphans(pool, &batch, BATCH - removed).await?;
        }
        Ok(Some(more))
    }

    async fn remove_orphans(
        &self,
        pool: &sqlx::PgPool,
        batch: &[String],
        budget: usize,
    ) -> Result<usize> {
        let referenced =
            crate::infrastructure::postgres::retention::references(pool, batch).await?;
        let mut removed = 0;
        for blob in batch
            .iter()
            .filter(|blob| !referenced.contains(*blob))
            .take(budget)
        {
            match tokio::fs::remove_file(self.directory.join(blob)).await {
                Ok(()) => removed += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(removed)
    }
}

fn valid_blob(name: &str) -> bool {
    uuid::Uuid::parse_str(name).is_ok_and(|id| id.to_string() == name)
}

#[cfg(test)]
mod tests;
