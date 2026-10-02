use super::*;

/// Server-confirmed semantic content, independent of container compaction and previews.
#[derive(Serialize, Deserialize)]
pub(super) struct Baseline {
    pub object: Object,
    pub content: String,
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
        Ok(value) => serde_json::to_vec(&value)?,
        Err(_) => bytes,
    })
}

fn normalize(value: &mut serde_json::Value) {
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
        }
        serde_json::Value::Array(values) => {
            for value in values {
                normalize(value);
            }
        }
        _ => {}
    }
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
    pub(super) fn store_baseline(&self, object: &Object, content: &[u8]) -> Result<String> {
        let bytes = serde_json::to_vec(&Baseline {
            object: object.clone(),
            content: STANDARD.encode(content),
        })?;
        let key = hex::encode(Sha256::digest(&bytes));
        write_atomic(&self.root.join("baselines").join(&key), &bytes)?;
        Ok(key)
    }

    pub(super) fn read_baseline(&self, link: &Link) -> Result<Option<Baseline>> {
        let Some(key) = &link.baseline else {
            return Ok(None);
        };
        ensure!(
            key.len() == 64 && key.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid baseline key"
        );
        let bytes = rovar_storage::fs::read(self.root.join("baselines").join(key))?;
        ensure!(
            hex::encode(Sha256::digest(&bytes)) == *key,
            "Damaged sync baseline"
        );
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    /// Upgrade only caches whose bytes match a previously acknowledged upload/download.
    /// A missing or unverifiable baseline must never make local edits disappear.
    pub(super) fn reconcile_baseline(&mut self, path: &Path) -> Result<()> {
        let Some(link) = self.catalog.links.get(path).cloned() else {
            return Ok(());
        };
        if link.object.revision == 0 {
            return Ok(());
        }
        if !link.object.deleted && !rovar_storage::exists(path) {
            return Ok(());
        }
        // If verification fails, keep the cache protected rather than assuming clean.
        self.catalog.links.get_mut(path).unwrap().dirty = true;
        let current = content(path, link.object.deleted)?;
        let baseline = if let Some(baseline) = self.read_baseline(&link)? {
            baseline
        } else {
            let bytes = if link.object.deleted {
                Vec::new()
            } else {
                rovar_storage::fs::read(path)?
            };
            if digest(&bytes, &link.object.title, link.object.deleted) != link.digest {
                return Ok(());
            }
            let key = self.store_baseline(&link.object, &current)?;
            self.catalog.links.get_mut(path).unwrap().baseline = Some(key);
            Baseline {
                object: link.object.clone(),
                content: STANDARD.encode(&current),
            }
        };
        let dirty = current != STANDARD.decode(&baseline.content)?
            || link.object.title != baseline.object.title
            || link.object.deleted != baseline.object.deleted;
        let pending_path = self.pending_path(&link);
        if !dirty && link.conflict && rovar_storage::exists(&pending_path) {
            rovar_storage::fs::remove_file(&pending_path)?;
        }
        let pending = rovar_storage::exists(pending_path);
        let link = self.catalog.links.get_mut(path).unwrap();
        link.dirty = dirty || pending;
        if !dirty && !pending {
            // A rejected upload may have been undone back to its confirmed base.
            // Pending requests with unknown outcomes must first be acknowledged.
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
