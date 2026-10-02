use super::*;
use crate::document;

impl Studio {
    pub(super) fn export_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.iter().find(|tab| Some(tab.token) == self.active) else {
            return;
        };
        if tab.editor.is_none() || tab.exporting {
            return;
        }
        let token = tab.token;
        let directory = crate::platform::export_directory();
        let dialog = crate::platform::prompt_for_new_path(
            cx,
            &directory,
            Some(&format!("{}.rovar", tab.file.title)),
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = dialog.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(mut path))) => {
                    if path.extension().is_none() {
                        path.set_extension("rovar");
                    }
                    this.export_to(token, path, window, cx);
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

    pub(super) fn export_to(
        &mut self,
        token: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = rovar_storage::fs::canonicalize(&path).unwrap_or_else(|_| {
            path.parent()
                .and_then(|parent| rovar_storage::fs::canonicalize(parent).ok())
                .zip(path.file_name())
                .map_or_else(|| path.clone(), |(parent, name)| parent.join(name))
        });
        let root = rovar_storage::fs::canonicalize(&self.directory)
            .unwrap_or_else(|_| self.directory.clone());
        if path.starts_with(&root) || files::is_internal(&self.directory, &path) {
            self.error = Some(t("export-outside-workspace").into());
            cx.notify();
            return;
        }
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.token == token) else {
            return;
        };
        if tab.exporting {
            return;
        }
        let Some(editor) = &tab.editor else {
            return;
        };
        editor.update(cx, |editor, cx| editor.suspend(window, cx));
        let snapshot = editor.read(cx).snapshot_document(&tab.document_id, cx);
        let (json, assets) = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        tab.exporting = true;
        self.save_tab(token, window, cx);
        cx.notify();
        let text_system = cx.text_system().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::render::raster::prepare_preview().await;
                    document::save_as(&path, &json, &assets, &text_system)?;
                    crate::platform::download(&path)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(tab) = this.tabs.iter_mut().find(|tab| tab.token == token) else {
                    return;
                };
                tab.exporting = false;
                match result {
                    Ok(()) => {
                        if tab.close_after_save {
                            this.save_tab(token, window, cx);
                        }
                        if this.closing {
                            this.autosave(window, cx);
                        }
                    }
                    Err(error) => {
                        tab.close_after_save = false;
                        this.closing = false;
                        this.error = Some(crate::i18n::message(
                            "export-failed",
                            &[("error", error.to_string())],
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
