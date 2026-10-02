use super::*;
use rovar_api::{Media, MediaContent};

/// The retry record remains the complete immutable local snapshot. Splitting it
/// deterministically on every attempt preserves the idempotency fingerprint.
pub(super) async fn prepare(client: &Client, space: &str, input: &Save) -> Result<Save> {
    let mut transfer = input.clone();
    if input.deleted || input.kind == Kind::ColorStyle {
        return Ok(transfer);
    }
    let source = rovar_storage::tempfile::NamedTempFile::new()?;
    rovar_storage::fs::write(source.path(), STANDARD.decode(&input.content)?)?;
    let Ok(reader) = rovar_format::Reader::open(source.path()) else {
        return Ok(transfer);
    };
    let media: Vec<_> = reader
        .entries()
        .filter(|(_, block)| block.kind == "media")
        .map(|(key, block)| {
            (
                key.to_owned(),
                Media {
                    hash: hex::encode(block.hash),
                    length: block.length,
                },
            )
        })
        .collect();
    if media.is_empty() {
        return Ok(transfer);
    }
    for (key, item) in &media {
        ensure!(
            *key == format!("media/{}", item.hash),
            "Invalid media block name"
        );
    }
    transfer.media = media.iter().map(|(_, item)| item.clone()).collect();
    let missing: Vec<String> = client
        .json(
            "POST",
            &format!("spaces/{space}/media/missing"),
            Some(serde_json::to_value(&transfer.media)?),
        )
        .await?;
    let expected: BTreeSet<_> = transfer.media.iter().map(|m| m.hash.as_str()).collect();
    ensure!(
        missing.iter().all(|hash| expected.contains(hash.as_str())),
        "Server requested unknown media"
    );
    for hash in missing {
        let bytes = reader.read(
            &format!("media/{hash}"),
            rovar_api::MAX_CONTENT_BYTES as u64,
        )?;
        let _: () = client
            .json(
                "PUT",
                &format!("spaces/{space}/media/{hash}"),
                Some(serde_json::to_value(MediaContent {
                    content: STANDARD.encode(bytes),
                })?),
            )
            .await?;
    }
    let reduced = rovar_storage::tempfile::NamedTempFile::new()?;
    let mut writer = rovar_format::Writer::create(reduced.path())?;
    for (key, block) in reader.entries().filter(|(_, block)| block.kind != "media") {
        writer.put(
            key,
            &block.kind,
            reader.block(key)?.reader(),
            Some(block.hash),
        )?;
    }
    writer.commit()?;
    drop(writer);
    transfer.content = STANDARD.encode(rovar_storage::fs::read(reduced.path())?);
    Ok(transfer)
}

pub(super) async fn hydrate(
    client: &Client,
    space: &str,
    snapshot: &Snapshot,
    local: &Path,
) -> Result<Vec<u8>> {
    let bytes = STANDARD.decode(&snapshot.content)?;
    if snapshot.media.is_empty() {
        return Ok(bytes);
    }
    ensure!(snapshot.media.len() <= 4096, "Too many media references");
    let mut seen = BTreeSet::new();
    let mut total = bytes.len() as u64;
    for item in &snapshot.media {
        ensure!(
            item.hash.len() == 64
                && item
                    .hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && seen.insert(&item.hash),
            "Invalid media reference"
        );
        total = total.saturating_add(item.length);
    }
    ensure!(
        total <= rovar_api::MAX_CONTENT_BYTES as u64,
        "Document exceeds the server's size limit"
    );
    let output = rovar_storage::tempfile::NamedTempFile::new()?;
    rovar_storage::fs::write(output.path(), bytes)?;
    let mut writer = rovar_format::Writer::open(output.path())?;
    let previous = rovar_format::Reader::open(local).ok();
    for item in &snapshot.media {
        let key = format!("media/{}", item.hash);
        let cached = previous.as_ref().and_then(|reader| {
            let block = reader.entry(&key)?;
            (block.length == item.length && hex::encode(block.hash) == item.hash)
                .then(|| reader.read(&key, item.length).ok())
                .flatten()
        });
        let bytes = if let Some(bytes) = cached {
            bytes
        } else {
            let content: MediaContent = client
                .json("GET", &format!("spaces/{space}/media/{}", item.hash), None)
                .await?;
            STANDARD.decode(content.content)?
        };
        ensure!(
            bytes.len() as u64 == item.length && hex::encode(Sha256::digest(&bytes)) == item.hash,
            "Media checksum mismatch"
        );
        writer.put_bytes(&key, "media", &bytes)?;
    }
    writer.commit()?;
    drop(writer);
    Ok(rovar_storage::fs::read(output.path())?)
}
