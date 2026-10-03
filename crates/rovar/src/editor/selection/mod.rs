use super::*;
use std::collections::BTreeSet;
mod clipboard;
mod labels;
mod picking;
mod properties;
mod resize;
pub(super) use resize::Resize;

pub(super) struct Duplicate {
    pub ids: BTreeSet<usize>,
    pub source_origin: Point<f32>,
}

#[derive(Clone)]
pub(super) struct Marquee {
    pub(super) start: Point<f32>,
    pub rect: Rect,
    pub initial: BTreeSet<usize>,
}

impl Workspace {
    pub(super) fn selection_ids(&self) -> BTreeSet<usize> {
        if !self.multi_selection.is_empty() {
            self.multi_selection.clone()
        } else {
            self.selected_text
                .or(self.selected_shape)
                .or(self.selected)
                .into_iter()
                .collect()
        }
    }

    pub(super) fn is_selected(&self, id: usize) -> bool {
        if self.multi_selection.is_empty() {
            self.selected_text.or(self.selected_shape).or(self.selected) == Some(id)
        } else {
            self.multi_selection.contains(&id)
        }
    }

    pub(super) fn set_selection(&mut self, mut ids: BTreeSet<usize>, cx: &mut Context<Self>) {
        ids.retain(|id| self.layer_editable(*id));
        let parents = ids.clone();
        ids.retain(|id| !self.ancestors(*id).iter().any(|p| parents.contains(p)));
        if ids == self.selection_ids() {
            return;
        }
        self.select(None, cx);
        if ids.len() == 1 && !self.hierarchy.groups.contains_key(ids.first().unwrap()) {
            let id = *ids.first().unwrap();
            if self.texts.iter().any(|t| t.id == id) {
                self.select_text(id, cx);
            } else if self.shapes.iter().any(|s| s.id == id) {
                self.select_shape(id, cx);
            } else {
                self.select(Some(id), cx);
            }
        } else {
            self.multi_selection = ids;
            self.sync_fields(cx);
            cx.notify();
        }
    }

    pub(super) fn toggle_selection(&mut self, id: usize, cx: &mut Context<Self>) {
        let mut ids = self.selection_ids();
        if !ids.remove(&id) {
            ids.insert(id);
        }
        self.set_selection(ids, cx);
    }

    pub(super) fn object_rect(&self, id: usize) -> Option<(Option<usize>, Rect)> {
        if let Some(group) = self.hierarchy.groups.get(&id)
            && let Some(layout) = self.hierarchy.layouts.get(&id)
        {
            return Some((group.board, layout.frame));
        }
        self.boards
            .iter()
            .find(|b| b.id == id)
            .map(|b| (None, b.rect))
            .or_else(|| {
                self.shapes
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| (s.board, s.rect))
            })
            .or_else(|| {
                self.texts
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| (t.board, t.rect))
            })
    }

    pub(super) fn world_rect(&self, id: usize) -> Option<Rect> {
        if self.hierarchy.groups.contains_key(&id) {
            return self.group_bounds(id);
        }
        self.object_rect(id).map(|(parent, mut rect)| {
            let origin = self.parent_origin(parent);
            rect.x += origin.x;
            rect.y += origin.y;
            rect
        })
    }

    pub(super) fn set_object_rect(&mut self, id: usize, parent: Option<usize>, rect: Rect) {
        self.set_object_geometry(id, parent, rect);
        self.refresh_constraints(id);
    }

    pub(super) fn set_object_geometry(&mut self, id: usize, parent: Option<usize>, rect: Rect) {
        if self.hierarchy.groups.contains_key(&id)
            && let Some(layout) = self.hierarchy.layouts.get(&id)
        {
            let delta = point(rect.x - layout.frame.x, rect.y - layout.frame.y);
            let ids = self.descendants(&std::collections::BTreeSet::from([id]));
            let moving: Vec<_> = ids
                .iter()
                .copied()
                .filter(|child| {
                    *child != id
                        && !self
                            .ancestors(*child)
                            .iter()
                            .any(|p| ids.contains(p) && self.boards.iter().any(|b| b.id == *p))
                })
                .collect();
            for child in moving {
                if let Some(frame) = self
                    .hierarchy
                    .layouts
                    .get_mut(&child)
                    .filter(|_| self.hierarchy.groups.contains_key(&child))
                {
                    frame.frame.x += delta.x;
                    frame.frame.y += delta.y;
                } else if let Some((board, mut child_rect)) = self.object_rect(child) {
                    child_rect.x += delta.x;
                    child_rect.y += delta.y;
                    self.set_object_geometry(child, board, child_rect);
                }
            }
            self.hierarchy.layouts.get_mut(&id).unwrap().frame = rect;
            return;
        }
        if let Some(b) = self.boards.iter_mut().find(|b| b.id == id) {
            b.rect = rect;
        } else if let Some(s) = self.shapes.iter_mut().find(|s| s.id == id) {
            s.board = parent;
            s.rect = rect;
        } else if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
            t.board = parent;
            t.rect = rect;
        }
    }

    pub(super) fn before_geometry(&self) -> Vec<Change> {
        let mut ids = self.operation_ids();
        for id in self.selection_ids().into_iter().filter(|id| {
            self.hierarchy.layouts.contains_key(id) && self.hierarchy.groups.contains_key(id)
        }) {
            ids.extend(
                self.descendants(&std::collections::BTreeSet::from([id]))
                    .into_iter()
                    .filter(|child| {
                        !self.ancestors(*child).iter().any(|p| {
                            self.boards.iter().any(|b| b.id == *p)
                                && self.ancestors(*p).contains(&id)
                        })
                    }),
            );
        }
        self.geometry_changes(ids)
    }

    fn geometry_changes(&self, ids: BTreeSet<usize>) -> Vec<Change> {
        ids.into_iter()
            .filter_map(|id| {
                if let Some((index, b)) = self.boards.iter().enumerate().find(|(_, b)| b.id == id) {
                    Some(Change::Board {
                        id,
                        index,
                        value: Some(b.clone()),
                    })
                } else if let Some((index, s)) =
                    self.shapes.iter().enumerate().find(|(_, s)| s.id == id)
                {
                    Some(Change::Shape {
                        id,
                        index,
                        value: Some(s.clone()),
                    })
                } else {
                    self.texts
                        .iter()
                        .find(|t| t.id == id)
                        .map(|t| Change::TextRect {
                            id,
                            board: t.board,
                            value: t.rect,
                        })
                }
            })
            .chain(
                (!self.hierarchy.groups.is_empty()
                    || !self.hierarchy.layouts.is_empty()
                    || !self.hierarchy.sizing.is_empty())
                .then(|| self.snapshot_hierarchy()),
            )
            .collect()
    }

    pub(super) fn restore_batch(&mut self, changes: &[Change], cx: &mut Context<Self>) {
        for change in changes {
            match change {
                Change::Hierarchy { value, .. } => self.hierarchy = value.clone(),
                Change::Board {
                    id,
                    value: Some(before),
                    ..
                } => {
                    if let Some(b) = self.boards.iter_mut().find(|b| b.id == *id) {
                        *b = before.clone();
                    }
                }
                Change::Shape {
                    id,
                    value: Some(before),
                    ..
                } => {
                    if let Some(s) = self.shapes.iter_mut().find(|s| s.id == *id) {
                        *s = before.clone();
                    }
                }
                Change::TextRect { id, board, value } => {
                    self.set_object_geometry(*id, *board, *value)
                }
                Change::Text { id, value } => {
                    if let Some(t) = self.texts.iter().find(|t| t.id == *id) {
                        t.editor.update(cx, |editor, cx| {
                            editor.restore(value.clone());
                            cx.notify();
                        });
                    }
                }
                _ => unreachable!("batch edits only change existing objects"),
            }
        }
    }

    pub(super) fn batch_changed(&self, changes: &[Change], cx: &gpui::App) -> bool {
        changes.iter().any(|change| match change {
            Change::Hierarchy { value, .. } => self.hierarchy != *value,
            Change::Board {
                id,
                value: Some(before),
                ..
            } => self.boards.iter().find(|b| b.id == *id) != Some(before),
            Change::Shape {
                id,
                value: Some(before),
                ..
            } => self.shapes.iter().find(|s| s.id == *id) != Some(before),
            Change::TextRect { id, board, value } => {
                self.object_rect(*id) != Some((*board, *value))
            }
            Change::Text { id, value } => self
                .texts
                .iter()
                .find(|t| t.id == *id)
                .is_some_and(|t| t.editor.read(cx).snapshot() != *value),
            _ => false,
        })
    }

    pub(super) fn selection_pointer(
        &mut self,
        id: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.modifiers.control || event.modifiers.platform {
            self.focus.focus(window, cx);
            if event.modifiers.shift {
                self.toggle_selection(id, cx);
                cx.stop_propagation();
            } else {
                self.set_selection(BTreeSet::from([id]), cx);
                self.batch_before = self.before_geometry();
                self.begin(
                    GestureKind::SelectionMove,
                    event.position,
                    event.button,
                    window,
                    cx,
                );
            }
            return true;
        }
        if event.click_count >= 2
            && !event.modifiers.shift
            && (!self.is_selected(id) || self.multi_selection.len() > 1)
            && self.group_target(id) != id
        {
            self.focus.focus(window, cx);
            self.set_selection(BTreeSet::from([id]), cx);
            cx.stop_propagation();
            return true;
        }
        let target = if event.click_count < 2 && !self.is_selected(id) {
            self.group_target(id)
        } else {
            id
        };
        if target != id {
            if event.modifiers.shift {
                self.toggle_selection(target, cx);
                cx.stop_propagation();
            } else {
                self.set_selection(BTreeSet::from([target]), cx);
                self.batch_before = self.before_geometry();
                self.begin(
                    GestureKind::SelectionMove,
                    event.position,
                    event.button,
                    window,
                    cx,
                );
            }
            return true;
        }
        if event.modifiers.shift {
            self.focus.focus(window, cx);
            self.toggle_selection(id, cx);
            cx.stop_propagation();
            return true;
        }
        if self.multi_selection.contains(&id) && event.click_count < 2 {
            self.batch_before = self.before_geometry();
            self.begin(
                GestureKind::SelectionMove,
                event.position,
                event.button,
                window,
                cx,
            );
            return true;
        }
        false
    }

    pub(super) fn start_marquee(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial = self.selection_ids();
        let start = self.board_point(None, event.position);
        self.marquee = Some(Marquee {
            start,
            rect: creation::drag_rect(start, start, false),
            initial,
        });
        self.marquee_additive = event.modifiers.shift;
        if !event.modifiers.shift {
            self.select(None, cx);
        }
        self.begin(
            GestureKind::Marquee,
            event.position,
            event.button,
            window,
            cx,
        );
    }

    pub(super) fn move_marquee(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let end = self.board_point(None, position);
        let Some(marquee) = &mut self.marquee else {
            return;
        };
        marquee.rect = creation::drag_rect(marquee.start, end, false);
        let rect = marquee.rect;
        if rect.width * self.view.zoom < 3. && rect.height * self.view.zoom < 3. {
            return;
        }
        let mut ids = if self.marquee_additive {
            marquee.initial.clone()
        } else {
            BTreeSet::new()
        };
        for board in &self.boards {
            let r = board.rect;
            // A frame must be enclosed; dragging inside it selects its children.
            if self.layer_editable(board.id)
                && r.x >= rect.x
                && r.y >= rect.y
                && r.x + r.width <= rect.x + rect.width
                && r.y + r.height <= rect.y + rect.height
            {
                ids.insert(self.group_target(board.id));
            }
        }
        for id in self
            .shapes
            .iter()
            .map(|s| s.id)
            .chain(self.texts.iter().map(|t| t.id))
        {
            if self.layer_editable(id)
                && self.world_rect(id).is_some_and(|r| {
                    crate::scene::rotation::intersects(r, self.object_rotation(id), rect)
                })
            {
                ids.insert(self.group_target(id));
            }
        }
        self.set_selection(ids, cx);
    }

    pub(super) fn move_selection(&mut self, delta: Point<f32>) {
        let moved = self.descendants(&self.selection_ids());
        if let Some(Change::Hierarchy { value, .. }) = self
            .batch_before
            .iter()
            .find(|c| matches!(c, Change::Hierarchy { .. }))
        {
            for (id, layout) in &value.layouts {
                if moved.contains(id)
                    && value
                        .groups
                        .get(id)
                        .is_some_and(|g| g.board.is_none_or(|b| !moved.contains(&b)))
                {
                    let frame = &mut self.hierarchy.layouts.get_mut(id).unwrap().frame;
                    frame.x = layout.frame.x + delta.x;
                    frame.y = layout.frame.y + delta.y;
                }
            }
        }
        for change in self.batch_before.clone() {
            let (id, parent, mut rect) = match change {
                Change::Board {
                    id, value: Some(b), ..
                } => (id, None, b.rect),
                Change::Shape {
                    id, value: Some(s), ..
                } => (id, s.board, s.rect),
                Change::TextRect { id, board, value } => (id, board, value),
                _ => continue,
            };
            rect.x = (rect.x + delta.x).clamp(-1_000_000., 1_000_000.);
            rect.y = (rect.y + delta.y).clamp(-1_000_000., 1_000_000.);
            self.set_object_rect(id, parent, rect);
        }
    }

    pub(super) fn finish_selection_move(&mut self, cx: &mut Context<Self>) {
        let before = std::mem::take(&mut self.batch_before);
        if !self.batch_changed(&before, cx) {
            return;
        }
        for id in self.selection_ids() {
            if self.is_auto_layout_child(id) {
                self.finish_layout_item_move(id);
                continue;
            }
            if self.hierarchy.groups.contains_key(&id) {
                self.reparent_group(id);
                continue;
            }
            if self.hierarchy.parents.contains_key(&id) {
                continue;
            }
            if self.boards.iter().any(|b| b.id == id) {
                continue;
            }
            if let Some((parent, rect)) = self.object_rect(id) {
                let (parent, rect) = self.parent_for_rect(parent, rect);
                self.set_object_rect(id, parent, rect);
                self.avoid_component_nesting(id);
            }
        }
        for id in self.selection_ids() {
            self.refresh_constraints(id);
        }
        self.history.borrow_mut().record(before, None);
        if self.multi_selection.is_empty() {
            if let Some(text) = self.selected_text() {
                self.selected = text.board;
            } else if let Some(shape) = self.selected_shape() {
                self.selected = shape.board;
            }
        }
        let ids = self.selection_ids();
        self.set_selection(ids, cx);
    }

    pub(super) fn selection_overlay(&self) -> Div {
        let mut overlay = div().absolute().inset_0();
        let outline = |r: Rect| {
            let p = self.view.screen(point(r.x, r.y));
            div()
                .absolute()
                .left(px(p.x))
                .top(px(p.y))
                .w(px((r.width * self.view.zoom).max(1.)))
                .h(px((r.height * self.view.zoom).max(1.)))
                .border_1()
                .border_color(rgb(ACCENT))
        };
        if !self.multi_selection.is_empty() {
            let mut union: Option<Rect> = None;
            for id in &self.multi_selection {
                if let Some(r) = self.world_bounds(*id) {
                    let id = *id;
                    overlay = overlay.child(
                        outline(r)
                            .debug_selector(move || format!("multi-selection-object-{id}"))
                            .border_2(),
                    );
                    union = Some(match union {
                        None => r,
                        Some(a) => {
                            let x = a.x.min(r.x);
                            let y = a.y.min(r.y);
                            Rect {
                                x,
                                y,
                                width: (a.x + a.width).max(r.x + r.width) - x,
                                height: (a.y + a.height).max(r.y + r.height) - y,
                            }
                        }
                    });
                }
            }
            if let Some(r) = union {
                overlay =
                    overlay.child(outline(r).debug_selector(|| "multi-selection-bounds".into()));
            }
        }
        if let Some(m) = &self.marquee {
            overlay = overlay.child(
                outline(m.rect)
                    .bg(gpui::rgba(0xb4a2ee18))
                    .debug_selector(|| "selection-marquee".into()),
            );
        }
        overlay
    }

    pub(super) fn selection_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.gesture.is_some() || self.bezier_draft.is_some() || self.draw_tool.is_some() {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        if modifiers.alt {
            return false;
        }
        let command = modifiers.control || modifiers.platform;
        if !command && self.vector_edit.is_some() {
            if matches!(event.keystroke.key.as_str(), "escape" | "enter") {
                self.exit_vector_edit(cx);
                cx.stop_propagation();
                return true;
            }
            if self.active_node().is_none()
                && matches!(event.keystroke.key.as_str(), "delete" | "backspace")
            {
                cx.stop_propagation();
                return true;
            }
        }
        if !command && self.active_node().is_some() {
            match event.keystroke.key.as_str() {
                "backspace" | "delete" => {
                    self.run_node_action(shapes::NodeAction::Delete, cx);
                    cx.stop_propagation();
                    return true;
                }
                "left" | "right" | "up" | "down" => {
                    let step = if modifiers.shift { 10. } else { 1. };
                    let delta = match event.keystroke.key.as_str() {
                        "left" => point(-step, 0.),
                        "right" => point(step, 0.),
                        "up" => point(0., -step),
                        _ => point(0., step),
                    };
                    self.nudge_node(delta, cx);
                    cx.stop_propagation();
                    return true;
                }
                _ => {}
            }
        }
        match (event.keystroke.key.as_str(), command) {
            ("a" | "A", false) if modifiers.shift => self.enable_auto_layout(window, cx),
            ("g", true) if modifiers.shift => self.ungroup_selection(cx),
            ("g", true) => self.group_selection(cx),
            ("[" | "{", true) => self.shift_layers(false, modifiers.shift, cx),
            ("]" | "}", true) => self.shift_layers(true, modifiers.shift, cx),
            ("f2", false) => {
                if self.selection_ids().len() == 1 {
                    self.begin_rename(*self.selection_ids().first().unwrap(), window, cx);
                }
            }
            ("a", true) => {
                let ids = self
                    .boards
                    .iter()
                    .map(|b| b.id)
                    .chain(self.shapes.iter().map(|s| s.id))
                    .chain(self.texts.iter().map(|t| t.id))
                    .chain(self.hierarchy.groups.keys().copied())
                    .collect();
                self.set_selection(ids, cx);
            }
            ("h" | "H", false) if modifiers.shift => self.flip_selection(true, cx),
            ("v" | "V", false) if modifiers.shift => self.flip_selection(false, cx),
            ("x", true) => self.cut_selection(cx),
            ("c", true) => self.copy_selection(cx),
            ("v", true) if modifiers.shift => self.paste_in_place(window, cx),
            ("v", true) => self.paste_selection(window, cx),
            ("d", true) => self.duplicate_selection(window, cx),
            ("left" | "right" | "up" | "down", false) => {
                let step = if modifiers.shift { 10. } else { 1. };
                let delta = match event.keystroke.key.as_str() {
                    "left" => point(-step, 0.),
                    "right" => point(step, 0.),
                    "up" => point(0., -step),
                    _ => point(0., step),
                };
                self.batch_before = self.before_geometry();
                self.move_selection(delta);
                self.finish_selection_move(cx);
                self.sync_fields(cx);
            }
            _ => return false,
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests;
