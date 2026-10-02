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
        self.recover_incoming()?;
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
        // Retire only this rejected request, after the chosen revision has
        // committed. A later request must survive replaying an old journal.
        let rejected_request = match rovar_storage::fs::read(self.pending_path(&previous)) {
            Ok(bytes) => Some(
                serde_json::from_slice::<PendingSave>(&bytes)?
                    .input
                    .request_id,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
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
        self.install_resolution(path, previous, resolved, server_bytes, rejected_request)?;
        cx.notify();
        Ok(())
    }
}
