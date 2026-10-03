use super::*;

impl Remote {
    /// Resolve against the snapshot the user reviewed, never an unseen revision.
    /// Local changes remain dirty and go through the normal guarded upload.
    pub(crate) fn resolve_conflict(
        &mut self,
        path: &Path,
        reviewed: &Object,
        server: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Result<gpui::Task<Result<()>>> {
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
        let connection = self
            .connection(&previous.connection)
            .filter(|c| c.authenticated)
            .ok_or_else(|| anyhow::anyhow!("Server account changed; reopen the comparison"))?;
        let generation = connection.generation;
        let local_generation = self.local_changes.get(path).copied().unwrap_or_default();
        let root = self.root.clone();
        let path = path.to_owned();
        let task = {
            let path = path.clone();
            let previous = previous.clone();
            let reviewed = reviewed.clone();
            cx.background_executor().spawn(async move {
                cache::prepare_resolution(&root, path, previous, reviewed, server)
            })
        };
        self.busy = true;
        // The worker outlives a dropped completion task so closing a window
        // cannot strand the shared busy flag.
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.spawn(async move |this, cx| {
            let prepared = task.await;
            let result = this
                .update(cx, |this, cx| {
                    this.busy = false;
                    let result = (|| {
                        ensure!(
                            this.connection(&previous.connection)
                                .is_some_and(|c| c.authenticated && c.generation == generation),
                            "Server account changed; reopen the comparison"
                        );
                        ensure!(
                            this.local_changes.get(&path).copied().unwrap_or_default()
                                == local_generation
                                && this.catalog.links.get(&path).is_some_and(|current| {
                                    current.connection == previous.connection
                                        && current.object.id == previous.object.id
                                        && current.object.revision == previous.object.revision
                                        && current.object.title == previous.object.title
                                        && current.object.deleted == previous.object.deleted
                                        && current.baseline == previous.baseline
                                        && current.conflict
                                }),
                            "Local document changed while resolving; try again"
                        );
                        this.install_resolution(prepared?)
                    })();
                    cx.notify();
                    result
                })
                .and_then(|result| result);
            let _ = sender.send(result);
        })
        .detach();
        cx.notify();
        Ok(cx.spawn(async move |_, _| receiver.await?))
    }
}
