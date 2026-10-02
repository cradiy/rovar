use crate::application::{
    documents::DocumentService,
    ports::{ContentStorage, ContentStream, DocumentWrite, Documents, MediaWriter, Preparation},
};
use crate::domain::document::{
    Changes, Document, DocumentKind, Media, SaveDocument, StoredVersion,
};
use crate::domain::error::{Error, Result};
use async_trait::async_trait;
use rovar_api::MAX_METADATA_BYTES;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{Mutex, OwnedMutexGuard};

struct State {
    document: Document,
    blob: String,
    requests: BTreeMap<String, (Vec<u8>, i64)>,
    base_reads: usize,
    versions: BTreeMap<i64, String>,
    media: BTreeMap<String, (String, u64)>,
}
struct Repository(Arc<Mutex<State>>);
struct Write {
    state: OwnedMutexGuard<State>,
    fingerprint: Vec<u8>,
}
struct Storage {
    blobs: Mutex<BTreeMap<String, (Vec<u8>, String)>>,
    media: crate::infrastructure::storage::ContentStore,
    _root: tempfile::TempDir,
}

impl Default for Storage {
    fn default() -> Self {
        let root = tempfile::tempdir().unwrap();
        Self {
            blobs: Mutex::default(),
            media: crate::infrastructure::storage::ContentStore::open(root.path()).unwrap(),
            _root: root,
        }
    }
}

#[async_trait]
impl ContentStorage for Storage {
    async fn write(&self, bytes: Vec<u8>, context: String) -> Result<String> {
        let mut blobs = self.blobs.lock().await;
        let id = blobs.len().to_string();
        blobs.insert(id.clone(), (bytes, context));
        Ok(id)
    }
    async fn read(&self, blob: &str, context: String) -> Result<Vec<u8>> {
        let blobs = self.blobs.lock().await;
        let (bytes, expected) = blobs
            .get(blob)
            .ok_or_else(|| anyhow::anyhow!("Missing stored blob"))?;
        assert_eq!(
            &context, expected,
            "Storage context must include the exact base revision"
        );
        Ok(bytes.clone())
    }

    async fn create_media(&self, context: String) -> Result<Box<dyn MediaWriter>> {
        self.media.create_media(context).await
    }

    async fn read_media(
        &self,
        blob: &str,
        context: String,
        expected: Media,
    ) -> Result<ContentStream> {
        self.media.read_media(blob, context, expected).await
    }
}

#[async_trait]
impl Documents for Repository {
    async fn prepare(
        &self,
        _actor: &str,
        _space: &str,
        input: &SaveDocument,
        fingerprint: Vec<u8>,
    ) -> Result<Preparation> {
        let state = self.0.clone().lock_owned().await;
        if let Some((previous, revision)) = state.requests.get(&input.request_id) {
            return if *previous == fingerprint && *revision == state.document.revision {
                Ok(Preparation::AlreadyCommitted(state.document.clone()))
            } else {
                Err(Error::Conflict)
            };
        }
        if input.base_revision != state.document.revision {
            return Err(Error::Conflict);
        }
        Ok(Preparation::Write(Box::new(Write { state, fingerprint })))
    }
    async fn current(&self, actor: &str, space: &str, id: &str) -> Result<StoredVersion> {
        let state = self.0.lock().await;
        if actor != "user" || space != "space" {
            return Err(Error::Forbidden);
        }
        if id != state.document.id {
            return Err(Error::NotFound);
        }
        Ok(StoredVersion {
            document: state.document.clone(),
            blob: state.blob.clone(),
            media: vec![],
        })
    }
    async fn version_blob(
        &self,
        actor: &str,
        space: &str,
        id: &str,
        revision: i64,
    ) -> Result<Option<String>> {
        let state = self.0.lock().await;
        if actor != "user" || space != "space" {
            return Err(Error::Forbidden);
        }
        if id != state.document.id {
            return Err(Error::NotFound);
        }
        Ok(state.versions.get(&revision).cloned())
    }
    async fn metadata(&self, _actor: &str, _space: &str, _id: &str) -> Result<Document> {
        unreachable!()
    }
    async fn list(&self, _actor: &str, _space: &str) -> Result<Vec<Document>> {
        unreachable!()
    }
    async fn changes(&self, _actor: &str, _space: &str, _after: i64) -> Result<Changes> {
        unreachable!()
    }
    async fn media_lengths(
        &self,
        _actor: &str,
        _space: &str,
        _hashes: &[String],
    ) -> Result<BTreeMap<String, u64>> {
        unreachable!()
    }
    async fn media(&self, actor: &str, space: &str, hash: &str) -> Result<Option<(String, u64)>> {
        if actor != "user" || space != "space" {
            return Err(Error::Forbidden);
        }
        Ok(self.0.lock().await.media.get(hash).cloned())
    }
    async fn store_media(&self, actor: &str, space: &str, media: &Media, blob: &str) -> Result<()> {
        if actor != "user" || space != "space" {
            return Err(Error::Forbidden);
        }
        self.0
            .lock()
            .await
            .media
            .insert(media.hash.clone(), (blob.into(), media.length));
        Ok(())
    }
}

#[async_trait]
impl DocumentWrite for Write {
    fn revision(&self) -> i64 {
        self.state.document.revision + 1
    }
    async fn base_blob(&mut self) -> Result<String> {
        self.state.base_reads += 1;
        Ok(self.state.blob.clone())
    }
    async fn commit(mut self: Box<Self>, command: &SaveDocument, blob: &str) -> Result<Document> {
        let revision = self.revision();
        self.state.document.revision = revision;
        self.state.document.title = command.title.clone();
        self.state.blob = blob.into();
        self.state.versions.insert(revision, blob.into());
        let fingerprint = self.fingerprint.clone();
        self.state
            .requests
            .insert(command.request_id.clone(), (fingerprint, revision));
        Ok(self.state.document.clone())
    }
}

fn container(x: i32) -> Vec<u8> {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut writer = rovar_format::Writer::create(file.path()).unwrap();
    writer
        .put_bytes(
            "document",
            "json",
            &serde_json::to_vec(
                &serde_json::json!({"schema":3,"x":x,"unchanged":"padding".repeat(1000)}),
            )
            .unwrap(),
        )
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    std::fs::read(file.path()).unwrap()
}

fn command(id: &str, revision: i64, request: &str, content: &[u8]) -> SaveDocument {
    SaveDocument {
        id: id.into(),
        kind: DocumentKind::Document,
        title: "Design".into(),
        base_revision: revision,
        request_id: request.into(),
        content: content.into(),
        media: vec![],
        deleted: false,
    }
}

fn service() -> (DocumentService, Arc<Mutex<State>>, String) {
    let id = uuid::Uuid::new_v4().to_string();
    let state = Arc::new(Mutex::new(State {
        document: Document {
            id: id.clone(),
            kind: DocumentKind::Document,
            title: "Design".into(),
            revision: 0,
            created: 1,
            modified: 1,
            deleted: false,
        },
        blob: String::new(),
        requests: BTreeMap::new(),
        base_reads: 0,
        versions: BTreeMap::new(),
        media: BTreeMap::new(),
    }));
    let service = DocumentService::new(
        Arc::new(Repository(state.clone())),
        Arc::new(Storage::default()),
    );
    (service, state, id)
}

#[tokio::test]
async fn streamed_media_is_authorized_verified_and_deduplicated_before_publishing() {
    use futures_util::stream;
    use sha2::{Digest, Sha256};
    let (service, state, id) = service();
    let hash = format!("{:x}", Sha256::digest(b"abcdef"));
    let unread = || {
        stream::poll_fn(|_| -> std::task::Poll<Option<std::io::Result<Vec<u8>>>> {
            panic!("Unauthorized or already stored media must not read the upload body")
        })
    };
    assert!(matches!(
        service
            .upload_media("other", "space", &hash, unread())
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.upload_media("user", "other", &hash, unread()).await,
        Err(Error::Forbidden)
    ));
    for chunks in [
        vec![Ok(b"abc".to_vec()), Ok(b"xyz".to_vec())],
        vec![
            Ok(b"abc".to_vec()),
            Err(std::io::ErrorKind::UnexpectedEof.into()),
        ],
    ] {
        assert!(matches!(
            service
                .upload_media("user", "space", &hash, stream::iter(chunks))
                .await,
            Err(Error::Invalid(_))
        ));
        assert!(state.lock().await.media.is_empty());
    }
    service
        .upload_media(
            "user",
            "space",
            &hash,
            stream::iter([Ok(b"abc"), Ok(b"def")]),
        )
        .await
        .unwrap();
    let stored = state.lock().await.media.clone();
    use futures_util::StreamExt;
    let mut download = service
        .download_media("user", "space", &hash)
        .await
        .unwrap();
    let mut bytes = Vec::new();
    while let Some(chunk) = download.body.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(bytes, b"abcdef");
    let first = service
        .download_media("user", "space", &hash)
        .await
        .unwrap();
    let second = service
        .download_media("user", "space", &hash)
        .await
        .unwrap();
    let command = command(&id, 0, &uuid::Uuid::new_v4().to_string(), &container(0));
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        service.save("user", "space", command),
    )
    .await
    .expect("Media responses must not exhaust metadata transfer capacity")
    .unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(20),
            service.download_media("user", "space", &hash)
        )
        .await
        .is_err(),
        "Unconsumed responses must retain their transfer permits"
    );
    drop(first);
    let third = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        service.download_media("user", "space", &hash),
    )
    .await
    .unwrap()
    .unwrap();
    drop((second, third));
    let snapshot = crate::domain::document::DocumentSnapshot {
        document: state.lock().await.document.clone(),
        content: container(0),
        media: vec![Media {
            hash: hash.clone(),
            length: 6,
        }],
    };
    let expanded = service
        .expand_media("user", "space", snapshot)
        .await
        .unwrap();
    assert!(expanded.media.is_empty());
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), expanded.content).unwrap();
    let reader = rovar_format::Reader::open(file.path()).unwrap();
    reader.verify().unwrap();
    assert_eq!(reader.read(&format!("media/{hash}"), 6).unwrap(), b"abcdef");
    service
        .upload_media("user", "space", &hash, unread())
        .await
        .unwrap();
    assert_eq!(state.lock().await.media, stored);
}

#[tokio::test]
async fn delta_commit_replays_before_reading_the_base_and_rejections_do_not_publish() {
    let (service, state, id) = service();
    let base = container(0);
    let next = container(10);
    let before = rovar_format::delta::Snapshot::from_bytes(&base, MAX_METADATA_BYTES).unwrap();
    let after = rovar_format::delta::Snapshot::from_bytes(&next, MAX_METADATA_BYTES).unwrap();
    service
        .save(
            "user",
            "space",
            command(&id, 0, &uuid::Uuid::new_v4().to_string(), &base),
        )
        .await
        .unwrap();
    let patch = serde_json::to_vec(&before.difference(&after).unwrap()).unwrap();
    let request = uuid::Uuid::new_v4().to_string();
    let first = service
        .save_delta("user", "space", command(&id, 1, &request, &patch))
        .await
        .unwrap();
    assert_eq!(first.revision, 2);
    assert_eq!(
        service
            .save_delta("user", "space", command(&id, 1, &request, &patch))
            .await
            .unwrap()
            .revision,
        2
    );
    assert_eq!(
        state.lock().await.base_reads,
        1,
        "An acknowledged retry must not apply the patch twice"
    );
    let saved = service.transfer("user", "space", &id).await.unwrap();
    assert_eq!(
        rovar_format::delta::Snapshot::from_bytes(&saved.content, MAX_METADATA_BYTES)
            .unwrap()
            .hash()
            .unwrap(),
        after.hash().unwrap()
    );
    assert!(matches!(
        service
            .save_delta(
                "user",
                "space",
                command(&id, 1, &uuid::Uuid::new_v4().to_string(), &patch)
            )
            .await,
        Err(Error::Conflict)
    ));
    assert!(
        matches!(
            service
                .save("user", "space", command(&id, 1, &request, &patch))
                .await,
            Err(Error::Conflict)
        ),
        "Full and delta request identities must not collide"
    );
    let fallback_request = uuid::Uuid::new_v4().to_string();
    assert!(matches!(
        service
            .save_delta("user", "space", command(&id, 2, &fallback_request, &patch))
            .await,
        Err(Error::DeltaBase)
    ));
    assert_eq!(state.lock().await.document.revision, 2);
    assert!(!state.lock().await.requests.contains_key(&fallback_request));
    let mut tampered: serde_json::Value =
        serde_json::from_slice(&serde_json::to_vec(&after.difference(&before).unwrap()).unwrap())
            .unwrap();
    tampered["result"][0] = 999.into();
    assert!(matches!(
        service
            .save_delta(
                "user",
                "space",
                command(
                    &id,
                    2,
                    &uuid::Uuid::new_v4().to_string(),
                    &serde_json::to_vec(&tampered).unwrap()
                )
            )
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(state.lock().await.document.revision, 2);
    // The explicitly rejected base can be replaced with a complete snapshot.
    assert_eq!(
        service
            .save(
                "user",
                "space",
                command(&id, 2, &uuid::Uuid::new_v4().to_string(), &base)
            )
            .await
            .unwrap()
            .revision,
        3
    );
}

#[tokio::test]
async fn downloads_use_confirmed_history_and_fall_back_without_changing_versions() {
    let (service, state, id) = service();
    let base = container(0);
    let next = container(10);
    let before = rovar_format::delta::Snapshot::from_bytes(&base, MAX_METADATA_BYTES).unwrap();
    let after = rovar_format::delta::Snapshot::from_bytes(&next, MAX_METADATA_BYTES).unwrap();
    for (revision, bytes) in [(0, &base), (1, &next)] {
        service
            .save(
                "user",
                "space",
                command(&id, revision, &uuid::Uuid::new_v4().to_string(), bytes),
            )
            .await
            .unwrap();
    }
    let response = service
        .download("user", "space", &id, 1, before.hash().unwrap())
        .await
        .unwrap();
    assert!(response.delta);
    assert_eq!(response.snapshot.document.revision, 2);
    assert!(response.snapshot.content.len() * 4 < next.len());
    let patch = serde_json::from_slice(&response.snapshot.content).unwrap();
    assert_eq!(
        before
            .apply(&patch, MAX_METADATA_BYTES)
            .unwrap()
            .hash()
            .unwrap(),
        after.hash().unwrap()
    );
    let same = service
        .download("user", "space", &id, 2, after.hash().unwrap())
        .await
        .unwrap();
    assert!(
        same.delta,
        "A missing local cache can be rebuilt from an unchanged confirmed base"
    );
    for (revision, hash) in [(1, [0; 32]), (3, after.hash().unwrap())] {
        let full = service
            .download("user", "space", &id, revision, hash)
            .await
            .unwrap();
        assert!(!full.delta);
        assert_eq!(full.snapshot.content, next);
    }
    let old = state.lock().await.versions.remove(&1).unwrap();
    assert!(
        !service
            .download("user", "space", &id, 1, before.hash().unwrap())
            .await
            .unwrap()
            .delta
    );
    state
        .lock()
        .await
        .versions
        .insert(1, "unavailable history blob".into());
    assert!(
        !service
            .download("user", "space", &id, 1, before.hash().unwrap())
            .await
            .unwrap()
            .delta
    );
    state.lock().await.versions.insert(1, old);
    assert!(matches!(
        service
            .download("another-user", "space", &id, 1, before.hash().unwrap())
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service
            .download("user", "another-space", &id, 1, before.hash().unwrap())
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service
            .download("user", "space", "another-object", 1, before.hash().unwrap())
            .await,
        Err(Error::NotFound)
    ));
    assert_eq!(state.lock().await.document.revision, 2);
    assert_eq!(state.lock().await.requests.len(), 2);
}

#[tokio::test]
async fn legacy_inline_media_remains_in_full_downloads() {
    let (service, _, id) = service();
    let base = container(0);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), container(20)).unwrap();
    let mut writer = rovar_format::Writer::open(file.path()).unwrap();
    writer
        .put_bytes("media/legacy", "media", b"original inline asset")
        .unwrap();
    writer.commit().unwrap();
    drop(writer);
    let next = std::fs::read(file.path()).unwrap();
    for (revision, bytes) in [(0, &base), (1, &next)] {
        service
            .save(
                "user",
                "space",
                command(&id, revision, &uuid::Uuid::new_v4().to_string(), bytes),
            )
            .await
            .unwrap();
    }
    let hash = rovar_format::delta::Snapshot::from_bytes(&base, MAX_METADATA_BYTES)
        .unwrap()
        .hash()
        .unwrap();
    let response = service
        .download("user", "space", &id, 1, hash)
        .await
        .unwrap();
    assert!(!response.delta);
    assert_eq!(response.snapshot.content, next);
}
