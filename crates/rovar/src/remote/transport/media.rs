use super::*;
use rovar_format::BlockHandle;

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
