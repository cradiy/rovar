use super::*;
use crate::layer::{LayerGroup, LayerState};
use std::collections::{BTreeSet, HashMap};
mod controls;

#[derive(Default)]
pub(super) struct LayerDrag {
    pub ids: Vec<usize>,
    pub target: Option<(usize, bool)>, // true means above in the layer list
    pub moved: bool,
}

impl Workspace {
    pub(super) fn layer_parent(&self, id: usize) -> Option<usize> {
        self.hierarchy.parents.get(&id).copied().or_else(|| {
            self.hierarchy
                .groups
                .get(&id)
                .and_then(|g| g.board)
                .or_else(|| self.object_rect(id).and_then(|(board, _)| board))
        })
    }

    pub(super) fn ancestors(&self, id: usize) -> Vec<usize> {
        let mut result = Vec::new();
        let mut parent = self.layer_parent(id);
        while let Some(id) = parent {
            if result.contains(&id) {
                break;
            }
            result.push(id);
            parent = self.layer_parent(id);
        }
        result
    }

    pub(super) fn layer_ids(&self) -> Vec<usize> {
        self.boards
            .iter()
            .map(|b| b.id)
            .chain(self.texts.iter().map(|t| t.id))
            .chain(self.shapes.iter().map(|s| s.id))
            .chain(self.hierarchy.groups.keys().copied())
            .collect()
    }

    pub(super) fn hierarchy_children(&self) -> HashMap<Option<usize>, Vec<usize>> {
        let ranks: HashMap<_, _> = self
            .hierarchy
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        let mut children: HashMap<Option<usize>, Vec<usize>> = HashMap::new();
        for (id, board) in self
            .boards
            .iter()
            .map(|b| (b.id, None))
            .chain(self.shapes.iter().map(|s| (s.id, s.board)))
            .chain(self.texts.iter().map(|t| (t.id, t.board)))
            .chain(self.hierarchy.groups.iter().map(|(id, g)| (*id, g.board)))
        {
            let parent = self.hierarchy.parents.get(&id).copied().or(board);
            children.entry(parent).or_default().push(id);
        }
        for ids in children.values_mut() {
            ids.sort_unstable_by_key(|id| ranks.get(id).map_or((1, *id), |rank| (0, *rank)));
        }
        children
    }

    pub(super) fn ordered_children(&self, parent: Option<usize>) -> Vec<usize> {
        self.hierarchy_children()
            .remove(&parent)
            .unwrap_or_default()
    }

    pub(super) fn paint_order(&self) -> Vec<usize> {
        self.canvas_layer_order()
            .into_iter()
            .filter(|id| !self.hierarchy.groups.contains_key(id))
            .collect()
    }

    pub(super) fn canvas_layer_order(&self) -> Vec<usize> {
        let children = self.hierarchy_children();
        let states: HashMap<_, _> = self
            .boards
            .iter()
            .map(|b| (b.id, b.layer))
            .chain(self.shapes.iter().map(|s| (s.id, s.layer)))
            .chain(self.texts.iter().map(|t| (t.id, t.layer)))
            .chain(self.hierarchy.groups.iter().map(|(id, g)| (*id, g.layer)))
            .collect();
        fn visit(
            parent: Option<usize>,
            children: &HashMap<Option<usize>, Vec<usize>>,
            states: &HashMap<usize, LayerState>,
            out: &mut Vec<usize>,
        ) {
            if let Some(ids) = children.get(&parent) {
                for id in ids {
                    if states[id].hidden {
                        continue;
                    }
                    out.push(*id);
                    visit(Some(*id), children, states, out);
                }
            }
        }
        let mut result = Vec::new();
        visit(None, &children, &states, &mut result);
        result
    }

    // Place the group's hit area below its children, at the group's own layer depth.
    pub(super) fn group_element(
        &self,
        id: usize,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.layer_editable(id) {
            return None;
        }
        let rect = self.group_bounds(id)?;
        let position = self.view.screen(point(rect.x, rect.y));
        Some(
            div()
                .id(("canvas-group", id))
                .debug_selector(move || format!("canvas-group-{id}"))
                .absolute()
                .left(px(position.x))
                .top(px(position.y))
                .w(px(rect.width * self.view.zoom))
                .h(px(rect.height * self.view.zoom))
                .cursor(gpui::CursorStyle::OpenHand)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        if this.space_down {
                            this.begin(
                                GestureKind::Pan {
                                    original: this.view.pan,
                                },
                                event.position,
                                event.button,
                                window,
                                cx,
                            );
                            return;
                        }
                        if this.selection_pointer(id, event, window, cx) {
                            return;
                        }
                        this.set_selection(BTreeSet::from([id]), cx);
                        this.batch_before = this.before_geometry();
                        this.begin(
                            GestureKind::SelectionMove,
                            event.position,
                            event.button,
                            window,
                            cx,
                        );
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.open_context_menu(Some(id), false, event.position, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

    pub(super) fn descendants(&self, roots: &BTreeSet<usize>) -> BTreeSet<usize> {
        self.layer_ids()
            .into_iter()
            .filter(|id| {
                roots.contains(id) || self.ancestors(*id).iter().any(|p| roots.contains(p))
            })
            .collect()
    }

    /// Expand groups, but not boards: moving a board already moves its contents.
    pub(super) fn operation_ids(&self) -> BTreeSet<usize> {
        let all = self.descendants(&self.selection_ids());
        all.iter()
            .copied()
            .filter(|id| {
                (!self.hierarchy.groups.contains_key(id) || self.hierarchy.layouts.contains_key(id))
                    && !self.ancestors(*id).iter().any(|p| {
                        all.contains(p)
                            && (self.boards.iter().any(|b| b.id == *p)
                                || self.hierarchy.layouts.contains_key(p))
                    })
            })
            .collect()
    }

    pub(super) fn group_bounds(&self, id: usize) -> Option<Rect> {
        if let Some(layout) = self.hierarchy.layouts.get(&id) {
            let mut rect = layout.frame;
            let origin = self.parent_origin(self.hierarchy.groups.get(&id)?.board);
            rect.x += origin.x;
            rect.y += origin.y;
            return Some(rect);
        }
        self.descendants(&BTreeSet::from([id]))
            .into_iter()
            .filter(|child| *child != id && !self.hierarchy.groups.contains_key(child))
            .filter_map(|id| self.world_bounds(id))
            .reduce(|a, b| {
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                Rect {
                    x,
                    y,
                    width: (a.x + a.width).max(b.x + b.width) - x,
                    height: (a.y + a.height).max(b.y + b.height) - y,
                }
            })
    }

    pub(super) fn group_target(&self, id: usize) -> usize {
        self.ancestors(id)
            .into_iter()
            .rfind(|p| self.hierarchy.groups.contains_key(p))
            .unwrap_or(id)
    }

    pub(super) fn snapshot_hierarchy(&self) -> Change {
        Change::Hierarchy {
            value: self.hierarchy.clone(),
            selection: self.selection_ids(),
        }
    }

    pub(super) fn common_parent(&self, ids: &BTreeSet<usize>) -> Option<Option<usize>> {
        let first = self.layer_parent(*ids.first()?);
        ids.iter()
            .all(|id| self.layer_parent(*id) == first)
            .then_some(first)
    }

    pub(super) fn can_group(&self) -> bool {
        let ids = self.selection_ids();
        ids.len() > 1 && self.common_parent(&ids).is_some()
    }

    pub(super) fn group_selection(&mut self, cx: &mut Context<Self>) {
        if !self.can_group() {
            return;
        }
        let ids = self.selection_ids();
        let Some(parent) = self.common_parent(&ids) else {
            return;
        };
        if !ids.iter().all(|id| self.layer_editable(*id)) {
            return;
        }
        self.seal_text_edits(cx);
        let before = self.snapshot_hierarchy();
        let mut order = self.ordered_children(parent);
        let at = order.iter().rposition(|id| ids.contains(id)).unwrap();
        let above = order[at + 1..].to_vec();
        order.retain(|id| !ids.contains(id));
        let id = self.next_id;
        self.next_id += 1;
        let board = parent.and_then(|p| {
            if self.boards.iter().any(|b| b.id == p) {
                Some(p)
            } else {
                self.hierarchy.groups.get(&p).and_then(|g| g.board)
            }
        });
        self.hierarchy.groups.insert(
            id,
            LayerGroup {
                name: crate::i18n::message("group-name", &[("id", id.to_string())]),
                board,
                layer: LayerState::default(),
            },
        );
        if let Some(p) = parent.filter(|p| self.hierarchy.groups.contains_key(p)) {
            self.hierarchy.parents.insert(id, p);
        }
        for child in ids {
            self.hierarchy.parents.insert(child, id);
        }
        let at = order
            .iter()
            .position(|id| above.contains(id))
            .unwrap_or(order.len());
        order.insert(at, id);
        self.set_sibling_order(&order);
        self.set_selection(BTreeSet::from([id]), cx);
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(super) fn ungroup_selection(&mut self, cx: &mut Context<Self>) {
        let groups: Vec<_> = self
            .selection_ids()
            .into_iter()
            .filter(|id| self.hierarchy.groups.contains_key(id))
            .collect();
        if groups.is_empty() {
            return;
        }
        self.seal_text_edits(cx);
        let before = self.snapshot_hierarchy();
        let mut selected = self.selection_ids();
        for id in groups {
            let parent = self.layer_parent(id);
            let children = self.ordered_children(Some(id));
            let mut order = self.ordered_children(parent);
            let at = order.iter().position(|i| *i == id).unwrap();
            order.splice(at..=at, children.iter().copied());
            for child in &children {
                self.hierarchy.parents.remove(child);
                if let Some(p) = parent.filter(|p| self.hierarchy.groups.contains_key(p)) {
                    self.hierarchy.parents.insert(*child, p);
                }
            }
            self.hierarchy.groups.remove(&id);
            self.hierarchy.components.remove(&id);
            self.hierarchy.parents.remove(&id);
            self.hierarchy.order.retain(|i| *i != id);
            self.hierarchy.layouts.remove(&id);
            self.hierarchy.sizing.remove(&id);
            self.hierarchy.exports.remove(&id);
            self.set_sibling_order(&order);
            selected.remove(&id);
            selected.extend(children);
        }
        self.set_selection(selected, cx);
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(super) fn set_sibling_order(&mut self, ids: &[usize]) {
        // Materialize current order before overriding one sibling list.
        let mut all = self.layer_ids();
        let rank: HashMap<_, _> = self
            .hierarchy
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        all.sort_unstable_by_key(|id| rank.get(id).map_or((1, *id), |i| (0, *i)));
        all.retain(|id| !ids.contains(id));
        all.extend_from_slice(ids);
        self.hierarchy.order = all;
    }

    pub(super) fn shift_layers(&mut self, forward: bool, edge: bool, cx: &mut Context<Self>) {
        let selected = self.selection_ids();
        let Some(parent) = self.common_parent(&selected) else {
            return;
        };
        let before = self.snapshot_hierarchy();
        let mut ids = self.ordered_children(parent);
        let original = ids.clone();
        if edge {
            let moved: Vec<_> = ids
                .iter()
                .copied()
                .filter(|id| selected.contains(id))
                .collect();
            ids.retain(|id| !selected.contains(id));
            if forward {
                ids.extend(moved);
            } else {
                ids.splice(0..0, moved);
            }
        } else if forward {
            for i in (0..ids.len().saturating_sub(1)).rev() {
                if selected.contains(&ids[i]) && !selected.contains(&ids[i + 1]) {
                    ids.swap(i, i + 1);
                }
            }
        } else {
            for i in 1..ids.len() {
                if selected.contains(&ids[i]) && !selected.contains(&ids[i - 1]) {
                    ids.swap(i, i - 1);
                }
            }
        }
        if ids == original {
            return;
        }
        self.seal_text_edits(cx);
        self.set_sibling_order(&ids);
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(super) fn layer_name(&self, id: usize, cx: &gpui::App) -> String {
        if let Some(name) = self.hierarchy.names.get(&id) {
            return name.clone();
        }
        if let Some(g) = self.hierarchy.groups.get(&id) {
            return g.name.clone();
        }
        if let Some(b) = self.boards.iter().find(|b| b.id == id) {
            return b.name.clone();
        }
        if let Some(s) = self.shapes.iter().find(|s| s.id == id) {
            return s.name.clone();
        }
        self.texts
            .iter()
            .find(|t| t.id == id)
            .map(|t| {
                t.editor
                    .read(cx)
                    .content
                    .chars()
                    .take(64)
                    .map(|c| if c.is_whitespace() { ' ' } else { c })
                    .collect::<String>()
            })
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| crate::i18n::message("text-name", &[("id", id.to_string())]))
    }

    pub(super) fn prune_hierarchy(&mut self) {
        loop {
            let empty: Vec<_> = self
                .hierarchy
                .groups
                .keys()
                .copied()
                .filter(|id| {
                    !self.hierarchy.layouts.contains_key(id)
                        && self.ordered_children(Some(*id)).is_empty()
                })
                .collect();
            if empty.is_empty() {
                break;
            }
            for id in empty {
                self.hierarchy.groups.remove(&id);
                self.hierarchy.parents.remove(&id);
            }
        }
        let ids: BTreeSet<_> = self.layer_ids().into_iter().collect();
        self.hierarchy
            .parents
            .retain(|id, p| ids.contains(id) && ids.contains(p));
        self.hierarchy.names.retain(|id, _| ids.contains(id));
        self.hierarchy.order.retain(|id| ids.contains(id));
        self.hierarchy.layouts.retain(|id, _| ids.contains(id));
        self.hierarchy.sizing.retain(|id, _| ids.contains(id));
        self.hierarchy.exports.retain(|id, _| ids.contains(id));
        self.hierarchy.components.retain(|id, _| ids.contains(id));
    }
}

impl Workspace {
    pub(super) fn reparent_group(&mut self, id: usize) {
        if self.hierarchy.parents.contains_key(&id) {
            return;
        }
        let descendants = self.descendants(&BTreeSet::from([id]));
        // Frames inside a group keep their independent coordinate systems.
        if descendants
            .iter()
            .any(|id| self.boards.iter().any(|b| b.id == *id))
        {
            return;
        }
        let Some(rect) = self.group_bounds(id) else {
            return;
        };
        let board = self.board_at(point(rect.x + rect.width / 2., rect.y + rect.height / 2.));
        let board = board.filter(|board| self.component_parent_allowed(id, Some(*board)));
        let origin = self.parent_origin(board);
        for child in &descendants {
            if let Some(group) = self.hierarchy.groups.get(child)
                && self.hierarchy.layouts.contains_key(child)
            {
                let previous = self.parent_origin(group.board);
                let frame = &mut self.hierarchy.layouts.get_mut(child).unwrap().frame;
                frame.x += previous.x - origin.x;
                frame.y += previous.y - origin.y;
            }
        }
        let positions: Vec<_> = descendants
            .iter()
            .filter(|id| !self.hierarchy.groups.contains_key(id))
            .filter_map(|id| self.world_rect(*id).map(|r| (*id, r)))
            .collect();
        for (id, mut rect) in positions {
            rect.x -= origin.x;
            rect.y -= origin.y;
            self.set_object_rect(id, board, rect);
        }
        for id in descendants {
            if let Some(g) = self.hierarchy.groups.get_mut(&id) {
                g.board = board;
            }
        }
    }
}

#[cfg(test)]
mod tests;
