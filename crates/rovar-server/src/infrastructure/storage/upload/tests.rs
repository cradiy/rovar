use super::*;
use crate::{application::ports::ContentStorage, infrastructure::storage::ContentStore};
use futures_util::StreamExt;

fn item(bytes: &[u8]) -> Media {
    Media {
        hash: format!("{:x}", Sha256::digest(bytes)),
        length: bytes.len() as u64,
    }
}

#[tokio::test]
async fn acknowledged_parts_survive_restart_and_replay_without_duplication() {
    let root = tempfile::tempdir().unwrap();
    let bytes = vec![41; CHUNK as usize + 17];
    let expected = item(&bytes);
    let context = format!("space/media/{}", expected.hash);
    let store = ContentStore::open(root.path()).unwrap();
    let mut upload = store
        .media_upload(context.clone(), expected.clone())
        .await
        .unwrap();
    upload
        .append(0, bytes[..CHUNK as usize].to_vec())
        .await
        .unwrap();
    assert_eq!(upload.offset(), CHUNK);
    assert!(
        matches!(
            store.media_upload(context.clone(), expected.clone()).await,
            Err(Error::RateLimited)
        ),
        "Separate handles must not race the same durable progress"
    );
    drop((upload, store));
    let store = ContentStore::open(root.path()).unwrap();
    let mut upload = store
        .media_upload(context.clone(), expected.clone())
        .await
        .unwrap();
    assert_eq!(upload.offset(), CHUNK);
    upload
        .append(0, bytes[..CHUNK as usize].to_vec())
        .await
        .unwrap();
    assert_eq!(upload.offset(), CHUNK);
    assert!(matches!(
        upload.append(0, vec![42; CHUNK as usize]).await,
        Err(Error::MediaOffset)
    ));
    assert!(matches!(
        upload.append(CHUNK * 2, vec![1]).await,
        Err(Error::MediaOffset)
    ));
    assert!(matches!(
        upload.append(CHUNK, vec![41; 16]).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(upload.finish().await, Err(Error::MediaOffset)));
    assert_eq!(
        std::fs::read_dir(root.path().join("blobs"))
            .unwrap()
            .count(),
        0
    );
    upload
        .append(CHUNK, bytes[CHUNK as usize..].to_vec())
        .await
        .unwrap();
    drop(upload);
    let mut upload = store
        .media_upload(context.clone(), expected.clone())
        .await
        .unwrap();
    assert_eq!(upload.offset(), expected.length);
    let blob = upload.finish().await.unwrap();
    let mut stream = store
        .read_media(&blob, context.clone(), expected.clone())
        .await
        .unwrap();
    let mut downloaded = Vec::new();
    while let Some(chunk) = stream.next().await {
        downloaded.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(downloaded, bytes);
    upload.discard().await.unwrap();
    assert!(
        matches!(
            store.media_upload(context.clone(), expected.clone()).await,
            Err(Error::RateLimited)
        ),
        "Discard must not replace the stable lock inode"
    );
    drop(upload);
    assert_eq!(
        store
            .media_upload(context, expected)
            .await
            .unwrap()
            .offset(),
        0
    );
}

#[tokio::test]
async fn staged_ciphertext_is_scoped_and_corrupt_uploads_restart_without_publishing() {
    let root = tempfile::tempdir().unwrap();
    let store = ContentStore::open(root.path()).unwrap();
    let expected = item(b"abcdef");
    let context = "space/media/hash";
    let open = || {
        Upload::open(
            store.uploads.clone(),
            store.directory.clone(),
            store.key.clone(),
            context.into(),
            expected.clone(),
        )
    };
    let mut upload = open().unwrap();
    upload.append(0, b"abcdef".to_vec()).await.unwrap();
    let path = upload.directory.join("0");
    let encrypted = std::fs::read(&path).unwrap();
    assert!(encrypted.starts_with(b"ROVENC01"));
    assert!(!encrypted.windows(6).any(|part| part == b"abcdef"));
    assert_eq!(
        store
            .media_upload("other-space/media/hash".into(), expected.clone())
            .await
            .unwrap()
            .offset(),
        0
    );
    assert_eq!(
        store
            .media_upload(
                context.into(),
                Media {
                    length: 7,
                    ..expected.clone()
                }
            )
            .await
            .unwrap()
            .offset(),
        0
    );
    let mut changed = encrypted.clone();
    *changed.last_mut().unwrap() ^= 1;
    std::fs::write(&path, changed).unwrap();
    assert!(upload.finish().await.is_err());
    drop(upload);
    let mut upload = open().unwrap();
    assert_eq!(upload.offset(), 0);
    upload.append(0, b"abcxyz".to_vec()).await.unwrap();
    assert!(matches!(upload.finish().await, Err(Error::Invalid(_))));
    drop(upload);
    let mut upload = open().unwrap();
    assert_eq!(upload.offset(), 0);
    upload.append(0, b"abcdef".to_vec()).await.unwrap();
    std::fs::write(upload.directory.join("0"), &encrypted[..30]).unwrap();
    drop(upload);
    assert_eq!(open().unwrap().offset(), 0);
    assert_eq!(
        std::fs::read_dir(root.path().join("blobs"))
            .unwrap()
            .count(),
        0
    );
}
