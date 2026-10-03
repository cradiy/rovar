use super::*;

mod recovery;

#[derive(Serialize, Deserialize)]
struct Incoming {
    path: PathBuf,
    previous: Option<Link>,
    next: Link,
    #[serde(default)]
    installed_content: Option<String>,
    #[serde(default)]
    rejected_request: Option<String>,
}

enum Installation {
    Prepared(rovar_storage::tempfile::NamedTempFile),
    Delete,
    Preserve,
}

pub(super) struct PreparedResolution {
    incoming: Incoming,
    file: Option<rovar_storage::tempfile::NamedTempFile>,
    local_stamp: Option<baseline::Stamp>,
    pending_stamp: Option<baseline::Stamp>,
}

fn optional_stamp(path: &Path) -> Result<Option<baseline::Stamp>> {
    match baseline::stamp(path, false) {
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        result => result,
    }
}

pub(super) fn prepare_resolution(
    root: &Path,
    path: PathBuf,
    previous: Link,
    reviewed: Object,
    server: Option<PathBuf>,
) -> Result<PreparedResolution> {
    let local_stamp = optional_stamp(&path)?;
    let pending = root
        .join("pending")
        .join(&previous.connection)
        .join(format!("{}.json", previous.object.id));
    let rejected = if optional_stamp(&pending)?.is_some() {
        Some(prepare_rejected_request(&pending)?)
    } else {
        None
    };
    let mut next = previous.clone();
    next.object.revision = reviewed.revision;
    next.object.modified = reviewed.modified;
    next.conflict = false;
    next.error = None;
    next.dirty = true;
    next.baseline = None;
    next.digest.clear();
    let (content, file) = if let Some(server) = server {
        let bytes = rovar_storage::fs::read(server)?;
        let content = baseline::snapshot_content(&bytes, false)?;
        let transfer =
            rovar_format::delta::Snapshot::from_bytes(&bytes, rovar_api::MAX_METADATA_BYTES).ok();
        next.baseline = Some(baseline::store(root, &reviewed, &content, transfer)?);
        next.digest = digest(&bytes, &reviewed.title, false);
        next.object = reviewed;
        next.dirty = false;
        (content, Some(stage(&path, &bytes)?))
    } else {
        (baseline::content(&path, false)?, None)
    };
    ensure!(
        optional_stamp(&path)? == local_stamp,
        "Local document changed while resolving; try again"
    );
    let (rejected_request, pending_stamp) = match rejected {
        Some(request) => (Some(request.id), request.stamp),
        None => (None, None),
    };
    Ok(PreparedResolution {
        incoming: Incoming {
            path,
            previous: Some(previous),
            next,
            installed_content: Some(hex::encode(Sha256::digest(content))),
            rejected_request,
        },
        file,
        local_stamp,
        pending_stamp,
    })
}

pub(super) struct PreparedSnapshot {
    pub object: Object,
    baseline: String,
    digest: String,
    file: Option<rovar_storage::tempfile::NamedTempFile>,
}

/// Prepare immutable files without changing the editable cache or publishing a
/// recovery journal. The shared worker protects the new baseline until commit.
pub(super) fn prepare_snapshot(
    root: &Path,
    path: &Path,
    object: Object,
    bytes: Option<Vec<u8>>,
) -> Result<PreparedSnapshot> {
    ensure!(
        object.deleted == bytes.is_none(),
        "Invalid downloaded snapshot"
    );
    let content = baseline::snapshot_content(bytes.as_deref().unwrap_or_default(), object.deleted)?;
    let transfer = bytes
        .as_ref()
        .filter(|_| object.kind != Kind::ColorStyle)
        .and_then(|bytes| {
            rovar_format::delta::Snapshot::from_bytes(bytes, rovar_api::MAX_METADATA_BYTES).ok()
        });
    let baseline = baseline::store(root, &object, &content, transfer)?;
    let digest = digest(
        bytes.as_deref().unwrap_or_default(),
        &object.title,
        object.deleted,
    );
    let file = if let Some(bytes) = bytes {
        Some(stage(path, &bytes)?)
    } else {
        None
    };
    Ok(PreparedSnapshot {
        object,
        baseline,
        digest,
        file,
    })
}

fn stage(path: &Path, bytes: &[u8]) -> Result<rovar_storage::tempfile::NamedTempFile> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing cache directory"))?;
    rovar_storage::fs::create_dir_all(parent)?;
    let mut file = rovar_storage::tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    Ok(file)
}

pub(super) struct PreparedMerge {
    incoming: Incoming,
    file: rovar_storage::tempfile::NamedTempFile,
    pending_stamp: Option<baseline::Stamp>,
}

pub(super) struct RejectedRequest {
    id: String,
    stamp: Option<baseline::Stamp>,
}

pub(super) fn prepare_rejected_request(path: &Path) -> Result<RejectedRequest> {
    let stamp = baseline::stamp(path, false)?;
    let saved: PendingSave = serde_json::from_slice(&rovar_storage::fs::read(path)?)?;
    ensure!(
        baseline::stamp(path, false)? == stamp,
        "Pending request changed during merge"
    );
    Ok(RejectedRequest {
        id: saved.input.request_id,
        stamp,
    })
}

#[derive(Clone, Copy)]
enum RequestCheck {
    Skip,
    Verified,
}

pub(super) struct MergeInput {
    pub object: Object,
    pub title: String,
    pub server: Vec<u8>,
    pub merged: Vec<u8>,
    pub rejected_request: RejectedRequest,
}

pub(super) fn prepare_merge(
    root: &Path,
    path: PathBuf,
    previous: Link,
    input: MergeInput,
) -> Result<PreparedMerge> {
    let MergeInput {
        object,
        title,
        server,
        merged,
        rejected_request,
    } = input;
    let confirmed = baseline::snapshot_content(&server, false)?;
    let content = baseline::snapshot_content(&merged, false)?;
    let transfer =
        rovar_format::delta::Snapshot::from_bytes(&server, rovar_api::MAX_METADATA_BYTES).ok();
    let mut next = previous.clone();
    next.baseline = Some(baseline::store(root, &object, &confirmed, transfer)?);
    next.digest = digest(&server, &object.title, false);
    next.dirty = content != confirmed || title != object.title;
    next.object = object;
    next.object.title = title;
    next.conflict = false;
    next.error = None;
    let file = stage(&path, &merged)?;
    Ok(PreparedMerge {
        incoming: Incoming {
            path,
            previous: Some(previous),
            next,
            installed_content: Some(hex::encode(Sha256::digest(content))),
            rejected_request: Some(rejected_request.id),
        },
        file,
        pending_stamp: rejected_request.stamp,
    })
}

#[cfg(all(test, not(target_family = "wasm")))]
mod resolution_tests;

pub(super) fn protect_baselines(root: &Path, protected: &mut BTreeSet<String>) -> Result<()> {
    let bytes = match rovar_storage::fs::read(root.join("incoming.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let incoming: Incoming = serde_json::from_slice(&bytes)?;
    protected.extend(incoming.previous.and_then(|link| link.baseline));
    protected.extend(incoming.next.baseline);
    Ok(())
}

impl Remote {
    /// Write the baseline and recovery record before replacing the cache. A
    /// restart can then distinguish downloaded content from a new local edit.
    pub(super) fn install_snapshot(
        &mut self,
        path: PathBuf,
        connection: String,
        prepared: PreparedSnapshot,
    ) -> Result<()> {
        self.ensure_recovered()?;
        let PreparedSnapshot {
            object,
            baseline,
            digest,
            file,
        } = prepared;
        let incoming = Incoming {
            previous: self.catalog.links.get(&path).cloned(),
            path,
            next: Link {
                connection,
                digest,
                object,
                baseline: Some(baseline),
                dirty: false,
                conflict: false,
                error: None,
            },
            installed_content: None,
            rejected_request: None,
        };
        self.install_incoming(
            incoming,
            file.map_or(Installation::Delete, Installation::Prepared),
            RequestCheck::Skip,
        )
    }

    pub(super) fn install_merge(&mut self, prepared: PreparedMerge) -> Result<()> {
        self.ensure_recovered()?;
        ensure!(
            baseline::stamp(&self.pending_path(&prepared.incoming.next), false)?
                == prepared.pending_stamp,
            "Pending request changed during merge"
        );
        self.install_incoming(
            prepared.incoming,
            Installation::Prepared(prepared.file),
            RequestCheck::Verified,
        )
    }

    pub(super) fn install_resolution(&mut self, prepared: PreparedResolution) -> Result<()> {
        self.ensure_recovered()?;
        ensure!(
            optional_stamp(&prepared.incoming.path)? == prepared.local_stamp,
            "Local document changed while resolving; try again"
        );
        ensure!(
            optional_stamp(&self.pending_path(&prepared.incoming.next))? == prepared.pending_stamp,
            "Pending request changed while resolving; try again"
        );
        self.install_incoming(
            prepared.incoming,
            prepared
                .file
                .map_or(Installation::Preserve, Installation::Prepared),
            RequestCheck::Verified,
        )
    }

    fn install_incoming(
        &mut self,
        incoming: Incoming,
        installation: Installation,
        request_check: RequestCheck,
    ) -> Result<()> {
        let journal = self.root.join("incoming.json");
        write_atomic(&journal, &serde_json::to_vec(&incoming)?)?;
        match installation {
            Installation::Prepared(file) => {
                file.persist(&incoming.path)?;
                rovar_format::sync_parent(&incoming.path)?;
            }
            Installation::Delete if rovar_storage::exists(&incoming.path) => {
                rovar_storage::fs::remove_file(&incoming.path)?;
            }
            Installation::Delete | Installation::Preserve => {}
        }
        self.finish_incoming(&incoming, request_check)?;
        rovar_storage::fs::remove_file(journal)?;
        Ok(())
    }

    fn finish_incoming(&mut self, incoming: &Incoming, request_check: RequestCheck) -> Result<()> {
        if incoming.next.object.kind == Kind::ColorStyle {
            let bytes = if incoming.next.object.deleted {
                None
            } else {
                Some(rovar_storage::fs::read(&incoming.path)?)
            };
            self.apply_color(
                &incoming.next.connection,
                &incoming.next.object,
                bytes.as_deref(),
            )?;
        }
        self.catalog
            .links
            .insert(incoming.path.clone(), incoming.next.clone());
        if !self.persist() {
            if let Some(previous) = &incoming.previous {
                self.catalog
                    .links
                    .insert(incoming.path.clone(), previous.clone());
            } else {
                self.catalog.links.remove(&incoming.path);
            }
            anyhow::bail!("Could not persist downloaded revision");
        }
        self.retire_rejected_request(incoming, request_check)?;
        if matches!(
            incoming.next.object.kind,
            Kind::Component | Kind::ColorStyle
        ) {
            self.libraries_changed
                .insert(incoming.next.connection.clone());
        }
        Ok(())
    }

    fn retire_rejected_request(
        &self,
        incoming: &Incoming,
        request_check: RequestCheck,
    ) -> Result<()> {
        if incoming.rejected_request.is_some() {
            let pending = self.pending_path(&incoming.next);
            if rovar_storage::exists(&pending) {
                let matches = match request_check {
                    RequestCheck::Verified => true,
                    RequestCheck::Skip => false,
                };
                if matches {
                    rovar_storage::fs::remove_file(pending)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn ensure_recovered(&self) -> Result<()> {
        ensure!(
            !rovar_storage::exists(self.root.join("incoming.json")),
            "Sync recovery is pending; try again"
        );
        Ok(())
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    #[gpui::test]
    fn prepared_downloads_do_not_replace_local_edits_or_bypass_the_journal(
        cx: &mut gpui::TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.rovar");
        write_atomic(&path, b"original").unwrap();
        let remote = cx.update(|cx| Remote::shared(root.path(), cx));
        remote.update(cx, |r, cx| {
            let connection = r
                .connect(
                    "https://example.test".into(),
                    crate::remote::tests::identity(),
                    "token".into(),
                    cx,
                )
                .unwrap();
            r.track(
                path.clone(),
                connection.clone(),
                "Design".into(),
                Kind::Document,
                cx,
            );
            let mut object = r.link(&path).unwrap().object.clone();
            object.revision = 1;
            let prepared =
                prepare_snapshot(&r.root, &path, object.clone(), Some(b"downloaded".to_vec()))
                    .unwrap();
            let staged = prepared.file.as_ref().unwrap().path().to_owned();
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"original");
            assert!(!rovar_storage::exists(root.path().join("incoming.json")));
            write_atomic(&path, b"later local edit").unwrap();
            // A stale or cancelled download can be dropped without publishing it.
            drop(prepared);
            assert!(!rovar_storage::exists(staged));
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"later local edit");

            let prepared =
                prepare_snapshot(&r.root, &path, object.clone(), Some(b"downloaded".to_vec()))
                    .unwrap();
            // Publication must fail before replacing the file if its recovery
            // record cannot be committed, and the temporary file must be retired.
            let staged = prepared.file.as_ref().unwrap().path().to_owned();
            let journal = root.path().join("incoming.json");
            rovar_storage::fs::create_dir_all(&journal).unwrap();
            assert!(
                r.install_snapshot(path.clone(), connection.clone(), prepared)
                    .is_err()
            );
            assert!(!rovar_storage::exists(staged));
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"later local edit");
            assert_eq!(r.link(&path).unwrap().object.revision, 0);
            rovar_storage::fs::remove_dir(&journal).unwrap();

            let prepared =
                prepare_snapshot(&r.root, &path, object, Some(b"downloaded".to_vec())).unwrap();
            r.install_snapshot(path.clone(), connection, prepared)
                .unwrap();
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"downloaded");
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            assert!(!r.link(&path).unwrap().dirty);
            assert!(!rovar_storage::exists(journal));
        });
    }
    use gpui::TestAppContext;

    #[gpui::test]
    fn merged_cache_recovery_advances_only_the_server_baseline_and_retires_the_rejected_request(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("design.rovar");
        write_atomic(&path, b"base").unwrap();
        let remote = cx.update(|cx| Remote::shared(root.path(), cx));
        remote.update(cx, |r, cx| {
            let connection = r
                .connect(
                    "https://example.test".into(),
                    super::super::tests::identity(),
                    "token".into(),
                    cx,
                )
                .unwrap();
            r.track(
                path.clone(),
                connection.clone(),
                "Design".into(),
                Kind::Document,
                cx,
            );
            let mut object = r.link(&path).unwrap().object.clone();
            object.revision = 1;
            let prepared =
                prepare_snapshot(&r.root, &path, object.clone(), Some(b"base".to_vec())).unwrap();
            r.install_snapshot(path.clone(), connection, prepared)
                .unwrap();
            write_atomic(&path, b"local").unwrap();
            let previous = r.catalog.links.get_mut(&path).unwrap();
            previous.dirty = true;
            previous.conflict = true;
            let previous = previous.clone();
            let request = PendingSave {
                delta: None,
                input: Save {
                    kind: Kind::Document,
                    title: "Design".into(),
                    base_revision: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    content: STANDARD.encode(b"local"),
                    media: vec![],
                    deleted: false,
                },
            };
            let pending = r.pending_path(&previous);
            let request_bytes = serde_json::to_vec(&request).unwrap();
            write_atomic(&pending, &request_bytes).unwrap();
            object.revision = 2;
            let mut next = previous.clone();
            next.object = object.clone();
            next.baseline = Some(r.store_baseline(&object, b"server").unwrap());
            next.conflict = false;
            let incoming = Incoming {
                path: path.clone(),
                previous: Some(previous),
                next: next.clone(),
                installed_content: Some(hex::encode(Sha256::digest(b"merged"))),
                rejected_request: Some(request.input.request_id),
            };
            let journal = root.path().join("incoming.json");
            let record = serde_json::to_vec(&incoming).unwrap();
            // Crash before the new cache is installed must leave the old request.
            write_atomic(&journal, &record).unwrap();
            recovery::recover_for_test(r).unwrap();
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            assert!(rovar_storage::exists(&pending));
            // Crash after writing the merged cache, before committing its revision.
            write_atomic(&journal, &record).unwrap();
            write_atomic(&path, b"merged").unwrap();
            recovery::recover_for_test(r).unwrap();
            assert_eq!(r.link(&path).unwrap().object.revision, 2);
            assert!(r.link(&path).unwrap().dirty);
            assert!(!r.link(&path).unwrap().conflict);
            assert_eq!(
                r.read_baseline(&next).unwrap().unwrap().content,
                STANDARD.encode(b"server")
            );
            assert!(!rovar_storage::exists(&pending));
            // Crash after catalog commit, then a new edit before recovery.
            write_atomic(&journal, &record).unwrap();
            write_atomic(&pending, &request_bytes).unwrap();
            write_atomic(&path, b"later edit").unwrap();
            r.catalog.links.get_mut(&path).unwrap().object.title = "Later title".into();
            recovery::recover_for_test(r).unwrap();
            assert!(!rovar_storage::exists(&pending));
            assert_eq!(r.link(&path).unwrap().object.title, "Later title");
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"later edit");
        });
    }

    #[gpui::test]
    fn interrupted_cache_install_recovers_the_revision_without_overwriting_new_edits(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.rovar");
        write_atomic(&path, b"version one").unwrap();
        let remote = cx.update(|cx| Remote::shared(root.path(), cx));
        remote.update(cx, |r, cx| {
            let connection = r
                .connect(
                    "https://example.test".into(),
                    super::super::tests::identity(),
                    "token".into(),
                    cx,
                )
                .unwrap();
            r.track(
                path.clone(),
                connection.clone(),
                "Design".into(),
                Kind::Document,
                cx,
            );
            let mut object = r.link(&path).unwrap().object.clone();
            object.revision = 1;
            let prepared = prepare_snapshot(
                &r.root,
                &path,
                object.clone(),
                Some(b"version one".to_vec()),
            )
            .unwrap();
            r.install_snapshot(path.clone(), connection, prepared)
                .unwrap();
            let previous = r.link(&path).unwrap().clone();
            object.revision = 2;
            let mut next = previous.clone();
            next.baseline = Some(r.store_baseline(&object, b"version two").unwrap());
            next.digest = digest(b"version two", "Design", false);
            next.object = object;
            let incoming = Incoming {
                path: path.clone(),
                previous: Some(previous.clone()),
                next,
                installed_content: None,
                rejected_request: None,
            };
            let journal = root.path().join("incoming.json");
            let record = serde_json::to_vec(&incoming).unwrap();
            // Interrupted before replacing the file: keep the old version.
            write_atomic(&journal, &record).unwrap();
            recovery::recover_for_test(r).unwrap();
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            // Interrupted after the file write but before the catalog commit.
            write_atomic(&journal, &record).unwrap();
            write_atomic(&path, b"version two").unwrap();
            recovery::recover_for_test(r).unwrap();
            assert_eq!(r.link(&path).unwrap().object.revision, 2);
            assert!(!r.link(&path).unwrap().dirty);
            let persisted: Catalog = serde_json::from_slice(
                &rovar_storage::fs::read(root.path().join("servers.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(persisted.links[&path].object.revision, 2);
            // A later local save must not be relabelled as the downloaded content.
            r.catalog.links.insert(path.clone(), previous);
            write_atomic(&journal, &record).unwrap();
            write_atomic(&path, b"new local edits").unwrap();
            recovery::recover_for_test(r).unwrap();
            r.reconcile_baselines(vec![path.clone()], cx, |_, _| {});
        });
        crate::remote::tests::wait_sync(&remote, cx);
        remote.update(cx, |r, _| {
            assert!(r.link(&path).unwrap().dirty);
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"new local edits");
        });
    }
}
