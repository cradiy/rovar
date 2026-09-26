use super::*;
use crate::bezier::{self, Node};
use crate::i18n::t;
use crate::workspace::inspector::{icon_button, inspector_section};

#[derive(Clone, Copy)]
pub(in crate::workspace) enum NodeAction {
    Insert,
    Delete,
    Corner,
    Smooth,
    ToggleClosed,
}

#[cfg(test)]
mod tests;

impl Workspace {
    pub(in crate::workspace) fn active_node(&self) -> Option<(usize, usize)> {
        let (id, index) = self.selected_node?;
        let shape = self.selected_shape()?;
        (shape.id == id
            && self.vector_edit == Some(id)
            && index < shape.editable_nodes().len()
            && self.multi_selection.is_empty()
            && self.layer_editable(id))
        .then_some((id, index))
    }
    pub(in crate::workspace) fn node_action_enabled(&self, action: NodeAction) -> bool {
        let Some(s) = self
            .selected_shape()
            .filter(|s| self.vector_edit == Some(s.id) && self.layer_editable(s.id))
        else {
            return false;
        };
        if self.gesture.is_some() || self.draw_tool.is_some() || !self.multi_selection.is_empty() {
            return false;
        }
        match action {
            NodeAction::ToggleClosed => s.editable_nodes().len() >= 2,
            NodeAction::Delete => self.active_node().is_some() && s.editable_nodes().len() > 2,
            _ => self.active_node().is_some(),
        }
    }
    fn commit_nodes(
        &mut self,
        nodes: Vec<Node>,
        closed: bool,
        selected: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.vector_hover = None;
        let Some(index) = self
            .shapes
            .iter()
            .position(|s| Some(s.id) == self.selected_shape)
        else {
            return;
        };
        let before = self.shapes[index].clone();
        if nodes == before.editable_nodes() && closed == before.editable_closed() {
            return;
        }
        let node_before = self.selected_node;
        let pivot = crate::rotation::center(before.rect);
        self.shapes[index].apply_vector_nodes(&nodes, closed);
        self.shapes[index].preserve_rotation_pivot(pivot);
        self.selected_node = selected.map(|i| (before.id, i));
        if self.shapes[index] != before {
            self.seal_text_edits(cx);
            self.history.borrow_mut().record(
                vec![
                    Change::NodeSelection { value: node_before },
                    Change::Shape {
                        id: before.id,
                        index,
                        value: Some(before),
                    },
                ],
                None,
            );
        }
        if !closed {
            self.stroke_editing = true;
        }
        self.sync_fields(cx);
        cx.notify();
    }
    pub(in crate::workspace) fn run_node_action(
        &mut self,
        action: NodeAction,
        cx: &mut Context<Self>,
    ) {
        if !self.node_action_enabled(action) {
            return;
        }
        let shape = self.selected_shape().unwrap();
        let mut nodes = shape.editable_nodes();
        let mut closed = shape.editable_closed();
        let mut selected = self.active_node().map(|(_, i)| i);
        match action {
            NodeAction::ToggleClosed => closed = !closed,
            NodeAction::Insert => {
                let i = selected.unwrap();
                let segment = if !closed && i + 1 == nodes.len() {
                    i - 1
                } else {
                    i
                };
                selected = Some(bezier::split_segment(&mut nodes, segment, 0.5));
            }
            NodeAction::Delete => {
                let i = selected.unwrap();
                nodes.remove(i);
                selected = Some(i.min(nodes.len() - 1));
            }
            NodeAction::Corner => {
                let i = selected.unwrap();
                if nodes[i] == Node::corner(nodes[i].anchor) {
                    return;
                }
                nodes[i] = Node::corner(nodes[i].anchor);
            }
            NodeAction::Smooth => {
                let i = selected.unwrap();
                if nodes[i].smooth {
                    return;
                }
                bezier::smooth_node(&mut nodes, i, closed);
            }
        }
        self.commit_nodes(nodes, closed, selected, cx);
    }
    pub(in crate::workspace) fn insert_bezier_at(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(shape) = self
            .selected_shape()
            .filter(|s| self.vector_edit == Some(s.id) && self.layer_editable(s.id))
        else {
            return;
        };
        let p = crate::rotation::around(
            self.board_point(shape.board, position),
            crate::rotation::center(shape.rect),
            -shape.layer.rotation,
        );
        let nodes = shape.editable_nodes();
        let closed = shape.editable_closed();
        let Some((segment, t)) = bezier::nearest_segment(&nodes, closed, p, 8. / self.view.zoom)
        else {
            return;
        };
        self.insert_bezier_segment(shape.id, segment, t, cx);
    }
    pub(super) fn insert_bezier_segment(
        &mut self,
        id: usize,
        segment: usize,
        t: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(shape) = self
            .selected_shape()
            .filter(|s| s.id == id && self.vector_edit == Some(id) && self.layer_editable(id))
        else {
            return;
        };
        let mut nodes = shape.editable_nodes();
        let closed = shape.editable_closed();
        let segments = nodes.len().saturating_sub(1) + usize::from(closed);
        if nodes.len() < 2 || segment >= segments {
            return;
        }
        let index = bezier::split_segment(&mut nodes, segment, t);
        self.commit_nodes(nodes, closed, Some(index), cx);
    }
    pub(in crate::workspace) fn nudge_node(&mut self, delta: Point<f32>, cx: &mut Context<Self>) {
        let Some((_, index)) = self.active_node() else {
            return;
        };
        let shape = self.selected_shape().unwrap();
        let closed = shape.editable_closed();
        let mut nodes = shape.editable_nodes();
        let delta = crate::rotation::vector(delta, -shape.layer.rotation);
        nodes[index] = nodes[index].map(|p| p + delta);
        self.commit_nodes(nodes, closed, Some(index), cx);
    }
    pub(in crate::workspace) fn node_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let shape = self.selected_shape().unwrap();
        let smooth = self
            .active_node()
            .is_some_and(|(_, i)| shape.editable_nodes()[i].smooth);
        let closed = shape.editable_closed();
        inspector_section(t("path-nodes")).child(
            div().flex().gap(px(5.)).children(
                [
                    (
                        NodeAction::Insert,
                        "node-insert",
                        t("insert-node"),
                        LucideIcons::Plus,
                        false,
                    ),
                    (
                        NodeAction::Delete,
                        "node-delete",
                        t("delete-node-hint"),
                        LucideIcons::Trash2,
                        false,
                    ),
                    (
                        NodeAction::Corner,
                        "node-corner",
                        t("corner-node-hint"),
                        LucideIcons::Diamond,
                        self.active_node().is_some() && !smooth,
                    ),
                    (
                        NodeAction::Smooth,
                        "node-smooth",
                        t("smooth-node"),
                        LucideIcons::Circle,
                        smooth,
                    ),
                    (
                        NodeAction::ToggleClosed,
                        "path-closed",
                        if closed {
                            t("open-path")
                        } else {
                            t("close-path")
                        },
                        if closed {
                            LucideIcons::Link
                        } else {
                            LucideIcons::Unlink
                        },
                        closed,
                    ),
                ]
                .into_iter()
                .map(|(action, id, label, glyph, active)| {
                    let enabled = self.node_action_enabled(action);
                    icon_button(id, label, glyph, active)
                        .opacity(if enabled { 1. } else { 0.35 })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_node_action(action, cx);
                            this.focus.focus(window, cx);
                        }))
                }),
            ),
        )
    }
}
