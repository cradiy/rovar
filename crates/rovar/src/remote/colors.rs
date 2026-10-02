use super::*;
use crate::{
    document::library::colors,
    scene::color_styles::{ColorStyle, Palette},
};

impl Remote {
    /// Local palettes remain the library's source of truth. Per-style snapshots
    /// participate in the existing revisioned upload queue independently.
    pub fn colors_changed(
        &mut self,
        directory: &Path,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let Some(connection) = self
            .connections()
            .iter()
            .find(|c| self.library_root(&c.id).join("components") == directory)
            .map(|c| c.id.clone())
        else {
            return Ok(());
        };
        let cache = self.library_root(&connection).join("colors");
        let mut paths = BTreeSet::new();
        let mut changed = false;
        for (id, style) in palette {
            ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid color style ID");
            style.validate()?;
            let path = cache.join(format!("{id}.json"));
            paths.insert(path.clone());
            let bytes = serde_json::to_vec(style)?;
            let hash = digest(&bytes, &style.name, false);
            if rovar_storage::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
                write_atomic(&path, &bytes)?;
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
                        connection: connection.clone(),
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
        Ok(())
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
        self.colors_changed(&directory, &palette, cx)?;
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
