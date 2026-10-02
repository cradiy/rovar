use super::*;
use rovar_format::BlockHandle;
use rovar_storage::{fs, tempfile::NamedTempFile};
use sha2::{Digest, Sha256};
use std::io::Write;

impl Client {
    pub async fn upload_media(&self, path: &str, block: BlockHandle) -> Result<()> {
        ensure!(
            block.info.length <= rovar_api::MAX_MEDIA_BYTES as u64,
            "Media exceeds the size limit"
        );
        let client = self.clone();
        let path = path.to_owned();
        execute(async move {
            let length = block.info.length;
            #[cfg(not(target_family = "wasm"))]
            let body = {
                let stream =
                    futures_util::stream::try_unfold(block.reader(), |mut reader| async move {
                        tokio::task::spawn_blocking(move || -> Result<_> {
                            use std::io::Read;
                            let mut bytes = vec![0; 64 * 1024];
                            let n = reader.read(&mut bytes)?;
                            bytes.truncate(n);
                            Ok((n != 0).then_some((bytes, reader)))
                        })
                        .await?
                    });
                reqwest::Body::wrap_stream(stream)
            };
            // Fetch upload streaming is not portable across browsers. Keep the
            // bounded binary body here; downloads stream on both platforms.
            #[cfg(target_family = "wasm")]
            let body = {
                let mut bytes = Vec::new();
                block.copy_verified(&mut bytes)?;
                reqwest::Body::from(bytes)
            };
            let request = client
                .request("PUT", &path)?
                .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                .header(reqwest::header::CONTENT_LENGTH, length)
                .body(body);
            let response = successful(request.send().await?).await?;
            bounded_body(response, 64 * 1024).await?;
            Ok(())
        })
        .await
    }

    pub async fn download_media(
        &self,
        path: &str,
        item: &rovar_api::Media,
    ) -> Result<NamedTempFile> {
        ensure!(
            item.length <= rovar_api::MAX_MEDIA_BYTES as u64,
            "Media exceeds the size limit"
        );
        let client = self.clone();
        let path = path.to_owned();
        let item = item.clone();
        execute(async move {
            let response = successful(client.request("GET", &path)?.send().await?).await?;
            ensure!(
                response.content_length().is_none_or(|n| n == item.length),
                "Media checksum or length mismatch"
            );
            let output = NamedTempFile::new()?;
            let mut file = fs::File::create(output.path())?;
            let mut stream = response.bytes_stream();
            let mut length = 0u64;
            let mut hash = Sha256::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                ensure!(
                    chunk.len() as u64 <= item.length.saturating_sub(length),
                    "Media exceeds the declared length"
                );
                file.write_all(&chunk)?;
                hash.update(&chunk);
                length += chunk.len() as u64;
            }
            ensure!(
                length == item.length && hex::encode(hash.finalize()) == item.hash,
                "Media checksum mismatch"
            );
            file.flush()?;
            Ok(output)
        })
        .await
    }
}
