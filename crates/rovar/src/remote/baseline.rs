use super::*;

/// Server-confirmed semantic content, independent of container compaction and previews.
#[derive(Serialize, Deserialize)]
pub(super) struct Baseline {
    pub object: Object,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<rovar_format::delta::Snapshot>,
}

pub(super) fn read(root: &Path, link: &Link) -> Result<Option<Baseline>> {
    let Some(key) = &link.baseline else {
        return Ok(None);
    };
    ensure!(
        key.len() == 64 && key.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid baseline key"
    );
    let bytes = rovar_storage::fs::read(root.join("baselines").join(key))?;
    ensure!(
        hex::encode(Sha256::digest(&bytes)) == *key,
        "Damaged sync baseline"
    );
    let mut baseline: Baseline = serde_json::from_slice(&bytes)?;
    baseline.content = STANDARD.encode(baseline_content(&STANDARD.decode(&baseline.content)?)?);
    Ok(Some(baseline))
}

pub(super) fn content(path: &Path, deleted: bool) -> Result<Vec<u8>> {
    if deleted {
        return Ok(Vec::new());
    }
    if let Ok(loaded) = crate::document::load(path) {
        let mut document: serde_json::Value = serde_json::from_slice(&loaded.json)?;
        normalize(&mut document);
        return Ok(serde_json::to_vec(&document)?);
    }
    let bytes = rovar_storage::fs::read(path)?;
    // Color styles and legacy JSON documents have no container.
    Ok(match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(mut value) => {
            normalize(&mut value);
            serde_json::to_vec(&value)?
        }
        Err(_) => bytes,
    })
}

fn normalize(value: &mut serde_json::Value) {
    crate::document::identity::upgrade_json_node_ids(value);
    match value {
        serde_json::Value::Object(fields) => {
            // Allocation watermarks can advance after an undone insertion.
            // They are not an edit to the document's visible content.
            if fields.contains_key("boards")
                && fields.contains_key("shapes")
                && fields.contains_key("texts")
            {
                fields.remove("next_id");
            }
            for value in fields.values_mut() {
                normalize(value);
            }
            // serde_json can preserve insertion order through workspace feature
            // unification. Semantic equality must not depend on field order.
            fields.sort_keys();
        }
        serde_json::Value::Array(values) => {
            for value in values {
                normalize(value);
            }
        }
        _ => {}
    }
}

fn baseline_content(bytes: &[u8]) -> Result<Vec<u8>> {
    // A baseline saved before persistent node IDs was introduced must compare
    // against the same deterministic upgrade used when loading its local cache.
    if let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(bytes) {
        normalize(&mut value);
        return Ok(serde_json::to_vec(&value)?);
    }
    Ok(bytes.to_vec())
}

pub(super) fn snapshot_content(bytes: &[u8], deleted: bool) -> Result<Vec<u8>> {
    if deleted {
        return Ok(Vec::new());
    }
    use std::io::Write;
    let mut file = rovar_storage::tempfile::NamedTempFile::new()?;
    file.write_all(bytes)?;
    content(file.path(), false)
}

impl Remote {
    #[cfg(test)]
    pub(super) fn store_baseline(&self, object: &Object, content: &[u8]) -> Result<String> {
        self.store_transfer_baseline(object, content, None)
    }

    #[cfg(test)]
    pub(super) fn store_snapshot_baseline(
        &self,
        object: &Object,
        content: &[u8],
        bytes: &[u8],
    ) -> Result<String> {
        let transfer = (!object.deleted && object.kind != Kind::ColorStyle)
            .then(|| {
                rovar_format::delta::Snapshot::from_bytes(bytes, rovar_api::MAX_METADATA_BYTES).ok()
            })
            .flatten();
        self.store_transfer_baseline(object, content, transfer)
    }

    #[cfg(test)]
    pub(super) fn store_transfer_baseline(
        &self,
        object: &Object,
        content: &[u8],
        transfer: Option<rovar_format::delta::Snapshot>,
    ) -> Result<String> {
        store(&self.root, object, content, transfer)
    }

    #[cfg(test)]
    pub(super) fn read_baseline(&self, link: &Link) -> Result<Option<Baseline>> {
        read(&self.root, link)
    }

    /// Keep the shared worker leased until background baselines are published.
    /// Edits made while scanning invalidate the result, including an undo that
    /// restores the same title and other catalog fields.
    pub(super) fn reconcile_baselines(
        &mut self,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        debug_assert!(!self.busy);
        let work: Vec<_> = paths
            .into_iter()
            .filter_map(|path| {
                let link = self.catalog.links.get_mut(&path)?;
                if link.object.revision == 0
                    || (!link.object.deleted && !rovar_storage::exists(&path))
                {
                    return None;
                }
                link.dirty = true;
                let generation = self.local_changes.get(&path).copied().unwrap_or_default();
                Some((path, link.clone(), generation))
            })
            .collect();
        if work.is_empty() {
            done(self, cx);
            return;
        }
        self.busy = true;
        let root = self.root.clone();
        let task = cx.background_executor().spawn(async move {
            work.into_iter()
                .map(|(path, link, generation)| {
                    let result = compare(&root, &path, &link);
                    (path, link, generation, result)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let results = task.await;
            let _ = this.update(cx, |this, cx| {
                for (path, original, generation, result) in results {
                    if !this.comparison_is_current(&path, &original, generation) {
                        continue;
                    }
                    if let Err(error) =
                        result.and_then(|result| this.apply_comparison(&path, result))
                    {
                        this.error = Some(error.to_string());
                    }
                }
                this.busy = false;
                if this.persist() {
                    done(this, cx);
                }
                cx.notify();
                this.publish_completed(cx);
            });
        })
        .detach();
    }

    fn comparison_is_current(&self, path: &Path, original: &Link, generation: u64) -> bool {
        self.local_changes.get(path).copied().unwrap_or_default() == generation
            && self.catalog.links.get(path).is_some_and(|current| {
                current.connection == original.connection
                    && current.object.id == original.object.id
                    && current.object.revision == original.object.revision
                    && current.object.title == original.object.title
                    && current.object.deleted == original.object.deleted
                    && current.baseline == original.baseline
                    && current.conflict == original.conflict
            })
    }

    fn apply_comparison(&mut self, path: &Path, comparison: Comparison) -> Result<()> {
        let link = &self.catalog.links[path];
        // The file can already have been saved while its UI callback is still
        // queued. Check its stamp as well as the explicit change generation.
        if stamp(path, link.object.deleted)? != comparison.stamp {
            return Ok(());
        }
        let pending_path = self.pending_path(link);
        if !comparison.dirty && link.conflict && rovar_storage::exists(&pending_path) {
            rovar_storage::fs::remove_file(&pending_path)?;
        }
        let pending = rovar_storage::exists(pending_path);
        let link = self.catalog.links.get_mut(path).unwrap();
        link.baseline = comparison.baseline;
        link.dirty = comparison.dirty || pending;
        if !link.dirty {
            // Unknown outcomes must be acknowledged even after an undo.
            link.conflict = false;
            link.error = None;
        }
        Ok(())
    }

    pub(super) fn pending_path(&self, link: &Link) -> PathBuf {
        self.root
            .join("pending")
            .join(&link.connection)
            .join(format!("{}.json", link.object.id))
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct Stamp {
    len: u64,
    modified: web_time::SystemTime,
}

pub(super) fn stamp(path: &Path, deleted: bool) -> Result<Option<Stamp>> {
    if deleted {
        return Ok(None);
    }
    let metadata = rovar_storage::fs::metadata(path)?;
    Ok(Some(Stamp {
        len: metadata.len(),
        modified: metadata.modified()?,
    }))
}

struct Comparison {
    dirty: bool,
    baseline: Option<String>,
    stamp: Option<Stamp>,
}

fn compare(root: &Path, path: &Path, link: &Link) -> Result<Comparison> {
    let before = stamp(path, link.object.deleted)?;
    let current = content(path, link.object.deleted)?;
    let mut key = link.baseline.clone();
    let dirty = if let Some(baseline) = read(root, link)? {
        current != STANDARD.decode(&baseline.content)?
            || link.object.title != baseline.object.title
            || link.object.deleted != baseline.object.deleted
    } else {
        // Only upgrade caches whose bytes match an acknowledged transfer.
        let bytes = if link.object.deleted {
            Vec::new()
        } else {
            rovar_storage::fs::read(path)?
        };
        if digest(&bytes, &link.object.title, link.object.deleted) == link.digest {
            key = Some(store(root, &link.object, &current, None)?);
            false
        } else {
            true
        }
    };
    ensure!(
        before == stamp(path, link.object.deleted)?,
        "Document changed during sync scan"
    );
    Ok(Comparison {
        dirty,
        baseline: key,
        stamp: before,
    })
}

pub(super) fn store(
    root: &Path,
    object: &Object,
    content: &[u8],
    transfer: Option<rovar_format::delta::Snapshot>,
) -> Result<String> {
    let bytes = serde_json::to_vec(&Baseline {
        object: object.clone(),
        content: STANDARD.encode(content),
        transfer,
    })?;
    let key = hex::encode(Sha256::digest(&bytes));
    write_atomic(&root.join("baselines").join(&key), &bytes)?;
    Ok(key)
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    #[gpui::test]
    fn completed_scan_cannot_clear_a_save_whose_callback_is_still_queued(
        cx: &mut gpui::TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.rovar");
        write_atomic(&path, b"confirmed").unwrap();
        let remote = cx.update(|cx| Remote::shared(root.path(), cx));
        cx.update(|cx| {
            crate::remote::tests::confirmed_document(
                &remote,
                "https://example.test".into(),
                path.clone(),
                Object {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: Kind::Document,
                    title: "Design".into(),
                    revision: 1,
                    created: 1,
                    modified: 1,
                    deleted: false,
                },
                cx,
            );
        });
        remote.update(cx, |r, _| {
            r.catalog.links.get_mut(&path).unwrap().dirty = true;
            let result = compare(root.path(), &path, &r.catalog.links[&path]).unwrap();
            // Disk replacement can precede Remote::changed on the UI executor.
            write_atomic(&path, b"new local content").unwrap();
            r.apply_comparison(&path, result).unwrap();
            assert!(r.link(&path).unwrap().dirty);
            let current = compare(root.path(), &path, &r.catalog.links[&path]).unwrap();
            r.apply_comparison(&path, current).unwrap();
            assert!(r.link(&path).unwrap().dirty);
            write_atomic(&path, b"confirmed").unwrap();
            let undone = compare(root.path(), &path, &r.catalog.links[&path]).unwrap();
            r.apply_comparison(&path, undone).unwrap();
            assert!(!r.link(&path).unwrap().dirty);
        });
    }
}
