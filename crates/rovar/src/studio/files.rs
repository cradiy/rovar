use super::*;
use crate::document;

impl Studio {
    fn finish_unchanged_save(
        &mut self,
        token: usize,
        view_changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.token == token) else {
            return;
        };
        let requested = std::mem::take(&mut tab.save_requested);
        let can_close = tab.close_after_save
            && !tab.exporting
            && tab
                .editor
                .as_ref()
                .is_none_or(|editor| !editor.read(cx).is_exporting());
        if can_close || view_changed || requested {
            let file = tab.file.clone();
            self.remember(file);
            if can_close {
                self.remove_tab(token, window, cx);
            } else {
                self.persist_session();
            }
        }
        if requested {
            cx.notify();
        }
    }

    pub(super) fn remember(&mut self, file: Recent) {
        let mut session = self.session.borrow_mut();
        session.recent.retain(|item| item.path != file.path);
        session.recent.insert(0, file);
    }

    pub(super) fn persist_session(&mut self) {
        let mut session = self.session.borrow_mut();
        let mut paths: Vec<_> = self.tabs.iter().map(|tab| tab.file.path.clone()).collect();
        if let Some(drag) = &self.strip.drag
            && drag.window.window_id().as_u64() == self.window_id
            && let Some(tab) = &drag.transaction.borrow().detached
        {
            paths.push(tab.file.path.clone());
        }
        session.windows.insert(self.window_id, paths);
        session.open = session.windows.values().flatten().cloned().collect();
        let mut unique = std::collections::BTreeSet::new();
        session.open.retain(|path| unique.insert(path.clone()));
        let result = (|| -> anyhow::Result<()> {
            rovar_storage::fs::create_dir_all(&self.directory)?;
            let mut temp = rovar_storage::tempfile::NamedTempFile::new_in(&self.directory)?;
            serde_json::to_writer(&mut temp, &*session)?;
            temp.as_file().sync_all()?;
            temp.persist(self.directory.join("session.json"))?;
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }

    pub(super) fn autosave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tokens: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| t.editor.is_some() && !t.saving)
            .map(|t| t.token)
            .collect();
        for token in tokens {
            self.save_tab(token, window, cx);
        }
        if self.closing {
            self.finish_close(window, cx);
        }
    }

    pub(super) fn save_tab(&mut self, token: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|t| t.token == token) else {
            return;
        };
        let tab = &mut self.tabs[index];
        if tab.saving || self.dragging == Some(token) {
            return;
        }
        let Some(editor) = &tab.editor else {
            return;
        };
        if let Some(error) = editor.update(cx, |editor, _| editor.take_export_failure())
            && (tab.close_after_save || self.closing)
        {
            tab.close_after_save = false;
            self.closing = false;
            self.quit_requested = false;
            self.error = Some(error);
            cx.notify();
        }
        if !editor.read(cx).save_ready(cx) {
            return;
        }
        let revision = editor.read(cx).document_revision();
        let views = editor.read(cx).page_views();
        let view_changed = views != tab.file.views;
        tab.file.views = views;
        if !tab.needs_upgrade && tab.saved_revision == Some(revision) {
            self.finish_unchanged_save(token, view_changed, window, cx);
            return;
        }
        let (json, assets) = match editor.read(cx).snapshot_document(&tab.document_id, cx) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tab.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        if !tab.needs_upgrade && json == tab.last_saved {
            tab.saved_revision = Some(revision);
            self.finish_unchanged_save(token, view_changed, window, cx);
            return;
        }
        let path = tab.file.path.clone();
        if !is_internal(&self.directory, &path) {
            tab.error = Some("Document is outside the workspace".into());
            cx.notify();
            return;
        }
        let expected = tab.last_saved.clone();
        let previews = self.directory.join("previews");
        tab.saving = true;
        tab.error = None;
        cx.notify();
        let text_system = cx.text_system().clone();
        cx.spawn_in(window, async move |this, cx| {
            let bytes = json.clone();
            let output = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    if let Some(parent) = output.parent() {
                        rovar_storage::fs::create_dir_all(parent)?;
                    }
                    document::save(&output, &bytes, &assets, &expected, &text_system)?;
                    Ok::<_, anyhow::Error>(
                        document::cache_preview(&output, &previews).ok().flatten(),
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(tab) = this.tabs.iter_mut().find(|t| t.token == token) else {
                    return;
                };
                tab.saving = false;
                match result {
                    Ok(preview) => {
                        tab.file.preview = preview;
                        tab.last_saved = json;
                        tab.saved_revision = Some(revision);
                        tab.needs_upgrade = false;
                        tab.file.modified = now();
                        let file = tab.file.clone();
                        let follow_up = tab.close_after_save || tab.save_requested;
                        this.remember(file);
                        this.persist_session();
                        if follow_up {
                            this.save_tab(token, window, cx);
                        }
                        if this.closing {
                            this.finish_close(window, cx);
                        }
                    }
                    Err(error) => {
                        tab.save_requested = false;
                        tab.error = Some(error.to_string());
                        tab.close_after_save = false;
                        this.closing = false;
                        this.quit_requested = false;
                        this.error = Some(crate::i18n::message(
                            "save-failed",
                            &[("error", error.to_string())],
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = crate::platform::prompt_for_paths(
            cx,
            gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: true,
                prompt: Some(t("open-document").into()),
            },
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = dialog.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    for path in paths {
                        this.open_path(path, window, cx);
                    }
                }
                Ok(Ok(None)) => {}
                error => {
                    this.error = Some(format!("{error:?}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let path = rovar_storage::fs::canonicalize(&path).unwrap_or(path);
        self.restore_recent(&path, cx);
        for other in cx
            .windows()
            .into_iter()
            .filter(|w| w.window_id().as_u64() != self.window_id)
            .filter_map(|w| w.downcast::<Studio>())
        {
            let opened = other
                .update(cx, |studio, window, cx| {
                    if let Some(token) = studio
                        .tabs
                        .iter()
                        .find(|tab| tab.file.path == path)
                        .map(|tab| tab.token)
                    {
                        studio.select_tab(Some(token), window, cx);
                        window.activate_window();
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if opened {
                return;
            }
        }
        if let Some(tab) = self.tabs.iter().find(|tab| tab.file.path == path) {
            let token = tab.token;
            self.select_tab(Some(token), window, cx);
            return;
        }
        let file = self
            .session
            .borrow()
            .recent
            .iter()
            .find(|file| file.path == path)
            .cloned()
            .unwrap_or_else(|| Recent {
                title: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                path,
                created: sorting::creation_time(),
                modified: now(),
                views: Default::default(),
                preview: None,
            });
        let token = self.next_token;
        self.next_token += 1;
        self.tabs.push(Tab {
            token,
            file,
            document_id: String::new(),
            editor: None,
            last_saved: Vec::new(),
            saved_revision: None,
            needs_upgrade: false,
            loading: false,
            saving: false,
            close_after_save: false,
            exporting: false,
            save_requested: false,
            error: None,
            _subscription: None,
        });
        self.select_tab(Some(token), window, cx);
    }

    pub(super) fn load_tab(&mut self, token: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.token == token) else {
            return;
        };
        if tab.loading || tab.editor.is_some() {
            return;
        }
        tab.loading = true;
        tab.error = None;
        let path = tab.file.path.clone();
        let internal = is_internal(&self.directory, &path);
        let documents = self.directory.join("documents");
        let previews = self.directory.join("previews");
        let text_system = cx.text_system().clone();
        cx.spawn_in(window, async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let (loaded, path) = if internal {
                        (document::load(&path)?, path)
                    } else {
                        document::import(&path, &documents, &text_system)?
                    };
                    Ok::<_, anyhow::Error>((
                        loaded,
                        document::cache_preview(&path, &previews).ok().flatten(),
                        path,
                    ))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(tab) = this.tabs.iter_mut().find(|tab| tab.token == token) else {
                    return;
                };
                tab.loading = false;
                let result = loaded.and_then(|(loaded, preview, path)| {
                    tab.needs_upgrade = loaded.needs_upgrade;
                    tab.file.path = path;
                    tab.file.preview = preview;
                    let json = loaded.json.clone();
                    let editor = cx.new(|cx| Workspace::new(window, cx));
                    editor.update(cx, |editor, cx| {
                        editor.attach_library(this.library.clone(), cx)
                    });
                    let id =
                        editor.update(cx, |editor, cx| editor.load_document(loaded, window, cx))?;
                    editor.update(cx, |editor, cx| {
                        editor.restore_page_views(tab.file.views.clone(), window, cx);
                        if this.active == Some(token) && this.open_errors.is_empty() {
                            editor.focus_canvas(window, cx);
                        }
                    });
                    Ok((editor, id, json))
                });
                match result {
                    Ok((editor, id, json)) => {
                        tab.saved_revision = Some(editor.read(cx).document_revision());
                        tab._subscription = Some(cx.observe(&editor, |_, _, cx| cx.notify()));
                        tab.editor = Some(editor);
                        tab.document_id = id;
                        tab.last_saved = json;
                        let file = tab.file.clone();
                        this.remember(file);
                        this.persist_session();
                    }
                    Err(error) => {
                        let title = tab.file.title.clone();
                        this.remove_tab(token, window, cx);
                        this.show_open_error(title, error.to_string(), window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn save_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(token) = self.active else {
            return;
        };
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.token == token) else {
            return;
        };
        let Some(editor) = &tab.editor else {
            return;
        };
        editor.update(cx, |editor, cx| editor.suspend(window, cx));
        tab.save_requested = true;
        self.save_tab(token, window, cx);
    }

    pub(super) fn close_tab(&mut self, token: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_tab_preview(cx);
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.token == token) else {
            return;
        };
        if let Some(editor) = &tab.editor {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
            tab.close_after_save = true;
            self.save_tab(token, window, cx);
        } else {
            self.remove_tab(token, window, cx);
        }
    }
    fn remove_tab(&mut self, token: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.active == Some(token) {
            self.select_tab(None, window, cx);
        }
        self.tabs.retain(|tab| tab.token != token);
        self.persist_session();
        cx.notify();
    }
    pub(super) fn begin_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.strip.drag.is_some() {
            self.quit_requested = false;
            return;
        }
        for editor in self.tabs.iter().filter_map(|t| t.editor.clone()) {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        self.closing = true;
        self.awaiting_library |= self.library.read(cx).busy;
        self.autosave(window, cx);
    }
    fn finish_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.library.read(cx).busy {
            return;
        }
        if self.awaiting_library {
            self.awaiting_library = false;
            if let Some(error) = self.library.read(cx).error.clone() {
                self.closing = false;
                self.quit_requested = false;
                self.error = Some(error);
                cx.notify();
                return;
            }
        }
        if self.tabs.iter().any(|t| {
            t.saving
                || t.loading
                || t.exporting
                || t.editor
                    .as_ref()
                    .is_some_and(|editor| editor.read(cx).is_exporting())
        }) {
            return;
        }
        if self
            .tabs
            .iter()
            .any(|t| t.editor.is_some() && t.error.is_some())
        {
            self.closing = false;
            self.quit_requested = false;
            return;
        }
        let dirty = self.tabs.iter().any(|tab| {
            tab.editor.as_ref().is_some_and(|editor| {
                editor
                    .read(cx)
                    .snapshot_document(&tab.document_id, cx)
                    .is_ok_and(|(json, _)| json != tab.last_saved)
            })
        });
        if !dirty {
            if cx.windows().len() > 1 && !self.quit_requested {
                self.tabs.clear();
            }
            self.persist_session();
            window.remove_window();
        }
    }
}

pub(super) fn is_internal(directory: &std::path::Path, path: &std::path::Path) -> bool {
    let path = rovar_storage::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let root = directory.join("documents");
    let root = rovar_storage::fs::canonicalize(&root).unwrap_or(root);
    path.parent() == Some(root.as_path())
        && path
            .file_stem()
            .and_then(|id| id.to_str())
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        && path.extension().is_some_and(|ext| ext == "rovar")
}

pub(super) fn recover_documents(directory: PathBuf) -> Vec<Recent> {
    let Ok(entries) = rovar_storage::fs::read_dir(directory.join("documents")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rovar") {
                return None;
            }
            let reader = rovar_format::Reader::open(&path).ok()?;
            let id = document::read_id(&reader).ok()?;
            if path.file_stem()?.to_str()? != id {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let created = metadata
                .created()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|time| time.as_millis() as u64)
                .unwrap_or(0);
            let modified = metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_secs();
            let preview = document::cache_preview(&path, &directory.join("previews"))
                .ok()
                .flatten();
            Some(Recent {
                path,
                title: String::new(),
                created,
                modified,
                views: Default::default(),
                preview,
            })
        })
        .collect()
}
