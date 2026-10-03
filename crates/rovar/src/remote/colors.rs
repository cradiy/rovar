use super::*;
use crate::{
    document::library::colors,
    scene::color_styles::{ColorStyle, Palette},
};

struct Prepared {
    palette: Palette,
    entries: Vec<(String, PathBuf, String, bool)>,
    removed: BTreeMap<String, String>,
    edited: bool,
}

fn prepare(directory: &Path, changes: colors::Changes) -> Result<Prepared> {
    let edited: BTreeSet<_> = changes.keys().cloned().collect();
    let removed = changes
        .iter()
        .filter_map(|(id, change)| {
            change
                .value
                .is_none()
                .then(|| change.name.clone().map(|name| (id.clone(), name)))
                .flatten()
        })
        .collect();
    let palette = colors::update(directory, changes)?;
    let cache = directory
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid library path"))?
        .join("colors");
    let mut entries = Vec::with_capacity(palette.len());
    for (id, style) in &palette {
        if !edited.is_empty() && !edited.contains(id) {
            continue;
        }
        let path = cache.join(format!("{id}.json"));
        let bytes = serde_json::to_vec(style)?;
        let hash = digest(&bytes, &style.name, false);
        let changed = match rovar_storage::fs::read(&path) {
            Ok(current) => current != bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => return Err(error.into()),
        };
        if changed {
            write_atomic(&path, &bytes)?;
        }
        entries.push((id.clone(), path, hash, changed));
    }
    Ok(Prepared {
        palette,
        entries,
        removed,
        edited: !edited.is_empty(),
    })
}

impl Remote {
    /// Serialize palette writes with download/receipt publication. The disk
    /// palette is read when work starts so unrelated remote changes survive.
    pub fn update_colors(
        &mut self,
        directory: PathBuf,
        changes: colors::Changes,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<Palette>> {
        let session = self.library_session(&directory);
        if let Some((connection, _)) = &session {
            for id in changes.keys() {
                let path = self
                    .library_root(connection)
                    .join("colors")
                    .join(format!("{id}.json"));
                *self.local_changes.entry(path.clone()).or_default() += 1;
                if let Some(link) = self.catalog.links.get_mut(&path) {
                    link.dirty = true;
                }
            }
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        self.scheduler
            .completed
            .push_back(Box::new(move |this, cx| {
                let Some((connection, _)) = session.filter(|(id, _)| this.connection(id).is_some())
                else {
                    let _ =
                        sender.send(Err(anyhow::anyhow!("Library connection no longer exists")));
                    return;
                };
                this.busy = true;
                let task = cx
                    .background_executor()
                    .spawn(async move { prepare(&directory, changes) });
                cx.spawn(async move |this, cx| {
                    let prepared = task.await;
                    let result = this
                        .update(cx, |this, cx| {
                            let result = prepared.and_then(|prepared| {
                                this.install_colors(&connection, prepared, cx)
                            });
                            this.busy = false;
                            if let Err(error) = &result {
                                this.error = Some(error.to_string());
                                this.persist();
                            }
                            this.publish_completed(cx);
                            cx.notify();
                            result
                        })
                        .and_then(|result| result);
                    let _ = sender.send(result);
                })
                .detach();
            }));
        self.publish_completed(cx);
        cx.spawn(async move |_, _| receiver.await?)
    }

    fn install_colors(
        &mut self,
        connection: &str,
        prepared: Prepared,
        cx: &mut Context<Self>,
    ) -> Result<Palette> {
        let cache = self.library_root(connection).join("colors");
        let paths: BTreeSet<_> = prepared
            .palette
            .keys()
            .map(|id| cache.join(format!("{id}.json")))
            .collect();
        let mut changed = prepared.edited;
        for (id, path, hash, written) in prepared.entries {
            let style = &prepared.palette[&id];
            if written {
                *self.local_changes.entry(path.clone()).or_default() += 1;
            }
            if let Some(link) = self.catalog.links.get_mut(&path) {
                if (link.digest != hash && !link.dirty)
                    || link.object.title != style.name
                    || link.object.deleted
                {
                    link.object.title = style.name.clone();
                    link.object.deleted = false;
                    link.dirty = true;
                    changed = true;
                }
            } else {
                self.catalog.links.insert(
                    path,
                    Link {
                        connection: connection.to_owned(),
                        object: Object {
                            id: id.clone(),
                            kind: Kind::ColorStyle,
                            title: style.name.clone(),
                            revision: 0,
                            created: crate::platform::now(),
                            modified: crate::platform::now(),
                            deleted: false,
                        },
                        dirty: true,
                        digest: String::new(),
                        baseline: None,
                        conflict: false,
                        error: None,
                    },
                );
                changed = true;
            }
        }
        for (path, link) in &mut self.catalog.links {
            if link.connection == connection
                && link.object.kind == Kind::ColorStyle
                && !link.object.deleted
                && !paths.contains(path)
            {
                if let Some(name) = prepared.removed.get(&link.object.id) {
                    link.object.title = name.clone();
                }
                link.object.deleted = true;
                link.dirty = true;
                *self.local_changes.entry(path.clone()).or_default() += 1;
                changed = true;
            }
        }
        if changed {
            ensure!(
                self.persist(),
                "Could not persist color synchronization: {}",
                self.error.as_deref().unwrap_or_default()
            );
            cx.notify();
        }
        Ok(prepared.palette)
    }

    /// Merge a remote item into the local palette, preserving unrelated colors.
    pub(super) fn apply_color(
        &self,
        connection: &str,
        object: &Object,
        bytes: Option<&[u8]>,
    ) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&object.id).is_ok(),
            "Invalid remote color style ID"
        );
        let directory = self.library_root(connection).join("components");
        let mut palette = colors::read(&directory)?;
        if let Some(bytes) = bytes {
            let style: ColorStyle = serde_json::from_slice(bytes)?;
            style.validate()?;
            ensure!(
                style.name == object.title,
                "Color style name does not match metadata"
            );
            palette.insert(object.id.clone(), style);
        } else {
            palette.remove(&object.id);
        }
        write_atomic(
            &directory.join("colors.json"),
            &serde_json::to_vec(&palette)?,
        )
    }

    /// A concurrently edited color becomes an independent local copy; the
    /// original is fetched again so neither version is silently overwritten.
    pub(super) fn preserve_color_conflict(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let link = self.catalog.links.get(path).unwrap().clone();
        let directory = self.library_root(&link.connection).join("components");
        let mut palette = colors::read(&directory)?;
        if let Some(mut style) = palette.remove(&link.object.id) {
            style.name = crate::i18n::message(
                "page-copy-name",
                &[("name", style.name.chars().take(180).collect())],
            );
            palette.insert(uuid::Uuid::new_v4().to_string(), style);
        }
        write_atomic(
            &directory.join("colors.json"),
            &serde_json::to_vec(&palette)?,
        )?;
        let prepared = prepare(&directory, Default::default())?;
        self.install_colors(&link.connection, prepared, cx)?;
        let current = self.catalog.links.get_mut(path).unwrap();
        current.dirty = false;
        current.conflict = false;
        current.error = None;
        current.object.revision = 0;
        ensure!(self.persist(), "Could not persist color conflict recovery");
        let pending = self
            .root
            .join("pending")
            .join(&link.connection)
            .join(format!("{}.json", link.object.id));
        if rovar_storage::exists(&pending) {
            rovar_storage::fs::remove_file(pending)?;
        }
        self.refresh_at.remove(&link.connection);
        self.libraries_changed.insert(link.connection);
        Ok(())
    }
}
