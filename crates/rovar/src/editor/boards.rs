use super::*;

impl Workspace {
    pub(super) fn selected_board(&self) -> Option<&Artboard> {
        self.boards.iter().find(|b| Some(b.id) == self.selected)
    }
    pub(super) fn select(&mut self, id: Option<usize>, cx: &mut Context<Self>) {
        self.image_crop = None;
        self.finish_corner_input(false, cx);
        self.corner_editor.clear_hover();
        if id.is_some_and(|id| !self.layer_editable(id)) {
            return;
        }
        if self.selection_ids() != id.into_iter().collect() {
            self.duplicate = None;
        }
        self.multi_selection.clear();
        self.seal_text_edits(cx);
        for text in &self.texts {
            text.editor.update(cx, |editor, _| editor.editing = false);
        }
        self.selected_text = None;
        self.selected_shape = None;
        self.selected_node = None;
        self.vector_edit = None;
        self.vector_hover = None;
        if self.selected != id {
            self.inspector.active_stop = self
                .boards
                .iter()
                .find(|b| Some(b.id) == id)
                .map(|b| b.gradient.stops()[0].id)
                .unwrap_or(0);
        }
        self.selected = id;
        self.inspector.invalid.fill(false);
        self.sync_fields(cx);
        cx.notify();
    }
    pub(super) fn add_artboard(&mut self, rect: Rect, cx: &mut Context<Self>) {
        self.finish_bezier(cx);
        self.draw_tool = None;
        self.seal_text_edits(cx);
        let id = self.next_id;
        self.next_id += 1;
        self.boards.push(Artboard {
            uid: uuid::Uuid::new_v4(),
            color_style: None,
            id,
            layer: Default::default(),
            name: crate::i18n::message("frame-name", &[("id", id.to_string())]),
            image_fill: Default::default(),
            rect,
            color: rgb(0xffffff),
            fill_mode: FillMode::Solid,
            gradient: Default::default(),
        });
        self.history.borrow_mut().record(
            vec![Change::Board {
                id,
                index: self.boards.len() - 1,
                value: None,
            }],
            None,
        );
        self.select(Some(id), cx);
    }
}
