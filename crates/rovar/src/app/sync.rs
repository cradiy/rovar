use super::*;

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;

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
            if link.dirty || link.conflict || link.baseline == tab.remote_baseline {
                continue;
            }
            if link.object.deleted {
                // Keep an open editor available for export, but never recreate a tombstone.
                self.tabs[index].error = Some(t("server-document-deleted").into());
                continue;
            }
            let result = crate::document::load(&tab.file.path);
            match result {
                Ok(loaded) => {
                    let json = loaded.json.clone();
                    if json != tab.last_saved {
                        let previous = tab.editor.as_ref().unwrap();
                        let view = previous.read(cx).sync_view(window, cx);
                        let restore_focus = !view.focused;
                        let previous_focus = window.focused(cx);
                        let library = self.source_library(Some(link.connection), cx);
                        let editor = cx.new(|cx| Workspace::new(window, cx));
                        let result = editor.update(cx, |editor, cx| {
                            editor.attach_library(library, cx);
                            let id = editor.load_document(loaded, window, cx)?;
                            editor.restore_sync_view(view, window, cx);
                            Ok::<_, anyhow::Error>(id)
                        });
                        if restore_focus {
                            if let Some(focus) = previous_focus {
                                focus.focus(window, cx);
                            } else {
                                window.blur();
                            }
                        }
                        match result {
                            Ok(id) => {
                                previous.update(cx, |editor, cx| editor.suspend(window, cx));
                                let tab = &mut self.tabs[index];
                                tab.document_id = id;
                                tab.saved_revision = Some(editor.read(cx).document_revision());
                                tab._subscription =
                                    Some(cx.observe(&editor, |_, _, cx| cx.notify()));
                                tab.editor = Some(editor);
                                tab.last_saved = json;
                                tab.needs_upgrade = false;
                            }
                            Err(error) => {
                                self.tabs[index].error = Some(error.to_string());
                                continue;
                            }
                        }
                    }
                    let tab = &mut self.tabs[index];
                    tab.remote_baseline = link.baseline;
                    tab.file.title = link.object.title;
                    tab.file.modified = link.object.modified;
                    tab.error = None;
                }
                Err(error) => self.tabs[index].error = Some(error.to_string()),
            }
        }
    }
}
