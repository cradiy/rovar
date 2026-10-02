use super::*;

impl Studio {
    pub(super) fn resolve_comparison(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.comparison.as_mut() else {
            return;
        };
        if panel.resolving || panel.loading || panel.object.is_none() || self.remote.read(cx).busy {
            return;
        }
        let Some(tab) = self.tabs.iter_mut().find(|t| t.file.path == panel.path) else {
            return;
        };
        tab.error = None;
        let token = tab.token;
        let request = panel.request;
        panel.resolving = true;
        panel.error = None;
        panel.local.editor.update(cx, |editor, cx| {
            editor.suspend(window, cx);
            editor.enable_preview(false);
            cx.notify();
        });
        // Finish normal local saves before either resolving or replacing the
        // file. This also includes edits merged from the server in this view.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let done = this
                    .update_in(cx, |this, window, cx| {
                        if this
                            .comparison
                            .as_ref()
                            .is_none_or(|p| p.request != request)
                        {
                            return true;
                        }
                        if let Some(error) = this
                            .tabs
                            .iter()
                            .find(|t| t.token == token)
                            .and_then(|t| t.error.clone())
                        {
                            this.comparison_resolution_error(error, cx);
                            return true;
                        }
                        this.save_tab(token, window, cx);
                        let Some(tab) = this.tabs.iter().find(|t| t.token == token) else {
                            this.comparison_resolution_error("Document is closed".into(), cx);
                            return true;
                        };
                        if let Some(error) = tab.error.clone() {
                            this.comparison_resolution_error(error, cx);
                            return true;
                        }
                        if tab.saving
                            || tab.editor.as_ref().is_none_or(|e| {
                                !e.read(cx).save_ready(cx)
                                    || tab.saved_revision != Some(e.read(cx).document_revision())
                            })
                        {
                            return false;
                        }
                        if this.remote.read(cx).busy {
                            return false;
                        }
                        let result = this.finish_comparison_resolution(token, window, cx);
                        if let Err(error) = result {
                            this.comparison_resolution_error(error.to_string(), cx);
                        }
                        true
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
            }
        })
        .detach();
        cx.notify();
    }

    fn comparison_resolution_error(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(panel) = self.comparison.as_mut() {
            panel.resolving = false;
            panel.error = Some(error);
            panel.local.editor.update(cx, |editor, cx| {
                editor.enable_preview(true);
                cx.notify();
            });
        }
        cx.notify();
    }

    fn finish_comparison_resolution(
        &mut self,
        token: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let panel = self.comparison.as_ref().unwrap();
        anyhow::ensure!(
            self.remote
                .read(cx)
                .connection(&panel.connection)
                .is_some_and(|c| c.authenticated && c.generation == panel.generation),
            crate::i18n::t("comparison-account-changed")
        );
        let server = panel.server_visible;
        let bytes = if server {
            let snapshot = panel
                ._snapshot
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Server snapshot is unavailable"))?;
            Some(rovar_storage::fs::read(&snapshot.0)?)
        } else {
            None
        };
        self.remote.update(cx, |remote, cx| {
            remote.resolve_conflict(
                &panel.path,
                panel.object.as_ref().unwrap(),
                bytes.as_deref(),
                cx,
            )
        })?;
        self.comparison.as_mut().unwrap().resolving = false;
        self.close_comparison(window, cx);
        if server {
            // Reload from the permanent file, so lazy media does not reference
            // the temporary comparison snapshot and stale undo history is gone.
            let tab = self.tabs.iter_mut().find(|t| t.token == token).unwrap();
            tab.editor = None;
            tab._subscription = None;
            tab.saved_revision = None;
            tab.file.preview = None;
            tab.save_requested = false;
            self.load_tab(token, window, cx);
        } else {
            self.remote.update(cx, |remote, cx| remote.sync(cx));
        }
        cx.notify();
        Ok(())
    }
}
