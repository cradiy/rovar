use super::*;
use crate::scene::color_styles::{ColorStyle, Palette};

#[derive(Clone)]
pub(crate) struct Change {
    pub value: Option<ColorStyle>,
    pub name: Option<String>,
}

pub(crate) type Changes = std::collections::BTreeMap<String, Change>;

pub(crate) fn update(directory: &Path, changes: Changes) -> Result<Palette> {
    let mut palette = read(directory)?;
    if changes.is_empty() {
        return Ok(palette);
    }
    for (id, change) in changes {
        ensure!(uuid::Uuid::parse_str(&id).is_ok(), "Invalid color style ID");
        if let Some(value) = change.value {
            value.validate()?;
            palette.insert(id, value);
        } else {
            palette.remove(&id);
        }
    }
    crate::remote::write_atomic(
        &directory.join("colors.json"),
        &serde_json::to_vec(&palette)?,
    )?;
    Ok(palette)
}

pub(crate) fn read(directory: &Path) -> Result<Palette> {
    match rovar_storage::fs::read(directory.join("colors.json")) {
        Ok(bytes) => {
            let palette: Palette = serde_json::from_slice(&bytes)?;
            for (id, style) in &palette {
                ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid color style ID");
                style.validate()?;
            }
            Ok(palette)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Palette::new()),
        Err(error) => Err(error.into()),
    }
}

impl Library {
    pub fn set_color(
        &mut self,
        id: String,
        value: Option<ColorStyle>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        ensure!(
            self.ready && (!self.busy || self.writing_colors),
            "Library is busy"
        );
        ensure!(uuid::Uuid::parse_str(&id).is_ok(), "Invalid color style ID");
        if let Some(value) = &value {
            value.validate()?;
        }
        let name = value
            .as_ref()
            .or_else(|| self.colors.get(&id))
            .map(|style| style.name.clone());
        if let Some(value) = &value {
            self.colors.insert(id.clone(), value.clone());
        } else {
            self.colors.remove(&id);
        }
        self.color_changes.insert(id, Change { value, name });
        if !self.writing_colors {
            self.flush_colors(cx);
        }
        cx.notify();
        Ok(())
    }

    pub(super) fn flush_colors(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        self.writing_colors = true;
        self.error = None;
        let changes = std::mem::take(&mut self.color_changes);
        let directory = self.directory.clone();
        let remote = crate::remote::Remote::existing(cx)
            .filter(|remote| remote.read(cx).library_session(&directory).is_some());
        let task = if let Some(remote) = remote {
            remote.update(cx, |remote, cx| {
                remote.update_colors(directory, changes.clone(), cx)
            })
        } else {
            let edits = changes.clone();
            let task = cx
                .background_executor()
                .spawn(async move { update(&directory, edits) });
            cx.spawn(async move |_, _| task.await)
        };
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.writing_colors = false;
                this.busy = false;
                match result {
                    Ok(mut palette) => {
                        // Edits made during the write remain visible until the
                        // next batch is durable; a refresh cannot replace them.
                        for (id, change) in &this.color_changes {
                            if let Some(value) = &change.value {
                                palette.insert(id.clone(), value.clone());
                            } else {
                                palette.remove(id);
                            }
                        }
                        this.colors = palette;
                        if !this.color_changes.is_empty() {
                            this.flush_colors(cx);
                        } else if std::mem::take(&mut this.refresh_pending) {
                            this.refresh(cx);
                        }
                    }
                    Err(error) => {
                        // Keep failed edits for an explicit refresh/retry. Newer
                        // edits to the same style take precedence over this batch.
                        for (id, value) in changes {
                            this.color_changes.entry(id).or_insert(value);
                        }
                        this.error = Some(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn has_pending_colors(&self) -> bool {
        self.writing_colors || !self.color_changes.is_empty()
    }
}
