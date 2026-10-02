//! Conflict inspection with an isolated server snapshot and the live local
//! editor. Local merges use normal history/autosave; the server stays read-only.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use gpui::App;
use std::{collections::BTreeSet, path::Path};

mod resolution;
#[cfg(test)]
mod tests;
mod view;

struct Version {
    editor: Entity<Workspace>,
    pages: BTreeSet<String>,
}

pub(super) struct Panel {
    request: uuid::Uuid,
    path: PathBuf,
    title: String,
    local: Version,
    server: Option<Version>,
    object: Option<rovar_api::Object>,
    base_revision: i64,
    connection: String,
    generation: u64,
    resolving: bool,
    error: Option<String>,
    original_views: crate::editor::PageViews,
    pages: Vec<(String, String)>,
    page: usize,
    pub(super) server_visible: bool,
    view: [f32; 3],
    loading: bool,
    failed: bool,
    // Embedded media reads lazily from this file, so keep it until preview closes.
    _snapshot: Option<SnapshotFile>,
}

struct SnapshotFile(PathBuf);
impl Drop for SnapshotFile {
    fn drop(&mut self) {
        let _ = rovar_storage::fs::remove_file(&self.0);
    }
}

impl Panel {
    fn visible(&self) -> Option<&Version> {
        if self.server_visible {
            self.server.as_ref()
        } else {
            Some(&self.local)
        }
    }

    pub(super) fn editor(&self) -> Option<Entity<Workspace>> {
        let version = self.visible()?;
        version
            .pages
            .contains(&self.pages[self.page].0)
            .then(|| version.editor.clone())
    }

    fn navigate(&mut self, server: bool, page: usize, window: &mut Window, cx: &mut App) {
        if let Some(editor) = self.editor() {
            self.view = editor.read(cx).view_state();
        }
        if let Some(editor) = self.editor() {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        self.server_visible = server;
        self.page = page;
        if let Some(editor) = self.editor() {
            editor.update(cx, |editor, cx| {
                editor.preview_page(&self.pages[page].0, window, cx);
                editor.restore_view(self.view);
                cx.notify();
            });
        }
    }
}

fn version(
    loaded: crate::document::Loaded,
    window: &mut Window,
    cx: &mut App,
) -> anyhow::Result<(Version, Vec<(String, String)>)> {
    let document = crate::document::Document::decode(&loaded.json)?;
    let pages: Vec<_> = document.pages.into_iter().map(|p| (p.id, p.name)).collect();
    let editor = cx.new(|cx| Workspace::new(window, cx));
    editor.update(cx, |editor, cx| {
        editor.load_document(loaded, window, cx)?;
        editor.enable_preview(false);
        Ok::<_, anyhow::Error>(())
    })?;
    Ok((
        Version {
            editor,
            pages: pages.iter().map(|p| p.0.clone()).collect(),
        },
        pages,
    ))
}

impl Studio {
    pub(super) fn open_comparison(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.comparison.is_some() {
            self.close_comparison(window, cx);
        }
        let Some(link) = self
            .remote
            .read(cx)
            .link(path)
            .filter(|l| l.conflict)
            .cloned()
        else {
            return;
        };
        let Some(connection) = self.remote.read(cx).connection(&link.connection).cloned() else {
            return;
        };
        if !connection.authenticated {
            self.reauthenticate(connection.id, window, cx);
            return;
        }
        let Some(tab) = self.tabs.iter().find(|tab| tab.file.path == path) else {
            return;
        };
        let Some(editor) = tab.editor.clone() else {
            return;
        };
        let id = tab.document_id.clone();
        let title = tab.file.title.clone();
        if !editor.read(cx).save_ready(cx) {
            return;
        }
        editor.update(cx, |editor, cx| editor.suspend(window, cx));
        let views = editor.read(cx).page_views();
        let view = editor.read(cx).view_state();
        let document = editor
            .read(cx)
            .snapshot_document(&id, cx)
            .and_then(|(json, _)| crate::document::Document::decode(&json));
        let document = match document {
            Ok(document) => document,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let pages: Vec<_> = document.pages.into_iter().map(|p| (p.id, p.name)).collect();
        let local = Version {
            editor: editor.clone(),
            pages: pages.iter().map(|p| p.0.clone()).collect(),
        };
        editor.update(cx, |editor, cx| {
            editor.enable_preview(true);
            cx.notify();
        });
        let page = pages.iter().position(|p| p.0 == views.active).unwrap_or(0);
        local.editor.update(cx, |editor, cx| {
            editor.preview_page(&pages[page].0, window, cx);
            editor.restore_view(view);
        });
        let request = uuid::Uuid::new_v4();
        self.server_info
            .update(cx, |menu, cx| menu.close(window, cx));
        self.dismiss_tab_preview(cx);
        self.comparison = Some(Panel {
            request,
            path: path.into(),
            title,
            local,
            server: None,
            object: None,
            base_revision: link.object.revision,
            connection: connection.id.clone(),
            generation: connection.generation,
            resolving: false,
            error: None,
            original_views: views,
            pages,
            page,
            server_visible: false,
            view,
            loading: true,
            failed: false,
            _snapshot: None,
        });
        self.focus_comparison(window, cx);
        let temporary = self
            .directory
            .join("comparison-cache")
            .join(format!("{request}.rovar"));
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let client = connection.client();
                let identity: rovar_api::Identity = client.json("GET", "session", None).await?;
                anyhow::ensure!(
                    identity.server_id == connection.identity.server_id
                        && identity.user_id == connection.identity.user_id,
                    "Server account changed"
                );
                let snapshot: rovar_api::Snapshot = client
                    .json(
                        "GET",
                        &format!("spaces/{}/objects/{}", connection.space.id, link.object.id),
                        None,
                    )
                    .await?;
                anyhow::ensure!(
                    !snapshot.object.deleted
                        && snapshot.object.id == link.object.id
                        && snapshot.object.kind == rovar_api::Kind::Document,
                    "Document is unavailable"
                );
                cx.background_executor()
                    .spawn(async move {
                        let file = SnapshotFile(temporary);
                        rovar_storage::fs::create_dir_all(file.0.parent().unwrap())?;
                        rovar_storage::fs::write(&file.0, STANDARD.decode(snapshot.content)?)?;
                        let loaded = crate::document::load(&file.0)?;
                        Ok::<_, anyhow::Error>((loaded, snapshot.object, file))
                    })
                    .await
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this
                    .comparison
                    .as_ref()
                    .is_none_or(|p| p.request != request)
                {
                    return;
                }
                let result = result.and_then(|(loaded, object, file)| {
                    anyhow::ensure!(
                        this.remote
                            .read(cx)
                            .connection(&connection.id)
                            .is_some_and(
                                |c| c.generation == connection.generation && c.authenticated
                            ),
                        "Server account changed"
                    );
                    let (version, pages) = version(loaded, window, cx)?;
                    Ok((version, pages, object, file))
                });
                let panel = this.comparison.as_mut().unwrap();
                panel.loading = false;
                match result {
                    Ok((server, pages, object, file)) => {
                        for page in pages {
                            if !panel.pages.iter().any(|p| p.0 == page.0) {
                                panel.pages.push(page);
                            }
                        }
                        panel.server = Some(server);
                        panel.object = Some(object);
                        panel._snapshot = Some(file);
                        panel.navigate(panel.server_visible, panel.page, window, cx);
                    }
                    Err(error) => {
                        eprintln!("Could not compare server version: {error:#}");
                        panel.failed = true;
                    }
                }
                this.focus_comparison(window, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn focus_comparison(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.comparison.as_ref().and_then(Panel::editor) {
            editor.update(cx, |editor, cx| editor.focus_canvas(window, cx));
        } else {
            self.focus.focus(window, cx);
        }
    }

    pub(super) fn close_comparison(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.comparison.as_ref().is_some_and(|p| p.resolving) {
            return;
        }
        if let Some(panel) = self.comparison.take() {
            panel.local.editor.update(cx, |editor, cx| {
                editor.disable_preview(panel.original_views, window, cx);
            });
        }
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.focus_canvas(window, cx));
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }
}
