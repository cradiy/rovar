use super::*;
use crate::document::{AssetSource, AssetUse, Document, Loaded, Page};
#[cfg(test)]
mod tests;

pub(crate) struct Transfer {
    loaded: Loaded,
    history: SharedHistory,
    view: [f32; 3],
    page_views: pages::Views,
    page_states: Vec<pages::PageState>,
    selection: std::collections::BTreeSet<usize>,
    tool: Option<DrawTool>,
    vector_edit: Option<usize>,
    selected_node: Option<(usize, usize)>,
    videos: std::collections::HashMap<usize, media::VideoRuntime>,
    sidebar: layers::Sidebar,
    panels: panels::Panels,
    hand: bool,
}

impl Workspace {
    pub(crate) fn can_transfer(&self) -> bool {
        !self.assets.inserting
            && !self.export.busy
            && !self.media_loading
            && self.image_fill_loading.is_none()
            && self.video_loading.is_empty()
    }
    pub(crate) fn transfer(&self, id: &str, cx: &gpui::App) -> anyhow::Result<Transfer> {
        let (json, _) = self.snapshot_document(id, cx)?;
        let mut assets = std::collections::BTreeMap::new();
        let (current, _) = self.snapshot_page(cx);
        for page in std::iter::once(&current).chain(
            self.pages
                .entries
                .iter()
                .filter(|p| p.page.id != self.pages.active)
                .map(|p| &p.page),
        ) {
            for asset in page
                .boards
                .iter()
                .filter_map(|b| b.image_fill.asset.as_ref())
                .chain(page.shapes.iter().flat_map(|s| {
                    [s.media.as_ref(), s.image_fill.asset.as_ref()]
                        .into_iter()
                        .flatten()
                }))
            {
                assets.insert(asset.hash.clone(), asset.clone());
            }
        }
        Ok(Transfer {
            loaded: Loaded {
                json,
                assets,
                needs_upgrade: false,
            },
            history: self.history.clone(),
            view: self.view_state(),
            page_views: self.page_views(),
            page_states: self.pages.entries.clone(),
            selection: self.selection_ids(),
            tool: self.draw_tool,
            vector_edit: self.vector_edit,
            selected_node: self.selected_node,
            videos: self.videos.clone(),
            sidebar: self.sidebar.clone(),
            panels: self.panels.clone(),
            hand: self.toolbar.hand,
        })
    }
    pub(crate) fn receive_transfer(
        &mut self,
        transfer: Transfer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.history = transfer.history;
        self.load_document(transfer.loaded, window, cx)?;
        for state in &mut self.pages.entries {
            if let Some(old) = transfer
                .page_states
                .iter()
                .find(|old| old.page.id == state.page.id)
            {
                state.selection = old.selection.clone();
                state.folded = old.folded.clone();
            }
        }
        self.restore_page_views(transfer.page_views, window, cx);
        self.restore_view(transfer.view);
        self.set_selection(transfer.selection, cx);
        self.draw_tool = transfer.tool;
        self.vector_edit = transfer.vector_edit;
        self.selected_node = transfer.selected_node;
        self.videos = transfer.videos;
        self.sidebar = transfer.sidebar;
        self.sidebar.renaming = None;
        self.panels = transfer.panels;
        self.toolbar.hand = transfer.hand;
        self.focus_canvas(window, cx);
        Ok(())
    }
    pub(crate) fn focus_canvas(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
    }
    pub(crate) fn dismiss_menus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tool_menus(window, cx);
    }
    pub(crate) fn can_undo_redo(&self, redo: bool) -> bool {
        if let Some(draft) = &self.bezier_draft {
            return draft.can_replay(redo);
        }
        if redo {
            self.history.borrow().can_redo()
        } else {
            self.history.borrow().can_undo()
        }
    }
    pub(crate) fn undo_redo(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.replay_history(redo, window, cx);
    }
    pub(crate) fn refresh_language(&mut self, old_mixed: &str, cx: &mut Context<Self>) {
        self.refresh_assets_language(cx);
        self.finish_inspector_input(cx);
        for field in &self.fields {
            field.update(cx, |input, cx| {
                input.set_placeholder(if input.value().is_empty() {
                    crate::i18n::t("mixed")
                } else {
                    ""
                });
                cx.notify();
            });
        }
        self.font_picker
            .update(cx, |picker, cx| picker.refresh_language(old_mixed, cx));
        if self
            .selected_shape()
            .is_some_and(|s| s.fill_mode == FillMode::Image)
            || (self.selected_shape.is_none()
                && self
                    .selected_board()
                    .is_some_and(|b| b.fill_mode == FillMode::Image))
        {
            self.sync_field(5, cx);
        }
        self.scene.update(cx, |_, cx| cx.notify());
        cx.notify();
    }
    pub(crate) fn snapshot_document(
        &self,
        id: &str,
        cx: &gpui::App,
    ) -> anyhow::Result<(Vec<u8>, Vec<AssetSource>)> {
        #[cfg(test)]
        self.snapshot_count.set(self.snapshot_count.get() + 1);
        let (current, _) = self.snapshot_page(cx);
        let pages: Vec<_> = self
            .pages
            .entries
            .iter()
            .map(|state| {
                if state.page.id == self.pages.active {
                    current.clone()
                } else {
                    state.page.clone()
                }
            })
            .collect();
        let document = Document {
            id: id.into(),
            pages,
        };
        let mut sources = std::collections::BTreeMap::new();
        for page in &document.pages {
            for asset in page
                .boards
                .iter()
                .filter_map(|b| b.image_fill.asset.as_ref())
                .chain(page.shapes.iter().flat_map(|s| {
                    [s.media.as_ref(), s.image_fill.asset.as_ref()]
                        .into_iter()
                        .flatten()
                }))
            {
                sources
                    .entry(asset.hash.clone())
                    .or_insert_with(|| AssetSource {
                        hash: asset.hash.clone(),
                        name: asset.name(),
                        path: asset.source.clone(),
                        size: [asset.width, asset.height],
                    });
            }
        }
        Ok((
            serde_json::to_vec(&document)?,
            sources.into_values().collect(),
        ))
    }

    pub(crate) fn load_document(
        &mut self,
        loaded: Loaded,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<String> {
        let document = loaded.into_document()?;
        self.suspend(window, cx);
        self.history.borrow_mut().mark_changed();
        self.pages.active = document.pages[0].id.clone();
        self.pages.entries = document
            .pages
            .into_iter()
            .map(pages::PageState::new)
            .collect();
        self.history
            .borrow_mut()
            .set_page(self.pages.active.clone());
        self.load_page(self.pages.current().page.clone(), window, cx);
        Ok(document.id)
    }

    pub(super) fn snapshot_page(&self, cx: &gpui::App) -> (Page, Vec<AssetSource>) {
        let mut sources = Vec::new();
        let mut assets = Vec::new();
        let mut add = |object, fill, asset: &Option<std::sync::Arc<crate::media::MediaAsset>>| {
            if let Some(asset) = asset {
                assets.push(AssetUse {
                    object,
                    fill,
                    hash: asset.hash.clone(),
                });
                sources.push(AssetSource {
                    hash: asset.hash.clone(),
                    name: asset.name(),
                    path: asset.source.clone(),
                    size: [asset.width, asset.height],
                });
            }
        };
        for board in &self.boards {
            add(board.id, true, &board.image_fill.asset);
        }
        for shape in &self.shapes {
            add(shape.id, false, &shape.media);
            add(shape.id, true, &shape.image_fill.asset);
        }
        let page = Page {
            id: self.pages.active.clone(),
            name: self.pages.current().page.name.clone(),
            boards: self.boards.clone(),
            shapes: self.shapes.clone(),
            texts: self
                .texts
                .iter()
                .map(|t| {
                    t.editor
                        .read(cx)
                        .document_text(t.id, t.board, t.rect, t.layer)
                })
                .collect(),
            hierarchy: self.hierarchy.clone(),
            next_id: self.next_id,
            assets,
        };
        (page, sources)
    }

    pub(super) fn load_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.boards = page.boards;
        self.shapes = page.shapes;
        self.hierarchy = page.hierarchy;
        self.next_id = page.next_id;
        self.texts.clear();
        for text in page.texts {
            let mut item = self.make_text(text.id, text.board, text.rect, window, cx);
            item.layer = text.layer;
            item.editor.update(cx, |editor, cx| {
                editor.load_document_text(text);
                cx.notify();
            });
            self.texts.push(item);
        }
        self.select(None, cx);
        cx.notify();
    }

    pub(crate) fn suspend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.suspend_assets();
        self.cancel_gesture(window, cx);
        self.finish_inspector_input(cx);
        self.seal_text_edits(cx);
        self.finish_bezier(cx);
        self.close_tool_menus(window, cx);
        for popover in &self.paint_popovers {
            popover.update(cx, |p, cx| p.close(window, cx));
        }
        self.pause_videos(cx);
    }
    fn finish_inspector_input(&mut self, cx: &mut Context<Self>) {
        self.history.borrow_mut().break_group();
        for index in 0..self.fields.len() {
            if self.invalid[index] {
                self.sync_input_field(index, cx);
            }
        }
    }
    pub(crate) fn save_ready(&self, cx: &gpui::App) -> bool {
        self.gesture.is_none()
            && self
                .texts
                .iter()
                .all(|text| !text.editor.read(cx).is_composing())
    }
    pub(crate) fn view_state(&self) -> [f32; 3] {
        [self.view.pan.x, self.view.pan.y, self.view.zoom]
    }
    pub(crate) fn document_revision(&self) -> u64 {
        self.history.borrow().revision()
    }
    pub(crate) fn restore_view(&mut self, view: [f32; 3]) {
        if view.iter().all(|v| v.is_finite()) {
            self.view.pan = point(view[0], view[1]);
            self.view.zoom = view[2].clamp(0.1, 256.);
        }
    }
}
