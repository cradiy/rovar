use super::*;
use crate::{application::ports::ContentStorage, infrastructure::storage::ContentStore};
use futures_util::StreamExt;

fn expected(bytes: &[u8]) -> Media {
    Media {
        hash: format!("{:x}", Sha256::digest(bytes)),
        length: bytes.len() as u64,
    }
}

async fn save(store: &ContentStore, bytes: &[u8]) -> String {
    let mut writer = store.create_media("space/media/hash".into()).await.unwrap();
    // Network boundaries do not define the authenticated segment boundaries.
    for chunk in bytes.chunks(73 * 1024 + 3) {
        writer.write(chunk).await.unwrap();
    }
    writer.finish().await.unwrap()
}

#[tokio::test]
async fn media_round_trips_in_bounded_segments_after_reopening_storage() {
    let root = tempfile::tempdir().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    for length in [0, CHUNK_BYTES, CHUNK_BYTES * 2 + 31] {
        let bytes: Vec<_> = (0..length).map(|i| (i / CHUNK_BYTES + 40) as u8).collect();
        let blob = save(&store, &bytes).await;
        let encrypted = std::fs::read(root.path().join("blobs").join(&blob)).unwrap();
        assert!(encrypted.starts_with(b"ROVMED01"));
        if !bytes.is_empty() {
            assert!(!encrypted.windows(128).any(|window| window == &bytes[..128]));
        }
        let reopened = ContentStore::open(root.path()).unwrap();
        let mut stream = reopened
            .read_media(&blob, "space/media/hash".into(), expected(&bytes))
            .await
            .unwrap();
        let mut downloaded = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            assert!(chunk.len() <= CHUNK_BYTES);
            downloaded.extend_from_slice(&chunk);
        }
        assert_eq!(downloaded, bytes);
        assert!(
            reopened
                .read_media(&blob, "other-space/media/hash".into(), expected(&bytes))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn media_rejects_tampering_truncation_reordering_and_wrong_manifest() {
    let root = tempfile::tempdir().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    let bytes = vec![42; CHUNK_BYTES * 2 + 31];
    let blob = save(&store, &bytes).await;
    let path = root.path().join("blobs").join(&blob);
    let original = std::fs::read(&path).unwrap();
    let record = 4 + CHUNK_BYTES + TAG_BYTES;
    let mut variants = Vec::new();
    let mut changed = original.clone();
    changed[HEADER_BYTES + 4] ^= 1;
    variants.push(changed);
    variants.push(original[..original.len() - 1].to_vec());
    variants.push(original[..original.len() - 20].to_vec());
    let mut changed = original.clone();
    changed.extend_from_slice(b"trailing");
    variants.push(changed);
    let mut changed = original.clone();
    changed[HEADER_BYTES..HEADER_BYTES + record]
        .copy_from_slice(&original[HEADER_BYTES + record..HEADER_BYTES + 2 * record]);
    changed[HEADER_BYTES + record..HEADER_BYTES + 2 * record]
        .copy_from_slice(&original[HEADER_BYTES..HEADER_BYTES + record]);
    variants.push(changed);
    let mut changed = original.clone();
    changed[HEADER_BYTES..HEADER_BYTES + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    variants.push(changed);
    for changed in variants {
        std::fs::write(&path, changed).unwrap();
        let mut stream = store
            .read_media(&blob, "space/media/hash".into(), expected(&bytes))
            .await
            .unwrap();
        let mut delivered = 0;
        loop {
            match stream.next().await {
                Some(Ok(chunk)) => delivered += chunk.len(),
                Some(Err(_)) => break,
                None => panic!("Damaged media completed successfully"),
            }
        }
        assert!(
            delivered < bytes.len(),
            "Corruption must fail before the full Content-Length is delivered"
        );
    }
    std::fs::write(&path, original).unwrap();
    for manifest in [
        Media {
            hash: "0".repeat(64),
            ..expected(&bytes)
        },
        Media {
            length: bytes.len() as u64 - 1,
            ..expected(&bytes)
        },
    ] {
        let mut stream = store
            .read_media(&blob, "space/media/hash".into(), manifest)
            .await
            .unwrap();
        let mut failed = false;
        while let Some(chunk) = stream.next().await {
            if chunk.is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed);
    }
}

#[tokio::test]
async fn unfinished_writes_leave_no_published_or_temporary_media() {
    let root = tempfile::tempdir().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    let mut writer = store.create_media("space/media/hash".into()).await.unwrap();
    writer.write(&vec![17; CHUNK_BYTES + 1]).await.unwrap();
    assert_eq!(
        std::fs::read_dir(root.path().join("blobs"))
            .unwrap()
            .count(),
        1
    );
    drop(writer);
    assert_eq!(
        std::fs::read_dir(root.path().join("blobs"))
            .unwrap()
            .count(),
        0
    );
    let blob = save(&store, b"complete").await;
    let files = std::fs::read_dir(root.path().join("blobs"))
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].as_ref().unwrap().file_name().to_str(),
        Some(blob.as_str())
    );
}
