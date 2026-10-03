use super::*;
use std::path::Path;

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;

fn file_stamp(path: &Path) -> anyhow::Result<(u64, web_time::SystemTime)> {
    let metadata = rovar_storage::fs::metadata(path)?;
    Ok((metadata.len(), metadata.modified()?))
}

impl Studio {
    pub(super) fn protected_paths(&self, cx: &gpui::App) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .filter(|tab| {
                self.comparison.is_some()
                    || tab.saving
                    || tab.exporting
                    || (tab.loading && !tab.checking_remote)
                    || tab.editor.as_ref().is_some_and(|editor| {
                        let editor = editor.read(cx);
                        !editor.sync_ready(cx)
                            || tab.saved_revision != Some(editor.document_revision())
                    })
            })
            .map(|tab| tab.file.path.clone())
            .collect()
    }

    pub(super) fn reload_synced_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let protected = self.protected_paths(cx);
        for index in 0..self.tabs.len() {
            let tab = &self.tabs[index];
            if tab.editor.is_none() || tab.loading || protected.contains(&tab.file.path) {
                continue;
            }
            let Some(link) = self.remote.read(cx).link(&tab.file.path).cloned() else {
                continue;
            };
            if link.conflict || link.baseline == tab.remote_baseline {
                continue;
            }
            if link.object.deleted {
                // Keep an open editor available for export, but never recreate a tombstone.
                self.tabs[index].error = Some(t("server-document-deleted").into());
                continue;
            }
            let token = tab.token;
            let path = tab.file.path.clone();
            let previous = tab.editor.as_ref().unwrap().clone();
            let revision = previous.read(cx).document_revision();
            let generation = self
                .remote
                .read(cx)
                .connection(&link.connection)
                .map(|c| c.generation);
            // Keep the current editor usable, but coalesce reloads and protect
            // the cache from another download while it is being read.
            self.tabs[index].loading = true;
            self.tabs[index].checking_remote = false;
            let input = path.clone();
            let task = cx.background_executor().spawn(async move {
                let stamp = file_stamp(&input)?;
                let loaded = crate::document::load(&input)?;
                let json = loaded.json.clone();
                let needs_upgrade = loaded.needs_upgrade;
                let document = loaded.into_document()?;
                anyhow::ensure!(
                    file_stamp(&input)? == stamp,
                    "Document changed while reloading"
                );
                Ok::<_, anyhow::Error>((document, json, needs_upgrade, stamp))
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    let Some(index) = this.tabs.iter().position(|tab| tab.token == token) else {
                        return;
                    };
                    let tab = &mut this.tabs[index];
                    tab.loading = false;
                    cx.notify();
                    if tab.file.path != path
                        || tab.editor.as_ref() != Some(&previous)
                        || previous.read(cx).document_revision() != revision
                        || this.protected_paths(cx).contains(&path)
                    {
                        return;
                    }
                    let remote = this.remote.read(cx);
                    let current = remote.link(&path).is_some_and(|current| {
                        current.connection == link.connection
                            && current.object.id == link.object.id
                            && current.object.revision == link.object.revision
                            && current.baseline == link.baseline
                            && current.digest == link.digest
                            && !current.conflict
                            && !current.object.deleted
                    }) && remote.connection(&link.connection).map(|c| c.generation)
                        == generation;
                    if !current {
                        this.reload_synced_tabs(window, cx);
                        return;
                    }
                    let (document, json, needs_upgrade, stamp) = match result {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            this.tabs[index].error = Some(error.to_string());
                            return;
                        }
                    };
                    if !file_stamp(&path).is_ok_and(|current| current == stamp) {
                        return;
                    }
                    if json != this.tabs[index].last_saved {
                        // Capture selection, viewport and focus at publication,
                        // so navigation during the background load is preserved.
                        let view = previous.read(cx).sync_view(window, cx);
                        let restore_focus = !view.focused;
                        let previous_focus = window.focused(cx);
                        let library = this.source_library(Some(link.connection), cx);
                        let editor = cx.new(|cx| Workspace::new(window, cx));
                        let id = editor.update(cx, |editor, cx| {
                            editor.attach_library(library, cx);
                            let id = editor.load_prepared_document(document, window, cx);
                            editor.restore_sync_view(view, window, cx);
                            id
                        });
                        if restore_focus {
                            if let Some(focus) = previous_focus {
                                focus.focus(window, cx);
                            } else {
                                window.blur();
                            }
                        }
                        previous.update(cx, |editor, cx| editor.suspend(window, cx));
                        let tab = &mut this.tabs[index];
                        tab.document_id = id;
                        tab.saved_revision = Some(editor.read(cx).document_revision());
                        tab._subscription = Some(cx.observe(&editor, |_, _, cx| cx.notify()));
                        tab.editor = Some(editor);
                        tab.last_saved = json;
                    }
                    let tab = &mut this.tabs[index];
                    tab.needs_upgrade = needs_upgrade;
                    tab.remote_baseline = link.baseline;
                    tab.file.title = link.object.title;
                    tab.file.modified = link.object.modified;
                    tab.error = None;
                });
            })
            .detach();
        }
    }
}
