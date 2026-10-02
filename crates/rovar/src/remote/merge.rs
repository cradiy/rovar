use super::*;

impl Remote {
    /// Called after a confirmed CAS rejection, outside a Studio entity update.
    /// Active unsaved edits defer the merge until autosave has completed.
    pub(super) fn try_merge(
        &mut self,
        protected: &BTreeSet<PathBuf>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.busy {
            return false;
        }
        let pending: Vec<_> = self.merge_pending.iter().cloned().collect();
        for path in pending {
            if protected.contains(&path) {
                continue;
            }
            let Some(link) = self.catalog.links.get(&path).cloned().filter(|link| {
                link.conflict && !link.object.deleted && link.object.kind == Kind::Document
            }) else {
                self.merge_pending.remove(&path);
                continue;
            };
            let Some(connection) = self
                .connection(&link.connection)
                .filter(|c| c.authenticated)
            else {
                continue;
            };
            let client = connection.client();
            let identity = connection.identity.clone();
            let space = connection.space.id.clone();
            let generation = connection.generation;
            let baseline = self.read_baseline(&link).ok().flatten();
            let Some(baseline) = baseline.filter(|b| !b.object.deleted) else {
                self.merge_pending.remove(&path);
                continue;
            };
            let Ok(base) = STANDARD.decode(&baseline.content) else {
                self.merge_pending.remove(&path);
                continue;
            };
            // Opaque/older unsupported content stays in the comparison flow.
            if crate::document::load(&path).is_err() {
                self.merge_pending.remove(&path);
                continue;
            }
            let pending = self.pending_path(&link);
            let Ok(request) = rovar_storage::fs::read(&pending).and_then(|bytes| {
                serde_json::from_slice::<PendingSave>(&bytes).map_err(std::io::Error::other)
            }) else {
                self.merge_pending.remove(&path);
                continue;
            };
            self.merge_pending.remove(&path);
            self.busy = true;
            let executor = cx.background_executor().clone();
            let text_system = cx.text_system().clone();
            let base_title = baseline.object.title.clone();
            cx.spawn(async move |this, cx| {
                let result = async {
                    let actual: Identity = client.json("GET", "session", None).await?;
                    ensure!(
                        actual.server_id == identity.server_id
                            && actual.user_id == identity.user_id,
                        HttpError::account_changed()
                    );
                    let (object, bytes) = delta::receive(
                        &client,
                        &space,
                        &link.object,
                        Some(baseline),
                        &path,
                        &executor,
                    )
                    .await?;
                    ensure!(
                        object.revision > link.object.revision,
                        "Document cannot be merged"
                    );
                    let local = rovar_storage::fs::read(&path)?;
                    let fingerprint = Sha256::digest(&local);
                    let title = merge_title(&base_title, &link.object.title, &object.title)?;
                    let (bytes, merged) = executor
                        .spawn(async move {
                            let merged = assemble(&base, &local, &bytes, &text_system)?;
                            Ok::<_, anyhow::Error>((bytes, merged))
                        })
                        .await?;
                    Ok::<_, anyhow::Error>((object, bytes, merged, fingerprint, title))
                }
                .await;
                cx.update(|cx| {
                    let protected = crate::app::Studio::protected_document_paths(cx);
                    let _ = this.update(cx, |this, cx| {
                        this.busy = false;
                        if this
                            .connection(&link.connection)
                            .is_none_or(|c| c.generation != generation || !c.authenticated)
                        {
                            cx.notify();
                            return;
                        }
                        // The local cache may have been saved while downloading.
                        // Merge its latest contents, but never an unsaved editor.
                        if protected.contains(&path) {
                            this.merge_pending.insert(path.clone());
                            cx.notify();
                            return;
                        }
                        let Some(current) =
                            this.catalog.links.get(&path).cloned().filter(|current| {
                                current.connection == link.connection
                                    && current.object.id == link.object.id
                                    && current.object.revision == link.object.revision
                                    && current.baseline == link.baseline
                                    && current.conflict
                                    && !current.object.deleted
                            })
                        else {
                            cx.notify();
                            return;
                        };
                        let result =
                            result.and_then(|(object, bytes, merged, fingerprint, title)| {
                                if current.object.title != link.object.title
                                    || Sha256::digest(rovar_storage::fs::read(&path)?)
                                        != fingerprint
                                {
                                    this.merge_pending.insert(path.clone());
                                    return Ok(());
                                }
                                this.install_merge(
                                    path.clone(),
                                    object,
                                    title,
                                    &bytes,
                                    &merged,
                                    request.input.request_id,
                                )
                            });
                        if let Err(error) = result {
                            if error.downcast_ref::<reqwest::Error>().is_some()
                                || error
                                    .downcast_ref::<HttpError>()
                                    .is_some_and(|e| e.status == 429 || e.status >= 500)
                            {
                                this.merge_pending.insert(path.clone());
                            }
                            if error
                                .downcast_ref::<HttpError>()
                                .is_some_and(|e| e.status == 401)
                            {
                                this.sign_out(&link.connection, cx);
                            }
                            if let Some(current) = this.catalog.links.get_mut(&path) {
                                current.error = Some(error.to_string());
                            }
                        }
                        cx.notify();
                    });
                });
            })
            .detach();
            cx.notify();
            return true;
        }
        false
    }
}

/// Build a complete container on the background executor. Both source files
/// remain alive until their embedded media have been copied into the output.
pub(super) fn assemble(
    base: &[u8],
    local: &[u8],
    remote: &[u8],
    text_system: &std::sync::Arc<gpui::TextSystem>,
) -> Result<Vec<u8>> {
    let local_file = rovar_storage::tempfile::NamedTempFile::new()?;
    let remote_file = rovar_storage::tempfile::NamedTempFile::new()?;
    rovar_storage::fs::write(local_file.path(), local)?;
    rovar_storage::fs::write(remote_file.path(), remote)?;
    let local = crate::document::load(local_file.path())?;
    let remote = crate::document::load(remote_file.path())?;
    let json = crate::document::merge::merge(base, &local.json, &remote.json)?;
    let mut sources = BTreeMap::new();
    for loaded in [local, remote] {
        let document = loaded.into_document()?;
        for page in document
            .pages
            .iter()
            .chain(document.components.values().map(|d| &d.page))
        {
            for source in crate::scene::components::sources(page) {
                sources.insert(source.hash.clone(), source);
            }
        }
    }
    let output = rovar_storage::tempfile::NamedTempFile::new()?;
    crate::document::save_as(
        output.path(),
        &json,
        &sources.into_values().collect::<Vec<_>>(),
        text_system,
    )?;
    let bytes = rovar_storage::fs::read(output.path())?;
    ensure!(
        bytes.len() <= rovar_api::MAX_DOCUMENT_BYTES,
        "Merged document exceeds the server's 128 MiB limit"
    );
    Ok(bytes)
}

fn merge_title(base: &str, local: &str, remote: &str) -> Result<String> {
    if local == remote || remote == base {
        return Ok(local.into());
    }
    ensure!(local == base, "Concurrent title edits");
    Ok(remote.into())
}
