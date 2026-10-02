use super::*;

impl Remote {
    /// Resolve against the snapshot the user reviewed, never an unseen revision.
    /// Local changes remain dirty and go through the normal guarded upload.
    pub(crate) fn resolve_conflict(
        &mut self,
        path: &Path,
        reviewed: &Object,
        server_bytes: Option<&[u8]>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        ensure!(!self.busy, "Sync is busy; try again");
        let previous = self
            .catalog
            .links
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Document is no longer linked"))?;
        ensure!(previous.conflict, "This conflict has already been resolved");
        ensure!(
            previous.object.id == reviewed.id
                && previous.object.kind == reviewed.kind
                && reviewed.revision >= previous.object.revision
                && !reviewed.deleted,
            "The compared document has changed"
        );
        let pending = self
            .root
            .join("pending")
            .join(&previous.connection)
            .join(format!("{}.json", reviewed.id));
        // A rejected request must not be replayed after choosing a new base.
        match rovar_storage::fs::remove_file(&pending) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let backup = if let Some(bytes) = server_bytes {
            let backup = rovar_storage::fs::read(path)?;
            write_atomic(path, bytes)?;
            Some(backup)
        } else {
            None
        };
        let mut resolved = previous.clone();
        resolved.object.revision = reviewed.revision;
        resolved.object.modified = reviewed.modified;
        resolved.conflict = false;
        resolved.error = None;
        resolved.dirty = true;
        // Choosing the local version rebases it onto the reviewed revision.
        resolved.baseline = None;
        resolved.digest.clear();
        if let Some(bytes) = server_bytes {
            resolved.object = reviewed.clone();
            resolved.digest = digest(bytes, &reviewed.title, false);
            resolved.baseline = Some(self.store_snapshot_baseline(
                reviewed,
                &baseline::snapshot_content(bytes, false)?,
                bytes,
            )?);
            resolved.dirty = false;
        }
        self.catalog.links.insert(path.into(), resolved);
        if !self.persist() {
            self.catalog.links.insert(path.into(), previous);
            if let Some(bytes) = backup {
                write_atomic(path, &bytes)?;
            }
            anyhow::bail!("Could not save the conflict resolution");
        }
        cx.notify();
        Ok(())
    }
}
