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
        if self.recover_incoming(cx, |_, _, _| {}) {
            return true;
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
                .filter(|c| c.authenticated && self.can_start(&c.id))
            else {
                continue;
            };
            let client = connection.client();
            let identity = connection.identity.clone();
            let space = connection.space.id.clone();
            let generation = connection.generation;
            let pending = self.pending_path(&link);
            self.merge_pending.remove(&path);
            self.start_network(&link.connection);
            let executor = cx.background_executor().clone();
            let text_system = cx.text_system().clone();
            let root = self.root.clone();
            let local_generation = self.local_changes.get(&path).copied().unwrap_or_default();
            let downloads = self.root.join("downloads");
            cx.spawn(async move |this, cx| {
                let result = async {
                    let input = {
                        let root = root.clone();
                        let path = path.clone();
                        let link = link.clone();
                        executor
                            .spawn(async move { prepare(&root, &path, &link, &pending) })
                            .await
                    };
                    let Some((baseline, base, request)) = input else {
                        return Ok(Outcome::Unsupported);
                    };
                    let base_title = baseline.object.title.clone();
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
                        &downloads,
                        &executor,
                    )
                    .await?;
                    ensure!(
                        object.revision > link.object.revision,
                        "Document cannot be merged"
                    );
                    let title = merge_title(&base_title, &link.object.title, &object.title)?;
                    let path = path.clone();
                    let link = link.clone();
                    executor
                        .spawn(async move {
                            let stamp = baseline::stamp(&path, false)?;
                            let local = rovar_storage::fs::read(&path)?;
                            let fingerprint = Sha256::digest(&local);
                            let merged = assemble(&base, &local, &bytes, &text_system)?;
                            let prepared = cache::prepare_merge(
                                &root,
                                path.clone(),
                                link,
                                cache::MergeInput {
                                    object,
                                    title,
                                    server: bytes,
                                    merged,
                                    rejected_request: request,
                                },
                            )?;
                            // Full verification stays on the background executor.
                            // The UI also checks the save generation and file stamp
                            // to cover edits queued after this comparison.
                            if Sha256::digest(rovar_storage::fs::read(&path)?) != fingerprint
                                || baseline::stamp(&path, false)? != stamp
                            {
                                return Ok(Outcome::Retry);
                            }
                            Ok::<_, anyhow::Error>(Outcome::Ready {
                                prepared: Box::new(prepared),
                                stamp,
                            })
                        })
                        .await
                }
                .await;
                cx.update(|cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.finish_network(link.connection.clone(), cx, move |this, cx| {
                            let protected = crate::app::Studio::protected_document_paths(cx);
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
                            if !this.merge_is_current(&path, &link) {
                                cx.notify();
                                return;
                            }
                            let result = result.and_then(|outcome| {
                                this.publish_merge(&path, &link, local_generation, outcome)
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
                });
            })
            .detach();
            cx.notify();
            return true;
        }
        false
    }

    fn merge_is_current(&self, path: &Path, original: &Link) -> bool {
        self.catalog.links.get(path).is_some_and(|current| {
            current.connection == original.connection
                && current.object.id == original.object.id
                && current.object.revision == original.object.revision
                && current.baseline == original.baseline
                && current.conflict
                && !current.object.deleted
        })
    }

    fn publish_merge(
        &mut self,
        path: &Path,
        original: &Link,
        generation: u64,
        outcome: Outcome,
    ) -> Result<()> {
        if !self.merge_is_current(path, original) {
            return Ok(());
        }
        let current = &self.catalog.links[path];
        if current.object.title != original.object.title
            || self.local_changes.get(path).copied().unwrap_or_default() != generation
        {
            self.merge_pending.insert(path.to_owned());
            return Ok(());
        }
        match outcome {
            Outcome::Ready { prepared, stamp } if baseline::stamp(path, false)? == stamp => {
                self.install_merge(*prepared)
            }
            Outcome::Unsupported => Ok(()),
            Outcome::Retry | Outcome::Ready { .. } => {
                self.merge_pending.insert(path.to_owned());
                Ok(())
            }
        }
    }
}

enum Outcome {
    Unsupported,
    Retry,
    Ready {
        prepared: Box<cache::PreparedMerge>,
        stamp: Option<baseline::Stamp>,
    },
}

/// Unsupported or unverified content remains available in the comparison UI.
fn prepare(
    root: &Path,
    path: &Path,
    link: &Link,
    pending: &Path,
) -> Option<(baseline::Baseline, Vec<u8>, cache::RejectedRequest)> {
    let baseline = baseline::read(root, link)
        .ok()
        .flatten()
        .filter(|b| !b.object.deleted)?;
    let base = STANDARD.decode(&baseline.content).ok()?;
    crate::document::load(path).ok()?;
    let request = cache::prepare_rejected_request(pending).ok()?;
    Some((baseline, base, request))
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

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    fn ready(root: &Path, path: &Path, link: &Link) -> Outcome {
        let mut object = link.object.clone();
        object.revision += 1;
        let stamp = baseline::stamp(path, false).unwrap();
        let prepared = cache::prepare_merge(
            root,
            path.to_owned(),
            link.clone(),
            cache::MergeInput {
                object,
                title: link.object.title.clone(),
                server: b"server".to_vec(),
                merged: b"merged".to_vec(),
                rejected_request: cache::prepare_rejected_request(
                    &root
                        .join("pending")
                        .join(&link.connection)
                        .join(format!("{}.json", link.object.id)),
                )
                .unwrap(),
            },
        )
        .unwrap();
        Outcome::Ready {
            prepared: Box::new(prepared),
            stamp,
        }
    }

    #[gpui::test]
    fn stale_merges_preserve_local_edits_and_retries_until_a_fresh_merge_is_published(
        cx: &mut gpui::TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document.rovar");
        write_atomic(&path, b"local").unwrap();
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
                connection,
                "Design".into(),
                Kind::Document,
                cx,
            );
            let link = r.catalog.links.get_mut(&path).unwrap();
            link.object.revision = 1;
            link.conflict = true;
            let original = link.clone();
            let pending = r.pending_path(&original);
            let request = serde_json::to_vec(&PendingSave {
                delta: None,
                input: Save {
                    kind: Kind::Document,
                    title: "Design".into(),
                    base_revision: 1,
                    request_id: "rejected".into(),
                    content: STANDARD.encode(b"local"),
                    media: vec![],
                    deleted: false,
                },
            })
            .unwrap();
            write_atomic(&pending, &request).unwrap();

            let prepared = ready(root.path(), &path, &original);
            write_atomic(&path, b"a later saved edit").unwrap();
            r.publish_merge(&path, &original, 0, prepared).unwrap();
            assert_eq!(
                rovar_storage::fs::read(&path).unwrap(),
                b"a later saved edit"
            );
            assert!(r.merge_pending.contains(&path));
            assert!(r.link(&path).unwrap().conflict);
            assert_eq!(rovar_storage::fs::read(&pending).unwrap(), request);
            assert!(!rovar_storage::exists(root.path().join("incoming.json")));

            r.merge_pending.clear();
            let prepared = ready(root.path(), &path, &original);
            // A title edit followed by undo leaves the file stamp unchanged.
            r.changed(&path, Some("Rename".into()), false, cx);
            r.changed(&path, Some("Design".into()), false, cx);
            r.merge_pending.clear();
            r.publish_merge(&path, &original, 0, prepared).unwrap();
            assert!(r.merge_pending.contains(&path));
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            assert_eq!(
                rovar_storage::fs::read(&path).unwrap(),
                b"a later saved edit"
            );
            assert_eq!(rovar_storage::fs::read(&pending).unwrap(), request);

            let prepared = ready(root.path(), &path, &original);
            let generation = r.local_changes[&path];
            let mut newer: PendingSave = serde_json::from_slice(&request).unwrap();
            newer.input.request_id = "newer request".into();
            let newer = serde_json::to_vec(&newer).unwrap();
            write_atomic(&pending, &newer).unwrap();
            assert!(
                r.publish_merge(&path, &original, generation, prepared)
                    .is_err()
            );
            assert_eq!(rovar_storage::fs::read(&pending).unwrap(), newer);
            assert_eq!(
                rovar_storage::fs::read(&path).unwrap(),
                b"a later saved edit"
            );
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            write_atomic(&pending, &request).unwrap();
            let prepared = ready(root.path(), &path, &original);
            r.publish_merge(&path, &original, generation, prepared)
                .unwrap();
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"merged");
            let link = r.link(&path).unwrap();
            assert_eq!(link.object.revision, 2);
            assert!(link.dirty && !link.conflict);
            assert_eq!(
                r.read_baseline(link).unwrap().unwrap().content,
                STANDARD.encode(b"server")
            );
            assert!(!rovar_storage::exists(pending));
        });
    }
}
