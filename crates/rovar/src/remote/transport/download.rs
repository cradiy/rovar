use super::*;
use rovar_storage::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const CHECKPOINT_BYTES: u64 = 1024 * 1024;

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    offset: u64,
    checksum: String,
}

struct Cache {
    file: fs::File,
    checkpoint: PathBuf,
    offset: u64,
    saved: u64,
    hash: Sha256,
    confirmed: bool,
}

impl Cache {
    fn open(root: &Path, client: &Client, path: &str, item: &rovar_api::Media) -> Result<Self> {
        let mut identity = Sha256::new();
        // Credential-derived names isolate accounts without writing credentials
        // into checkpoint contents. The path includes the workspace and hash.
        for field in [&client.url, &client.token, path, &item.hash] {
            identity.update((field.len() as u64).to_le_bytes());
            identity.update(field.as_bytes());
        }
        identity.update(item.length.to_le_bytes());
        let id = hex::encode(identity.finalize());
        fs::create_dir_all(root)?;
        let checkpoint = root.join(format!("{id}.json"));
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(root.join(format!("{id}.part")))?;
        file.try_lock()?;
        let restored = (|| -> Result<(u64, Sha256)> {
            let mut bytes = Vec::new();
            fs::File::open(&checkpoint)?
                .take(4097)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 4096, "Invalid download checkpoint");
            let state: Checkpoint = serde_json::from_slice(&bytes)?;
            ensure!(
                state.offset <= item.length && file.metadata()?.len() >= state.offset,
                "Incomplete download checkpoint"
            );
            let mut remaining = state.offset;
            let mut hash = Sha256::new();
            let mut buffer = vec![0; 64 * 1024];
            while remaining > 0 {
                let count = remaining.min(buffer.len() as u64) as usize;
                file.read_exact(&mut buffer[..count])?;
                hash.update(&buffer[..count]);
                remaining -= count as u64;
            }
            ensure!(
                hex::encode(hash.clone().finalize()) == state.checksum,
                "Damaged download checkpoint"
            );
            Ok((state.offset, hash))
        })();
        let confirmed = restored.is_ok();
        let (offset, hash) = restored.unwrap_or_else(|_| (0, Sha256::new()));
        // Discard any bytes written after the last durable checkpoint.
        file.set_len(offset)?;
        file.seek(SeekFrom::Start(offset))?;
        Ok(Self {
            file,
            checkpoint,
            offset,
            saved: offset,
            hash,
            confirmed,
        })
    }

    fn save(&mut self) -> Result<()> {
        self.file.sync_all()?;
        super::super::write_atomic(
            &self.checkpoint,
            &serde_json::to_vec(&Checkpoint {
                offset: self.offset,
                checksum: hex::encode(self.hash.clone().finalize()),
            })?,
        )?;
        self.saved = self.offset;
        self.confirmed = true;
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.offset = 0;
        self.hash = Sha256::new();
        self.save()
    }

    fn verified(mut self, item: &rovar_api::Media) -> Result<fs::File> {
        if self.offset != item.length || hex::encode(self.hash.clone().finalize()) != item.hash {
            self.reset()?;
            anyhow::bail!("Media checksum mismatch");
        }
        self.save()?;
        self.file.seek(SeekFrom::Start(0))?;
        Ok(self.file)
    }
}

impl Client {
    /// Returns a locked, verified file positioned at its start. Incomplete files
    /// and integrity checkpoints stay in the app cache across retries/restarts.
    pub async fn download_media(
        &self,
        path: &str,
        item: &rovar_api::Media,
        root: &Path,
    ) -> Result<fs::File> {
        ensure!(
            item.length <= rovar_api::MAX_MEDIA_BYTES as u64,
            "Media exceeds the size limit"
        );
        ensure!(
            item.hash.len() == 64
                && item
                    .hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Invalid media hash"
        );
        let client = self.clone();
        let path = path.to_owned();
        let item = item.clone();
        let root = root.to_path_buf();
        execute(async move {
            let mut cache = Cache::open(&root, &client, &path, &item)?;
            if cache.confirmed && cache.offset == item.length {
                if hex::encode(cache.hash.clone().finalize()) == item.hash {
                    return cache.verified(&item);
                }
                cache.reset()?;
            }
            let etag = format!("\"{}\"", item.hash);
            let mut full_retry = false;
            loop {
                let offset = cache.offset;
                let mut request = client
                    .request("GET", &path)?
                    .header(reqwest::header::ACCEPT_ENCODING, "identity");
                if offset > 0 {
                    request = request
                        .header(reqwest::header::RANGE, format!("bytes={offset}-"))
                        .header(reqwest::header::IF_RANGE, &etag);
                }
                let response = request.send().await?;
                if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
                    && offset > 0
                    && !full_retry
                {
                    cache.reset()?;
                    full_retry = true;
                    continue;
                }
                let response = successful(response).await?;
                let start = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                    let expected = format!(
                        "bytes {offset}-{}/{}",
                        item.length.saturating_sub(1),
                        item.length
                    );
                    ensure!(
                        offset > 0
                            && response
                                .headers()
                                .get(reqwest::header::CONTENT_RANGE)
                                .and_then(|h| h.to_str().ok())
                                == Some(expected.as_str())
                            && response
                                .headers()
                                .get(reqwest::header::ETAG)
                                .and_then(|h| h.to_str().ok())
                                == Some(etag.as_str()),
                        "Invalid media range response"
                    );
                    offset
                } else {
                    ensure!(
                        response.status() == reqwest::StatusCode::OK,
                        "Invalid media response status"
                    );
                    ensure!(
                        response
                            .headers()
                            .get(reqwest::header::ETAG)
                            .is_none_or(|h| h.to_str().ok() == Some(etag.as_str())),
                        "Media checksum or identity mismatch"
                    );
                    0
                };
                ensure!(
                    response
                        .content_length()
                        .is_none_or(|n| n == item.length - start),
                    "Media checksum or length mismatch"
                );
                // A server or proxy may ignore Range and return the full body.
                if start == 0 && offset != 0 {
                    cache.reset()?;
                }
                let mut stream = response.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    let chunk = match chunk {
                        Ok(chunk) => chunk,
                        Err(error) => {
                            cache.save()?;
                            return Err(error.into());
                        }
                    };
                    if chunk.len() as u64 > item.length.saturating_sub(cache.offset) {
                        cache.reset()?;
                        anyhow::bail!("Media exceeds the declared length");
                    }
                    cache.file.write_all(&chunk)?;
                    cache.hash.update(&chunk);
                    cache.offset += chunk.len() as u64;
                    if cache.offset - cache.saved >= CHECKPOINT_BYTES {
                        cache.save()?;
                    }
                }
                if cache.offset != item.length {
                    cache.save()?;
                    anyhow::bail!("Media checksum or length mismatch");
                }
                return cache.verified(&item);
            }
        })
        .await
    }
}
