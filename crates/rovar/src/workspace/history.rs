use super::*;
use crate::history::{Change, Group, SavedText};
use gpui::EntityInputHandler;

impl Workspace {
    pub(super) fn seal_text_edits(&mut self, cx: &mut Context<Self>) {
        for text in &self.texts {
            text.editor
                .update(cx, |editor, _| editor.finish_composition());
        }
        self.history.borrow_mut().break_group();
    }

    pub(super) fn edit_board<R>(
        &mut self,
        group: Option<Group>,
        edit: impl FnOnce(&mut Artboard) -> R,
    ) -> Option<R> {
        let index = self
            .boards
            .iter()
            .position(|b| Some(b.id) == self.selected)?;
        let before = self.boards[index].clone();
        let result = edit(&mut self.boards[index]);
        let board = &mut self.boards[index];
        if board.color != before.color
            || board.fill_mode != before.fill_mode
            || board.gradient != before.gradient
        {
            board.color_style = None;
        }
        if self.boards[index] != before {
            let mut changes: Vec<_> = self
                .fix_layout_size(before.id, before.rect, self.boards[index].rect)
                .into_iter()
                .collect();
            changes.push(Change::Board {
                id: before.id,
                index,
                value: Some(before),
            });
            self.history.borrow_mut().record(changes, group);
        }
        Some(result)
    }

    pub(super) fn saved_text(&self, index: usize, cx: &gpui::App) -> SavedText {
        let text = &self.texts[index];
        SavedText {
            id: text.id,
            layer: text.layer,
            board: text.board,
            rect: text.rect,
            text: text.editor.read(cx).snapshot(),
        }
    }

    pub(super) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        self.seal_text_edits(cx);
        let hierarchy_before = self.snapshot_hierarchy();
        let ids = self.descendants(&self.selection_ids());
        let fallback = self.selected.filter(|id| !ids.contains(id));
        let included = |id, parent: Option<usize>| {
            ids.contains(&id) || parent.is_some_and(|p| ids.contains(&p))
        };
        let mut changes = Vec::new();
        for index in (0..self.texts.len()).rev() {
            let text = &self.texts[index];
            if included(text.id, text.board) {
                changes.push(Change::TextBox {
                    id: text.id,
                    index,
                    value: Some(self.saved_text(index, cx)),
                });
                self.texts.remove(index);
            }
        }
        for index in (0..self.shapes.len()).rev() {
            let shape = &self.shapes[index];
            if included(shape.id, shape.board) {
                let shape = self.shapes.remove(index);
                self.shape_paths.borrow_mut().remove(&shape.id);
                changes.push(Change::Shape {
                    id: shape.id,
                    index,
                    value: Some(shape),
                });
            }
        }
        for index in (0..self.boards.len()).rev() {
            if ids.contains(&self.boards[index].id) {
                let board = self.boards.remove(index);
                changes.push(Change::Board {
                    id: board.id,
                    index,
                    value: Some(board),
                });
            }
        }
        for id in &ids {
            self.hierarchy.groups.remove(id);
            self.hierarchy.parents.remove(id);
        }
        self.prune_hierarchy();
        if let Change::Hierarchy { ref value, .. } = hierarchy_before
            && *value != self.hierarchy
        {
            changes.insert(0, hierarchy_before);
        }
        if !changes.is_empty() {
            self.history.borrow_mut().record(changes, None);
        }
        self.select(fallback, cx);
    }

    pub(super) fn finish_gesture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.snapping.clear();
        if let Some(gesture) = self.gesture.take() {
            let change = match gesture.kind {
                GestureKind::Rotate { id, original } => {
                    self.layer_info(id).and_then(|(mut layer, _)| {
                        if layer.rotation == original {
                            return None;
                        }
                        layer.rotation = original;
                        Some(Change::Layer { id, value: layer })
                    })
                }
                GestureKind::LayerSort => {
                    self.finish_layer_sort(cx);
                    None
                }
                GestureKind::Marquee => {
                    self.marquee = None;
                    None
                }
                GestureKind::SelectionMove => {
                    self.finish_selection_move(cx);
                    None
                }
                GestureKind::CornerRadius => {
                    self.commit_corner_radius(window, cx);
                    None
                }
                GestureKind::ImageCrop { .. } => None,
                GestureKind::SelectionResize { .. } => {
                    self.finish_selection_resize(true, cx);
                    None
                }
                GestureKind::Spacing { .. } => {
                    self.finish_spacing(true, cx);
                    None
                }
                GestureKind::MultiProperty { .. } => {
                    self.finish_multi_property(cx);
                    None
                }
                GestureKind::Property { index, original } => {
                    let changed = self
                        .field_value(index, cx)
                        .and_then(|s| s.parse::<f32>().ok())
                        .is_some_and(|value| value != original);
                    self.finish_property_scrub(changed, cx);
                    None
                }
                GestureKind::LayoutProperty { index, original } => {
                    let changed = self
                        .layout_number_value(index)
                        .is_some_and(|v| v != original);
                    self.finish_layout_scrub(changed, cx);
                    None
                }
                GestureKind::Draw => {
                    self.finish_drawing(window, cx);
                    None
                }
                GestureKind::BezierPlace => None,
                GestureKind::LineEnd { id, .. } | GestureKind::BezierEdit { id, .. } => {
                    self.path_before.take().and_then(|before| {
                        self.shapes
                            .iter()
                            .enumerate()
                            .find(|(_, s)| s.id == id && **s != before)
                            .map(|(index, _)| Change::Shape {
                                id,
                                index,
                                value: Some(before),
                            })
                    })
                }
                GestureKind::Text { id, original, .. } => self
                    .texts
                    .iter()
                    .find(|t| t.id == id)
                    .filter(|t| t.rect != original)
                    .map(|text| Change::TextRect {
                        id,
                        board: text.board,
                        value: original,
                    }),
                GestureKind::Move { id, original } | GestureKind::Resize { id, original, .. } => {
                    self.boards
                        .iter()
                        .enumerate()
                        .find(|(_, b)| b.id == id)
                        .filter(|(_, b)| b.rect != original)
                        .map(|(index, board)| {
                            let mut before = board.clone();
                            before.rect = original;
                            Change::Board {
                                id,
                                index,
                                value: Some(before),
                            }
                        })
                }
                GestureKind::Shape { id, original, .. } => self
                    .shapes
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.id == id)
                    .filter(|(_, s)| s.rect != original)
                    .map(|(index, shape)| {
                        let mut before = shape.clone();
                        before.rect = original;
                        Change::Shape {
                            id,
                            index,
                            value: Some(before),
                        }
                    }),
                GestureKind::Pan { .. }
                | GestureKind::Panel { .. }
                | GestureKind::ColorStyleProperty { .. } => None,
                GestureKind::ColorStyleStop { .. } => None,
                GestureKind::GradientSeam { style, original } => {
                    if !style {
                        let changed = self
                            .fill_state(cx)
                            .is_some_and(|(_, g)| g.seam_width != original);
                        self.finish_property_scrub(changed, cx);
                    }
                    None
                }
                GestureKind::GradientMidpoint {
                    style,
                    id,
                    original,
                    ..
                } => {
                    if !style {
                        let changed = self
                            .fill_state(cx)
                            .and_then(|(_, g)| g.stop(id).map(|s| s.midpoint))
                            != Some(original);
                        self.finish_property_scrub(changed, cx);
                    }
                    None
                }
            };
            let moving = match gesture.kind {
                GestureKind::Shape {
                    id, handle: None, ..
                }
                | GestureKind::Text {
                    id, handle: None, ..
                }
                | GestureKind::Move { id, .. } => Some(id),
                _ => None,
            };
            let layout_item =
                moving.filter(|id| change.is_some() && self.is_auto_layout_child(*id));
            let layout_change = layout_item.and_then(|id| self.finish_layout_item_move(id));
            if layout_item.is_none() {
                self.reparent_moved(gesture.kind, cx);
            } else {
                self.sync_fields(cx);
            }
            if let Some(change) = change {
                let resized = match &change {
                    Change::Board {
                        id, value: Some(b), ..
                    } => Some((*id, b.rect)),
                    Change::Shape {
                        id, value: Some(s), ..
                    } => Some((*id, s.rect)),
                    Change::TextRect { id, value, .. } => Some((*id, *value)),
                    _ => None,
                }
                .and_then(|(id, before)| {
                    self.object_rect(id)
                        .and_then(|(_, after)| self.fix_layout_size(id, before, after))
                });
                let mut changes = vec![change];
                if let Some(before) = resized {
                    changes.insert(0, before);
                }
                if let Some(before) = layout_change {
                    changes.insert(0, before);
                }
                if matches!(gesture.kind, GestureKind::BezierEdit { .. }) {
                    changes.insert(
                        0,
                        Change::NodeSelection {
                            value: self.selected_node,
                        },
                    );
                }
                self.history.borrow_mut().record(changes, None);
            }
        }
        window.release_pointer();
        cx.notify();
    }

    pub(super) fn history_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if !(modifiers.control || modifiers.platform) || modifiers.alt {
            return;
        }
        let redo = match event.keystroke.key.as_str() {
            "z" => modifiers.shift,
            "y" => true,
            _ => return,
        };
        // A pending rename must not undo an unrelated document operation.
        if self.rename_input.focus_handle(cx).is_focused(window) {
            if self.rename_input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            }) {
                return;
            }
            if !redo {
                self.finish_rename(false, window, cx);
            }
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // Composition keys belong to the IME, including property input composition.
        if self.texts.iter().any(|t| {
            let editor = t.editor.read(cx);
            editor.focus.is_focused(window) && editor.is_composing()
        }) {
            return;
        }
        for field in &self.fields {
            if field.focus_handle(cx).is_focused(window)
                && field.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
            {
                return;
            }
        }
        self.replay_history(redo, window, cx);
        cx.stop_propagation();
        window.prevent_default();
    }

    pub(super) fn replay_history(
        &mut self,
        redo: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bezier_draft.is_some() {
            self.bezier_history(redo, window, cx);
            return;
        }
        if self.gesture.is_some() {
            self.cancel_gesture(window, cx);
            return;
        }
        self.duplicate = None;
        self.image_crop = None;
        self.finish_spacing_input(false, cx);
        self.finish_corner_input(false, cx);
        self.seal_text_edits(cx);
        let page = self.history.borrow().replay_page(redo).map(str::to_owned);
        let changes = self.history.borrow_mut().take(redo);
        let Some(changes) = changes else {
            return;
        };
        if let [Change::Pages { value }] = changes.as_slice() {
            let inverse = self.replay_page_edit(value.clone(), window, cx);
            self.history
                .borrow_mut()
                .finish_replay(vec![Change::Pages { value: inverse }], redo);
            cx.notify();
            return;
        }
        if let Some(page) = page {
            self.switch_page(&page, window, cx);
        }
        let replay_ids: std::collections::BTreeSet<_> = changes
            .iter()
            .filter_map(|change| match change {
                Change::Layer { id, .. }
                | Change::Board { id, .. }
                | Change::TextBox { id, .. }
                | Change::Text { id, .. }
                | Change::Shape { id, .. }
                | Change::TextRect { id, .. } => Some(*id),
                Change::Hierarchy { .. } | Change::NodeSelection { .. } | Change::Pages { .. } => {
                    None
                }
            })
            .collect();
        let previous_selection = self.selection_ids();
        let previous_vector = self.vector_edit;
        let mut restored_selection = None;
        let mut restored_node = None;
        self.multi_selection.clear();
        self.draw_tool = None;
        let old_component = (self.selected_text.is_some(), self.selected_shape.is_some());
        let was_text_focus = self
            .texts
            .iter()
            .any(|t| t.editor.read(cx).focus.is_focused(window));
        let board_target = changes.iter().find_map(|change| match change {
            Change::Board { id, .. } => Some(*id),
            _ => None,
        });
        let mut inverse = Vec::new();
        let mut target = None;
        for change in changes.into_iter().rev() {
            match change {
                Change::Pages { .. } => unreachable!("page operations are replayed atomically"),
                Change::NodeSelection { value } => {
                    inverse.push(Change::NodeSelection {
                        value: self.selected_node,
                    });
                    restored_node = Some(value);
                }
                Change::Hierarchy { value, selection } => {
                    inverse.push(Change::Hierarchy {
                        value: std::mem::replace(&mut self.hierarchy, value),
                        selection: previous_selection.clone(),
                    });
                    restored_selection = Some(selection);
                }
                Change::Layer { id, value } => {
                    if let Some(layer) = self.layer_state_mut(id) {
                        inverse.push(Change::Layer { id, value: *layer });
                        *layer = value;
                    }
                    target = Some(id);
                }
                Change::Board { id, index, value } => {
                    let current = self
                        .boards
                        .iter()
                        .position(|b| b.id == id)
                        .map(|i| self.boards.remove(i));
                    inverse.push(Change::Board {
                        id,
                        index,
                        value: current,
                    });
                    if let Some(board) = value {
                        self.boards.insert(index.min(self.boards.len()), board);
                    }
                    target = Some(id);
                }
                Change::TextBox { id, index, value } => {
                    let current = self.texts.iter().position(|t| t.id == id).map(|i| {
                        let saved = self.saved_text(i, cx);
                        self.texts.remove(i);
                        saved
                    });
                    inverse.push(Change::TextBox {
                        id,
                        index,
                        value: current,
                    });
                    if let Some(saved) = value {
                        let mut text =
                            self.make_text(saved.id, saved.board, saved.rect, window, cx);
                        text.layer = saved.layer;
                        text.editor.update(cx, |editor, _| {
                            editor.restore(saved.text);
                            editor.editing = false;
                        });
                        self.texts.insert(index.min(self.texts.len()), text);
                    }
                    target = Some(id);
                }
                Change::Text { id, value } => {
                    if let Some(text) = self.texts.iter().find(|t| t.id == id) {
                        inverse.push(Change::Text {
                            id,
                            value: text.editor.read(cx).snapshot(),
                        });
                        text.editor.update(cx, |editor, _| editor.restore(value));
                    }
                    target = Some(id);
                }
                Change::Shape { id, index, value } => {
                    let current = self
                        .shapes
                        .iter()
                        .position(|s| s.id == id)
                        .map(|i| self.shapes.remove(i));
                    inverse.push(Change::Shape {
                        id,
                        index,
                        value: current,
                    });
                    if let Some(shape) = value {
                        self.shapes.insert(index.min(self.shapes.len()), shape);
                    } else {
                        self.shape_paths.borrow_mut().remove(&id);
                    }
                    target = Some(id);
                }
                Change::TextRect { id, board, value } => {
                    if let Some(text) = self.texts.iter_mut().find(|t| t.id == id) {
                        inverse.push(Change::TextRect {
                            id,
                            board: text.board,
                            value: text.rect,
                        });
                        text.rect = value;
                        text.board = board;
                    }
                    target = Some(id);
                }
            }
        }
        self.history.borrow_mut().finish_replay(inverse, redo);
        if board_target.is_some() {
            target = board_target;
        }
        let blocked_target = target
            .and_then(|id| self.layer_info(id))
            .is_some_and(|(own, parent)| !self.effective_layer(own, parent).editable());
        target = target.filter(|id| self.layer_editable(*id));
        let text_id = target.filter(|id| self.texts.iter().any(|t| t.id == *id));
        let shape_id = target.filter(|id| self.shapes.iter().any(|s| s.id == *id));
        let board_id = target.filter(|id| self.boards.iter().any(|b| b.id == *id));
        let removed = text_id.is_none() && shape_id.is_none() && board_id.is_none();
        // Keep inspector focus only while its component type remains visible.
        self.selected_text = text_id;
        self.selected_shape = shape_id;
        if self
            .selected_shape()
            .is_some_and(|s| s.kind.is_path() && !s.can_fill())
        {
            self.stroke_editing = true;
        }
        self.selected = if let Some(text) = self.selected_text() {
            text.board
        } else if let Some(shape) = self.selected_shape() {
            shape.board
        } else {
            board_id.or_else(|| {
                if blocked_target {
                    return None;
                }
                self.selected.filter(|id| {
                    self.boards
                        .iter()
                        .any(|b| b.id == *id && b.layer.editable())
                })
            })
        };
        for text in &self.texts {
            text.editor.update(cx, |editor, _| {
                editor.editing = Some(text.id) == text_id && (editor.editing || was_text_focus);
            });
        }
        if was_text_focus {
            if let Some(text) = self.selected_text() {
                text.editor.read(cx).focus.clone().focus(window, cx);
            } else {
                self.focus.focus(window, cx);
            }
        } else if removed
            || (self.selected.is_none() && text_id.is_none() && shape_id.is_none())
            || old_component != (text_id.is_some(), shape_id.is_some())
            || (shape_id.is_some()
                && self.fields.iter().enumerate().any(|(index, field)| {
                    field.focus_handle(cx).is_focused(window)
                        && !self.shape_field_visible(index % PROPERTY_COUNT)
                }))
        {
            self.focus.focus(window, cx);
        }
        let gradient = self
            .selected_text()
            .map(|t| &t.editor.read(cx).effective_style().gradient)
            .or_else(|| {
                self.selected_shape()
                    .map(|s| s.paint_gradient(self.stroke_editing))
            })
            .or_else(|| self.selected_board().map(|b| &b.gradient));
        self.active_stop = gradient.map_or(0, |gradient| {
            if gradient.stop(self.active_stop).is_some() {
                self.active_stop
            } else {
                gradient.stops()[0].id
            }
        });
        if let Some(ids) = restored_selection {
            self.set_selection(ids, cx);
        } else if replay_ids.len() > 1
            || replay_ids
                .iter()
                .any(|id| self.hierarchy.groups.contains_key(id))
        {
            self.set_selection(replay_ids, cx);
        }
        self.invalid.fill(false);
        self.vector_edit = previous_vector
            .filter(|id| self.selected_shape == Some(*id) && self.layer_editable(*id));
        self.selected_node = restored_node.unwrap_or(None).filter(|(id, index)| {
            self.selected_shape == Some(*id)
                && self
                    .shapes
                    .iter()
                    .any(|s| s.id == *id && *index < s.editable_nodes().len())
        });
        self.sync_fields(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
