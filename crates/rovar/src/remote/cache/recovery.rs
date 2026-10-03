use super::*;

enum Action {
    Discard,
    Finish,
    Retire,
}

struct Prepared {
    incoming: Incoming,
    observed: Option<Link>,
    generation: u64,
    journal_stamp: Option<baseline::Stamp>,
    // None means no document verification was needed; Some(None) is a tombstone.
    local_stamp: Option<Option<baseline::Stamp>>,
    pending_stamp: Option<Option<baseline::Stamp>>,
    request_check: RequestCheck,
    action: Action,
}

fn same_link(current: Option<&Link>, previous: Option<&Link>) -> bool {
    match (current, previous) {
        (Some(current), Some(previous)) => {
            current.connection == previous.connection
                && current.object.id == previous.object.id
                && current.object.revision == previous.object.revision
                && current.object.title == previous.object.title
                && current.object.deleted == previous.object.deleted
                && current.baseline == previous.baseline
                && current.digest == previous.digest
                && current.dirty == previous.dirty
                && current.conflict == previous.conflict
        }
        (None, None) => true,
        _ => false,
    }
}

fn prepare(
    root: &Path,
    links: &BTreeMap<PathBuf, Link>,
    generations: &BTreeMap<PathBuf, u64>,
) -> Result<Prepared> {
    let journal = root.join("incoming.json");
    let journal_stamp = optional_stamp(&journal)?;
    let incoming: Incoming = serde_json::from_slice(&rovar_storage::fs::read(&journal)?)?;
    let observed = links.get(&incoming.path).cloned();
    let generation = generations.get(&incoming.path).copied().unwrap_or_default();
    let already_applied = observed.as_ref().is_some_and(|link| {
        link.baseline == incoming.next.baseline
            && link.object.revision == incoming.next.object.revision
            && link.object.id == incoming.next.object.id
            && link.connection == incoming.next.connection
            && link.conflict == incoming.next.conflict
    });
    let mut local_stamp = None;
    let action = if already_applied {
        // A committed catalog needs only request retirement. Later edits are
        // independent of this receipt and must not be overwritten.
        Action::Retire
    } else if same_link(observed.as_ref(), incoming.previous.as_ref()) {
        let stamp = optional_stamp(&incoming.path)?;
        // Even a semantic witness cannot bypass validation of a present baseline.
        let baseline = baseline::read(root, &incoming.next)?;
        let installed = if incoming.next.object.deleted {
            stamp.is_none()
        } else if let Some(hash) = &incoming.installed_content {
            baseline::content(&incoming.path, false)
                .is_ok_and(|content| hex::encode(Sha256::digest(content)) == *hash)
        } else {
            let baseline = baseline.ok_or_else(|| anyhow::anyhow!("Missing incoming baseline"))?;
            baseline::content(&incoming.path, false)
                .is_ok_and(|content| STANDARD.encode(content) == baseline.content)
        };
        ensure!(
            optional_stamp(&incoming.path)? == stamp,
            "Document changed during sync recovery; retrying"
        );
        local_stamp = Some(stamp);
        if installed {
            Action::Finish
        } else {
            Action::Discard
        }
    } else {
        Action::Discard
    };
    let mut pending_stamp = None;
    let mut request_check = RequestCheck::Skip;
    if !matches!(action, Action::Discard)
        && let Some(request) = &incoming.rejected_request
    {
        let pending = root
            .join("pending")
            .join(&incoming.next.connection)
            .join(format!("{}.json", incoming.next.object.id));
        let stamp = optional_stamp(&pending)?;
        if stamp.is_some() {
            let saved = prepare_rejected_request(&pending)?;
            ensure!(
                saved.stamp == stamp,
                "Pending request changed during sync recovery; retrying"
            );
            if saved.id == *request {
                request_check = RequestCheck::Verified;
            }
        }
        pending_stamp = Some(stamp);
    }
    ensure!(
        optional_stamp(&journal)? == journal_stamp,
        "Recovery journal changed; retrying"
    );
    Ok(Prepared {
        incoming,
        observed,
        generation,
        journal_stamp,
        local_stamp,
        pending_stamp,
        request_check,
        action,
    })
}

impl Remote {
    /// Returns true when recovery owns the worker. The callback runs after it
    /// releases the worker, allowing sync/refresh to continue without polling.
    pub(in crate::remote) fn recover_incoming(
        &mut self,
        cx: &mut Context<Self>,
        done: impl FnOnce(Result<()>, &mut Self, &mut Context<Self>) + 'static,
    ) -> bool {
        if !rovar_storage::exists(self.root.join("incoming.json")) {
            return false;
        }
        debug_assert!(!self.busy);
        self.busy = true;
        let root = self.root.clone();
        let links = self.catalog.links.clone();
        let generations = self.local_changes.clone();
        let task = cx
            .background_executor()
            .spawn(async move { prepare(&root, &links, &generations) });
        cx.spawn(async move |this, cx| {
            let prepared = task.await;
            let _ = this.update(cx, |this, cx| {
                let result = prepared.and_then(|prepared| this.apply_recovery(prepared));
                this.busy = false;
                if let Err(error) = &result {
                    this.error = Some(error.to_string());
                }
                done(result, this, cx);
                this.publish_completed(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        true
    }

    fn apply_recovery(&mut self, prepared: Prepared) -> Result<()> {
        let Prepared {
            incoming,
            observed,
            generation,
            journal_stamp,
            local_stamp,
            pending_stamp,
            request_check,
            action,
        } = prepared;
        let journal = self.root.join("incoming.json");
        ensure!(
            optional_stamp(&journal)? == journal_stamp,
            "Recovery journal changed; retrying"
        );
        ensure!(
            same_link(self.catalog.links.get(&incoming.path), observed.as_ref())
                && self
                    .local_changes
                    .get(&incoming.path)
                    .copied()
                    .unwrap_or_default()
                    == generation,
            "Local document changed during sync recovery; retrying"
        );
        if let Some(stamp) = local_stamp {
            ensure!(
                optional_stamp(&incoming.path)? == stamp,
                "Document changed during sync recovery; retrying"
            );
        }
        if let Some(stamp) = pending_stamp {
            ensure!(
                optional_stamp(&self.pending_path(&incoming.next))? == stamp,
                "Pending request changed during sync recovery; retrying"
            );
        }
        match action {
            Action::Finish => self.finish_incoming(&incoming, request_check)?,
            Action::Retire => self.retire_rejected_request(&incoming, request_check)?,
            Action::Discard => {}
        }
        rovar_storage::fs::remove_file(journal)?;
        Ok(())
    }
}

// Crash-state unit tests drive the same preparation/publication phases without
// an executor; worker scheduling and invalidation are tested separately below.
#[cfg(all(test, not(target_family = "wasm")))]
pub(super) fn recover_for_test(remote: &mut Remote) -> Result<()> {
    let prepared = prepare(&remote.root, &remote.catalog.links, &remote.local_changes)?;
    remote.apply_recovery(prepared)
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn interrupted(root: &Path) -> (PathBuf, PathBuf, Catalog) {
        let path = root.join("document.rovar");
        let object = Object {
            id: uuid::Uuid::new_v4().to_string(),
            kind: Kind::Document,
            title: "Design".into(),
            revision: 1,
            created: 1,
            modified: 1,
            deleted: false,
        };
        let previous = Link {
            connection: "account".into(),
            object: object.clone(),
            dirty: false,
            digest: digest(b"old", "Design", false),
            baseline: None,
            conflict: false,
            error: None,
        };
        let mut next = previous.clone();
        next.object.revision = 2;
        next.digest = digest(b"server", "Design", false);
        next.baseline = Some(baseline::store(root, &next.object, b"server", None).unwrap());
        let incoming = Incoming {
            path: path.clone(),
            previous: Some(previous.clone()),
            next,
            installed_content: None,
            rejected_request: Some("old-request".into()),
        };
        let pending = root
            .join("pending/account")
            .join(format!("{}.json", object.id));
        write_atomic(
            &pending,
            &serde_json::to_vec(&PendingSave {
                delta: None,
                input: Save {
                    kind: Kind::Document,
                    title: "Design".into(),
                    base_revision: 1,
                    request_id: "old-request".into(),
                    content: STANDARD.encode(b"server"),
                    media: vec![],
                    deleted: false,
                },
            })
            .unwrap(),
        )
        .unwrap();
        let mut catalog = Catalog::default();
        catalog.links.insert(path.clone(), previous);
        write_atomic(
            &root.join("servers.json"),
            &serde_json::to_vec(&catalog).unwrap(),
        )
        .unwrap();
        write_atomic(&path, b"server").unwrap();
        write_atomic(
            &root.join("incoming.json"),
            &serde_json::to_vec(&incoming).unwrap(),
        )
        .unwrap();
        (path, pending, catalog)
    }

    #[gpui::test]
    fn startup_recovers_the_durable_snapshot_before_releasing_the_worker(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (path, pending, _) = interrupted(root.path());
        let remote = cx.update(|cx| Remote::shared(root.path(), cx));
        remote.read_with(cx, |r, _| {
            assert!(r.busy);
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
        });
        crate::remote::tests::wait_sync(&remote, cx);
        remote.read_with(cx, |r, _| {
            assert!(r.error.is_none());
            assert_eq!(r.link(&path).unwrap().object.revision, 2);
            assert!(!r.link(&path).unwrap().dirty);
        });
        let durable: Catalog = serde_json::from_slice(
            &rovar_storage::fs::read(root.path().join("servers.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(durable.links[&path].object.revision, 2);
        assert!(!pending.exists() && !root.path().join("incoming.json").exists());
    }

    #[gpui::test]
    fn edits_during_startup_keep_the_journal_and_are_protected_on_retry(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (path, pending, _) = interrupted(root.path());
        let remote = cx.update(|cx| Remote::shared(root.path(), cx));
        write_atomic(&path, b"later local edit").unwrap();
        remote.update(cx, |r, cx| r.changed(&path, None, false, cx));
        crate::remote::tests::wait_sync(&remote, cx);
        assert!(root.path().join("incoming.json").exists());
        remote.update(cx, |r, cx| {
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            assert!(r.link(&path).unwrap().dirty);
            assert!(r.recover_incoming(cx, |result, _, _| result.unwrap()));
        });
        crate::remote::tests::wait_sync(&remote, cx);
        assert_eq!(rovar_storage::fs::read(&path).unwrap(), b"later local edit");
        assert!(pending.exists());
        assert!(!root.path().join("incoming.json").exists());
        remote.read_with(cx, |r, _| assert!(r.link(&path).unwrap().dirty));
    }

    #[gpui::test]
    fn completed_checks_cannot_retire_a_new_request_or_publish_over_a_later_save(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let remote = cx.update(|cx| Remote::shared(root.path(), cx));
        let (path, pending, catalog) = interrupted(root.path());
        remote.update(cx, |r, _| {
            r.catalog = catalog;
            let request = rovar_storage::fs::read(&pending).unwrap();
            let prepared = prepare(root.path(), &r.catalog.links, &r.local_changes).unwrap();
            let mut newer: PendingSave = serde_json::from_slice(&request).unwrap();
            newer.input.request_id = "new-request-after-scan".into();
            let newer = serde_json::to_vec(&newer).unwrap();
            write_atomic(&pending, &newer).unwrap();
            assert!(r.apply_recovery(prepared).is_err());
            assert_eq!(rovar_storage::fs::read(&pending).unwrap(), newer);
            assert_eq!(r.link(&path).unwrap().object.revision, 1);
            write_atomic(&pending, &request).unwrap();
            let prepared = prepare(root.path(), &r.catalog.links, &r.local_changes).unwrap();
            write_atomic(&path, b"new save before its UI callback").unwrap();
            assert!(r.apply_recovery(prepared).is_err());
            assert!(root.path().join("incoming.json").exists());
            assert_eq!(
                rovar_storage::fs::read(&path).unwrap(),
                b"new save before its UI callback"
            );
            assert_eq!(rovar_storage::fs::read(&pending).unwrap(), request);
        });
    }
}
