use super::*;
use crate::application::{
    documents::DocumentService,
    ports::{ContentStorage, DocumentWrite, Documents, Preparation},
};
use crate::domain::document::{
    Changes, Document, DocumentKind, Media, SaveDocument, StoredVersion,
};
use async_trait::async_trait;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{Mutex, OwnedMutexGuard};

struct State {
    document: Document,
    blob: String,
    requests: BTreeMap<String, (Vec<u8>, i64)>,
    base_reads: usize,
}
struct Repository(Arc<Mutex<State>>);
struct Write {
    state: OwnedMutexGuard<State>,
    fingerprint: Vec<u8>,
}
#[derive(Default)]
struct Storage(Mutex<BTreeMap<String, (Vec<u8>, String)>>);

#[async_trait]
impl ContentStorage for Storage {
    async fn write(&self, bytes: Vec<u8>, context: String) -> Result<String> {
        let mut blobs = self.0.lock().await;
        let id = blobs.len().to_string();
        blobs.insert(id.clone(), (bytes, context));
        Ok(id)
    }
    async fn read(&self, blob: &str, context: String) -> Result<Vec<u8>> {
        let blobs = self.0.lock().await;
        let (bytes, expected) = &blobs[blob];
        assert_eq!(
            &context, expected,
            "Storage context must include the exact base revision"
        );
        Ok(bytes.clone())
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
    async fn current(&self, _actor: &str, _space: &str, _id: &str) -> Result<StoredVersion> {
        let state = self.0.lock().await;
        Ok(StoredVersion {
            document: state.document.clone(),
            blob: state.blob.clone(),
            media: vec![],
        })
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
    async fn media(
        &self,
        _actor: &str,
        _space: &str,
        _hash: &str,
    ) -> Result<Option<(String, u64)>> {
        unreachable!()
    }
    async fn store_media(
        &self,
        _actor: &str,
        _space: &str,
        _media: &Media,
        _blob: &str,
    ) -> Result<()> {
        unreachable!()
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

#[tokio::test]
async fn delta_commit_replays_before_reading_the_base_and_rejections_do_not_publish() {
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
    }));
    let service = DocumentService::new(
        Arc::new(Repository(state.clone())),
        Arc::new(Storage::default()),
    );
    let base = container(0);
    let next = container(10);
    let before = rovar_format::delta::Snapshot::from_bytes(&base, MAX_CONTENT_BYTES).unwrap();
    let after = rovar_format::delta::Snapshot::from_bytes(&next, MAX_CONTENT_BYTES).unwrap();
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
        rovar_format::delta::Snapshot::from_bytes(&saved.content, MAX_CONTENT_BYTES)
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
