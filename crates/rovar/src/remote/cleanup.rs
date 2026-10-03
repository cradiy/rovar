use super::*;
use rovar_storage::fs;

const INTERVAL: u64 = 3600;
const BATCH: usize = 64;

#[derive(Default)]
pub(super) struct Cleanup {
    last: Option<web_time::Instant>,
    scanning: bool,
    pending: Option<Vec<String>>,
}

impl Remote {
    pub(super) fn cleanup_baselines(&mut self, cx: &mut Context<Self>) {
        // Background downloads can still hold old links. Delete only between
        // sync operations, serialized with all baseline publication on the UI.
        if self.is_busy() {
            return;
        }
        if let Some(candidates) = self.baseline_cleanup.pending.take() {
            match prune(&self.root, &self.catalog, &candidates) {
                Ok(()) if candidates.len() == BATCH => {
                    self.baseline_cleanup.last = None;
                }
                Ok(()) => {}
                Err(error) => eprintln!("Sync baseline cleanup failed: {error}"),
            }
        }
        if self.baseline_cleanup.scanning
            || self
                .baseline_cleanup
                .last
                .is_some_and(|last| last.elapsed().as_secs() < INTERVAL)
        {
            return;
        }
        self.baseline_cleanup.scanning = true;
        self.baseline_cleanup.last = Some(web_time::Instant::now());
        let root = self.root.clone();
        let protected = live_references(&self.catalog);
        let scan = cx
            .background_executor()
            .spawn(async move { candidates(&root, protected) });
        cx.spawn(async move |this, cx| {
            let result = scan.await;
            let _ = this.update(cx, |this, _| {
                this.baseline_cleanup.scanning = false;
                match result {
                    Ok(candidates) if !candidates.is_empty() => {
                        // The next idle sync tick rechecks references before
                        // deleting this bounded batch; the scan never deletes.
                        this.baseline_cleanup.pending = Some(candidates);
                    }
                    Ok(_) => {}
                    Err(error) => eprintln!("Sync baseline scan failed: {error}"),
                }
            });
        })
        .detach();
    }
}

fn live_references(catalog: &Catalog) -> BTreeSet<String> {
    catalog
        .links
        .values()
        .filter_map(|link| link.baseline.clone())
        .collect()
}

fn protect_durable(root: &Path, protected: &mut BTreeSet<String>) -> Result<()> {
    // A missing/damaged catalog is not proof that a baseline is disposable.
    // Also retain disk-only references when the latest persist failed.
    let catalog: Catalog = serde_json::from_slice(&fs::read(root.join("servers.json"))?)?;
    protected.extend(live_references(&catalog));
    cache::protect_baselines(root, protected)?;
    // PendingSave owns its full immutable snapshot and frozen delta. It has no
    // baseline-file references; pending/conflicting links above stay protected.
    Ok(())
}

fn is_key(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn candidates(root: &Path, mut protected: BTreeSet<String>) -> Result<Vec<String>> {
    let entries = match fs::read_dir(root.join("baselines")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    protect_durable(root, &mut protected)?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(key) = name.to_str().filter(|key| is_key(key)) else {
            continue;
        };
        if !protected.contains(key) && entry.file_type()?.is_file() {
            candidates.push(key.to_owned());
            if candidates.len() == BATCH {
                break;
            }
        }
    }
    Ok(candidates)
}

fn prune(root: &Path, current: &Catalog, candidates: &[String]) -> Result<()> {
    let mut protected = live_references(current);
    protect_durable(root, &mut protected)?;
    for key in candidates.iter().take(BATCH) {
        if !is_key(key) || protected.contains(key) {
            continue;
        }
        let path = root.join("baselines").join(key);
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;
