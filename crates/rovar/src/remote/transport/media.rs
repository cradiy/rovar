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
            let route = format!("{path}/upload?length={length}");
            let mut progress: rovar_api::MediaUpload = client.json("GET", &route, None).await?;
            validate_progress(&progress, length)?;
            let mut resyncs = 0;
            while progress.offset < length {
                let offset = progress.offset;
                let count =
                    (length - offset).min(rovar_api::MEDIA_UPLOAD_CHUNK_BYTES as u64) as usize;
                let source = block.clone();
                let read = move || -> Result<Vec<u8>> {
                    use std::io::{Read, Seek, SeekFrom};
                    let mut reader = source.reader();
                    reader.seek(SeekFrom::Start(offset))?;
                    let mut bytes = vec![0; count];
                    reader.read_exact(&mut bytes)?;
                    Ok(bytes)
                };
                #[cfg(not(target_family = "wasm"))]
                let bytes = tokio::task::spawn_blocking(read).await??;
                #[cfg(target_family = "wasm")]
                let bytes = read()?;
                let response = client
                    .request("PUT", &format!("{route}&offset={offset}"))?
                    .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                    .body(bytes)
                    .send()
                    .await?;
                let response = match successful(response).await {
                    Ok(response) => response,
                    Err(error)
                        if resyncs < 2
                            && error.downcast_ref::<HttpError>().is_some_and(|error| {
                                error.status == 409 && error.code == "media_offset_mismatch"
                            }) =>
                    {
                        progress = client.json("GET", &route, None).await?;
                        validate_progress(&progress, length)?;
                        resyncs += 1;
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                progress = serde_json::from_slice(&bounded_body(response, 64 * 1024).await?)?;
                validate_progress(&progress, length)?;
                ensure!(
                    progress.offset > offset,
                    "Server did not acknowledge media upload progress"
                );
            }
            if !progress.complete {
                let completed: rovar_api::MediaUpload = client.json("POST", &route, None).await?;
                validate_progress(&completed, length)?;
                ensure!(
                    completed.complete,
                    "Server did not complete the media upload"
                );
            }
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

fn validate_progress(progress: &rovar_api::MediaUpload, length: u64) -> Result<()> {
    ensure!(
        progress.offset <= length
            && (progress.offset == length
                || progress
                    .offset
                    .is_multiple_of(rovar_api::MEDIA_UPLOAD_CHUNK_BYTES as u64))
            && (!progress.complete || progress.offset == length),
        "Invalid media upload progress"
    );
    Ok(())
}
