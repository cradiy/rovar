use super::*;

pub(super) struct Prepared {
    pub pending: PendingSave,
    pub media: media::Upload,
}

/// Called on the background executor. A retry is restored before reading the
/// editable file, so changes made since the first attempt cannot alter it.
pub(super) fn prepare(root: &Path, path: &Path, link: &Link, record: &Path) -> Result<Prepared> {
    let pending = match rovar_storage::fs::read(record) {
        Ok(bytes) => serde_json::from_slice::<PendingSave>(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let bytes = if link.object.deleted {
                Vec::new()
            } else {
                rovar_storage::fs::read(path)?
            };
            ensure!(
                bytes.len() <= rovar_api::MAX_DOCUMENT_BYTES,
                "Document exceeds the server's 128 MiB limit"
            );
            let input = Save {
                kind: link.object.kind.clone(),
                title: link.object.title.clone(),
                base_revision: link.object.revision,
                request_id: uuid::Uuid::new_v4().to_string(),
                content: STANDARD.encode(bytes),
                media: Vec::new(),
                deleted: link.object.deleted,
            };
            let base = baseline::read(root, link).ok().flatten().and_then(|base| {
                (base.object.revision == link.object.revision
                    && base.object.id == link.object.id
                    && !base.object.deleted)
                    .then_some(base.transfer)
                    .flatten()
            });
            let pending = PendingSave {
                delta: delta::prepare(base.as_ref(), &input),
                input,
            };
            write_atomic(record, &serde_json::to_vec(&pending)?)?;
            pending
        }
        Err(error) => return Err(error.into()),
    };
    let media = media::prepare(&pending.input)?;
    Ok(Prepared { pending, media })
}

pub(super) struct Confirmed {
    pub object: Object,
    pub digest: String,
    pub baseline: String,
    pub content: Vec<u8>,
}

/// The immutable submitted snapshot supplies the baseline, never the current
/// editable file. The UI persists the acknowledgement before retiring retries.
pub(super) fn confirm(root: &Path, object: Object, pending: PendingSave) -> Result<Confirmed> {
    let input = pending.input;
    let bytes = STANDARD.decode(&input.content)?;
    let digest = digest(&bytes, &input.title, input.deleted);
    let content = baseline::snapshot_content(&bytes, input.deleted)?;
    let transfer = (!input.deleted && input.kind != Kind::ColorStyle)
        .then(|| {
            rovar_format::delta::Snapshot::from_bytes(&bytes, rovar_api::MAX_METADATA_BYTES).ok()
        })
        .flatten();
    let baseline = baseline::store(root, &object, &content, transfer)?;
    Ok(Confirmed {
        object,
        digest,
        baseline,
        content,
    })
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    fn link() -> Link {
        Link {
            connection: "account".into(),
            object: Object {
                id: uuid::Uuid::new_v4().to_string(),
                kind: Kind::Document,
                title: "Design".into(),
                revision: 0,
                created: 1,
                modified: 1,
                deleted: false,
            },
            dirty: true,
            digest: String::new(),
            baseline: None,
            conflict: false,
            error: None,
        }
    }

    #[test]
    fn preparation_and_confirmation_keep_the_submitted_snapshot_when_local_content_changes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.rovar");
        let record = root.path().join("pending.json");
        let mut link = link();
        write_atomic(&path, b"submitted").unwrap();
        let prepared = prepare(root.path(), &path, &link, &record).unwrap();
        let request = rovar_storage::fs::read(&record).unwrap();
        write_atomic(&path, b"later edit").unwrap();
        link.object.title = "Later title".into();
        let retry = prepare(root.path(), &path, &link, &record).unwrap();
        assert_eq!(
            retry.pending.input.request_id,
            prepared.pending.input.request_id
        );
        assert_eq!(
            STANDARD.decode(&retry.pending.input.content).unwrap(),
            b"submitted"
        );
        assert_eq!(retry.pending.input.title, "Design");
        let mut acknowledged = link.object.clone();
        acknowledged.title = "Design".into();
        acknowledged.revision = 1;
        let confirmed = confirm(root.path(), acknowledged, retry.pending).unwrap();
        assert_eq!(confirmed.content, b"submitted");
        assert_eq!(confirmed.digest, digest(b"submitted", "Design", false));
        link.baseline = Some(confirmed.baseline);
        assert_eq!(
            STANDARD
                .decode(&baseline::read(root.path(), &link).unwrap().unwrap().content)
                .unwrap(),
            b"submitted"
        );
        assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"later edit");
        assert_eq!(
            rovar_storage::fs::read(&record).unwrap(),
            request,
            "Only the catalog acknowledgement may retire the request"
        );
    }

    #[test]
    fn failed_confirmation_and_corrupt_retry_records_do_not_replace_pending_work() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.rovar");
        let record = root.path().join("pending.json");
        let link = link();
        write_atomic(&path, b"submitted").unwrap();
        let prepared = prepare(root.path(), &path, &link, &record).unwrap();
        let request = rovar_storage::fs::read(&record).unwrap();
        write_atomic(&root.path().join("baselines"), b"blocked").unwrap();
        assert!(confirm(root.path(), link.object.clone(), prepared.pending).is_err());
        assert_eq!(rovar_storage::fs::read(&record).unwrap(), request);
        write_atomic(&record, b"damaged retry").unwrap();
        assert!(prepare(root.path(), &path, &link, &record).is_err());
        assert_eq!(rovar_storage::fs::read(record).unwrap(), b"damaged retry");
        assert_eq!(rovar_storage::fs::read(path).unwrap(), b"submitted");
    }
}
