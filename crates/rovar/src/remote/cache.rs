use super::*;

#[derive(Serialize, Deserialize)]
struct Incoming {
    path: PathBuf,
    previous: Option<Link>,
    next: Link,
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
        let baseline = self.store_baseline(&object, &content)?;
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
        };
        let journal = self.root.join("incoming.json");
        write_atomic(&journal, &serde_json::to_vec(&incoming)?)?;
        if let Some(bytes) = bytes {
            write_atomic(&incoming.path, bytes)?;
        } else if rovar_storage::exists(&incoming.path) {
            rovar_storage::fs::remove_file(&incoming.path)?;
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
        if matches!(
            incoming.next.object.kind,
            Kind::Component | Kind::ColorStyle
        ) {
            self.libraries_changed
                .insert(incoming.next.connection.clone());
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
                    && !current.dirty
            }
            (None, None) => true,
            _ => false,
        };
        let already_applied =
            current.is_some_and(|link| link.baseline == incoming.next.baseline && !link.dirty);
        if unchanged || already_applied {
            let baseline = self
                .read_baseline(&incoming.next)?
                .ok_or_else(|| anyhow::anyhow!("Missing incoming baseline"))?;
            let installed = if incoming.next.object.deleted {
                !rovar_storage::exists(&incoming.path)
            } else {
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
