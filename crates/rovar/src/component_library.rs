use crate::document::{self, AssetSource};
use anyhow::{Context as _, Result, ensure};
use gpui::{App, AppContext, Context, Entity, Global, WeakEntity};
use rovar_format::{Reader, Writer};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct Metadata {
    name: String,
    size: [f32; 2],
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub preview: Option<PathBuf>,
    pub size: [f32; 2],
    pub error: Option<String>,
}

pub(crate) struct Library {
    directory: PathBuf,
    pub entries: Vec<Entry>,
    pub busy: bool,
    pub ready: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct Libraries(HashMap<PathBuf, WeakEntity<Library>>);
impl Global for Libraries {}

impl Library {
    pub fn open(root: &Path, cx: &mut App) -> Entity<Self> {
        let directory = root.join("components");
        if let Some(library) = cx
            .try_global::<Libraries>()
            .and_then(|registry| registry.0.get(&directory))
            .and_then(WeakEntity::upgrade)
        {
            return library;
        }
        let library = cx.new(|_| Self {
            directory: directory.clone(),
            entries: Vec::new(),
            busy: false,
            ready: false,
            error: None,
        });
        if cx.try_global::<Libraries>().is_none() {
            cx.set_global(Libraries::default());
        }
        cx.global_mut::<Libraries>()
            .0
            .insert(directory, library.downgrade());
        library.update(cx, |library, cx| library.refresh(cx));
        library
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.run(|_| Ok(()), false, cx);
    }

    fn run(
        &mut self,
        operation: impl FnOnce(&Path) -> Result<()> + Send + 'static,
        needs_renderer: bool,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let directory = self.directory.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if needs_renderer {
                        crate::raster::prepare_preview().await;
                    }
                    operation(&directory)?;
                    catalog(&directory)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(entries) => {
                        this.entries = entries;
                        this.ready = true;
                        if let Some(remote) = crate::remote::Remote::existing(cx) {
                            remote.update(cx, |remote, cx| {
                                remote.library_changed(&this.directory, &this.entries, cx)
                            });
                        }
                    }
                    Err(error) => this.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn save(
        &mut self,
        name: String,
        json: Vec<u8>,
        sources: Vec<AssetSource>,
        size: [f32; 2],
        cx: &mut Context<Self>,
    ) {
        let text_system = cx.text_system().clone();
        self.run(
            move |directory| store(directory, &name, &json, &sources, size, &text_system),
            true,
            cx,
        );
    }

    pub fn rename(&mut self, entry: Entry, name: String, cx: &mut Context<Self>) {
        self.run(
            move |directory| {
                let path = component_path(directory, &entry.id)?;
                let mut writer = Writer::open(path)?;
                let mut metadata: Metadata =
                    serde_json::from_slice(&writer.snapshot().read("component", 4096)?)?;
                metadata.name = valid_name(&name)?.to_owned();
                writer.put_bytes("component", "json", &serde_json::to_vec(&metadata)?)?;
                writer.commit()?;
                Ok(())
            },
            false,
            cx,
        );
    }

    pub fn mark_failed(&mut self, id: &str, error: String, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.error = Some(error);
            entry.preview = None;
            cx.notify();
        }
    }

    pub fn remove_failed(&mut self, entry: Entry, cx: &mut Context<Self>) {
        if !self
            .entries
            .iter()
            .any(|item| item.path == entry.path && item.error.is_some())
        {
            return;
        }
        self.run(
            move |directory| {
                ensure!(
                    entry.path.parent() == Some(directory),
                    "Invalid component path"
                );
                let removed = directory.join("removed");
                rovar_storage::fs::create_dir_all(&removed)?;
                let target = removed.join(format!("{}.rovar", uuid::Uuid::new_v4()));
                match rovar_storage::fs::rename(&entry.path, target) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(error.into()),
                }
            },
            false,
            cx,
        );
    }

    pub fn delete(&mut self, entry: Entry, cx: &mut Context<Self>) {
        self.run(
            move |directory| {
                rovar_storage::fs::remove_file(component_path(directory, &entry.id)?)?;
                Ok(())
            },
            false,
            cx,
        );
    }
}

fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(
        !name.is_empty() && name.chars().count() <= 200,
        "Component name must contain 1–200 characters"
    );
    Ok(name)
}

fn component_path(directory: &Path, id: &str) -> Result<PathBuf> {
    ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid component ID");
    Ok(directory.join(format!("{id}.rovar")))
}

fn store(
    directory: &Path,
    name: &str,
    json: &[u8],
    sources: &[AssetSource],
    size: [f32; 2],
    text_system: &std::sync::Arc<gpui::TextSystem>,
) -> Result<()> {
    let name = valid_name(name)?;
    ensure!(
        size.iter().all(|v| v.is_finite() && *v >= 0.),
        "Invalid component size"
    );
    let document = document::Document::decode(json)?;
    rovar_storage::fs::create_dir_all(directory)?;
    let temporary = rovar_storage::tempfile::Builder::new()
        .suffix(".component")
        .tempfile_in(directory)?
        .into_temp_path();
    document::save_as(&temporary, json, sources, text_system)?;
    {
        let mut writer = Writer::open(&temporary)?;
        writer.put_bytes(
            "component",
            "json",
            &serde_json::to_vec(&Metadata {
                name: name.into(),
                size,
            })?,
        )?;
        writer.commit()?;
    }
    temporary
        .persist_noclobber(component_path(directory, &document.id)?)
        .context("Could not save component")?;
    Ok(())
}

fn catalog(directory: &Path) -> Result<Vec<Entry>> {
    let files = match rovar_storage::fs::read_dir(directory) {
        Ok(files) => files,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut entries = Vec::new();
    for file in files {
        let path = file?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rovar") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("Invalid component filename")?
            .to_owned();
        let mut entry = Entry {
            name: String::new(),
            id,
            path: path.clone(),
            preview: None,
            size: [0., 0.],
            error: None,
        };
        let result = (|| -> Result<()> {
            component_path(directory, &entry.id)?;
            let reader = Reader::open(&path)?;
            let metadata: Metadata = serde_json::from_slice(&reader.read("component", 4096)?)?;
            valid_name(&metadata.name)?;
            entry.name = metadata.name;
            ensure!(
                metadata.size.iter().all(|v| v.is_finite() && *v >= 0.),
                "Invalid component size"
            );
            entry.size = metadata.size;
            let previews = directory.join("previews");
            entry.preview =
                document::cache_preview(&path, &previews)?.map(|name| previews.join(name));
            Ok(())
        })();
        if let Err(error) = result {
            entry.error = Some(format!("{error:#}"));
        }
        entries.push(entry);
    }
    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests;
