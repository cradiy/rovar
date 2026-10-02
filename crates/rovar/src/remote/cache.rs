use super::*;

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

enum Installation<'a> {
    Replace(&'a [u8]),
    Delete,
    Preserve,
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
        object: Object,
        bytes: Option<&[u8]>,
    ) -> Result<()> {
        self.recover_incoming()?;
        let content = baseline::snapshot_content(bytes.unwrap_or_default(), object.deleted)?;
        let baseline =
            self.store_snapshot_baseline(&object, &content, bytes.unwrap_or_default())?;
        let incoming = Incoming {
            previous: self.catalog.links.get(&path).cloned(),
            path,
            next: Link {
                connection,
                digest: digest(bytes.unwrap_or_default(), &object.title, object.deleted),
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
            bytes.map_or(Installation::Delete, Installation::Replace),
        )
    }

    pub(super) fn install_merge(
        &mut self,
        path: PathBuf,
        object: Object,
        title: String,
        server: &[u8],
        merged: &[u8],
        rejected_request: String,
    ) -> Result<()> {
        self.recover_incoming()?;
        let previous = self
            .catalog
            .links
            .get(&path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing sync link"))?;
        let confirmed = baseline::snapshot_content(server, false)?;
        let content = baseline::snapshot_content(merged, false)?;
        let mut next = previous.clone();
        next.baseline = Some(self.store_snapshot_baseline(&object, &confirmed, server)?);
        next.digest = digest(server, &object.title, false);
        next.dirty = content != confirmed || title != object.title;
        next.object = object;
        next.object.title = title;
        next.conflict = false;
        next.error = None;
        let incoming = Incoming {
            path,
            previous: Some(previous),
            next,
            installed_content: Some(hex::encode(Sha256::digest(&content))),
            rejected_request: Some(rejected_request),
        };
        self.install_incoming(incoming, Installation::Replace(merged))
    }

    pub(super) fn install_resolution(
        &mut self,
        path: &Path,
        previous: Link,
        next: Link,
        server_bytes: Option<&[u8]>,
        rejected_request: Option<String>,
    ) -> Result<()> {
        let content = match server_bytes {
            Some(bytes) => baseline::snapshot_content(bytes, next.object.deleted)?,
            None => baseline::content(path, next.object.deleted)?,
        };
        let incoming = Incoming {
            path: path.into(),
            previous: Some(previous),
            next,
            installed_content: Some(hex::encode(Sha256::digest(content))),
            rejected_request,
        };
        self.install_incoming(
            incoming,
            server_bytes.map_or(Installation::Preserve, Installation::Replace),
        )
    }

    fn install_incoming(
        &mut self,
        incoming: Incoming,
        installation: Installation<'_>,
    ) -> Result<()> {
        let journal = self.root.join("incoming.json");
        write_atomic(&journal, &serde_json::to_vec(&incoming)?)?;
        match installation {
            Installation::Replace(bytes) => write_atomic(&incoming.path, bytes)?,
            Installation::Delete if rovar_storage::exists(&incoming.path) => {
                rovar_storage::fs::remove_file(&incoming.path)?;
            }
            Installation::Delete | Installation::Preserve => {}
        }
        self.finish_incoming(&incoming)?;
        rovar_storage::fs::remove_file(journal)?;
        Ok(())
    }

    fn finish_incoming(&mut self, incoming: &Incoming) -> Result<()> {
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
        self.retire_rejected_request(incoming)?;
        if matches!(
            incoming.next.object.kind,
            Kind::Component | Kind::ColorStyle
        ) {
            self.libraries_changed
                .insert(incoming.next.connection.clone());
        }
        Ok(())
    }

    fn retire_rejected_request(&self, incoming: &Incoming) -> Result<()> {
        if let Some(request) = &incoming.rejected_request {
            let pending = self.pending_path(&incoming.next);
            if rovar_storage::exists(&pending) {
                let saved: PendingSave =
                    serde_json::from_slice(&rovar_storage::fs::read(&pending)?)?;
                if saved.input.request_id == *request {
                    rovar_storage::fs::remove_file(pending)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn recover_incoming(&mut self) -> Result<()> {
        let journal = self.root.join("incoming.json");
        if !rovar_storage::exists(&journal) {
            return Ok(());
        }
        let incoming: Incoming = serde_json::from_slice(&rovar_storage::fs::read(&journal)?)?;
        let current = self.catalog.links.get(&incoming.path);
        let unchanged = match (current, incoming.previous.as_ref()) {
            (Some(current), Some(previous)) => {
                current.connection == previous.connection
                    && current.object.id == previous.object.id
                    && current.object.revision == previous.object.revision
                    && current.object.title == previous.object.title
                    && current.object.deleted == previous.object.deleted
                    && current.baseline == previous.baseline
                    && current.dirty == previous.dirty
                    && current.conflict == previous.conflict
            }
            (None, None) => true,
            _ => false,
        };
        let already_applied = current.is_some_and(|link| {
            link.baseline == incoming.next.baseline
                && link.object.revision == incoming.next.object.revision
                && link.object.id == incoming.next.object.id
                && link.connection == incoming.next.connection
                && link.conflict == incoming.next.conflict
        });
        if already_applied {
            // The catalog committed before a crash, but retiring the rejected
            // request may not have. Do not reset any subsequent local edits.
            self.retire_rejected_request(&incoming)?;
        } else if unchanged {
            // A present baseline must still pass integrity checks. Only the
            // explicit local-resolution path may recover without one.
            let baseline = self.read_baseline(&incoming.next)?;
            let installed = if incoming.next.object.deleted {
                !rovar_storage::exists(&incoming.path)
            } else if let Some(hash) = &incoming.installed_content {
                // Keeping local content has no confirmed server baseline. Its
                // recorded content hash is the recovery witness instead.
                baseline::content(&incoming.path, false)
                    .is_ok_and(|content| hex::encode(Sha256::digest(content)) == *hash)
            } else {
                let baseline =
                    baseline.ok_or_else(|| anyhow::anyhow!("Missing incoming baseline"))?;
                baseline::content(&incoming.path, false)
                    .is_ok_and(|content| STANDARD.encode(content) == baseline.content)
            };
            if installed {
                self.finish_incoming(&incoming)?;
            }
        }
        rovar_storage::fs::remove_file(journal)?;
        Ok(())
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
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
            r.install_snapshot(path.clone(), connection, object.clone(), Some(b"base"))
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
            r.recover_incoming().unwrap();
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            assert!(rovar_storage::exists(&pending));
            // Crash after writing the merged cache, before committing its revision.
            write_atomic(&journal, &record).unwrap();
            write_atomic(&path, b"merged").unwrap();
            r.recover_incoming().unwrap();
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
            r.recover_incoming().unwrap();
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
            r.install_snapshot(
                path.clone(),
                connection,
                object.clone(),
                Some(b"version one"),
            )
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
            r.recover_incoming().unwrap();
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            // Interrupted after the file write but before the catalog commit.
            write_atomic(&journal, &record).unwrap();
            write_atomic(&path, b"version two").unwrap();
            r.recover_incoming().unwrap();
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
            r.recover_incoming().unwrap();
            r.reconcile_baseline(&path).unwrap();
            assert!(r.link(&path).unwrap().dirty);
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"new local edits");
        });
    }
}
