use super::*;
use crate::{bezier::Node, shape::StrokeAlign};

pub(in crate::workspace) struct BezierDraft {
    pub shape: Shape,
    nodes: Vec<Node>,
    redo: Vec<Node>,
    hover: Option<Point<Pixels>>,
}

impl BezierDraft {
    pub(in crate::workspace) fn can_replay(&self, redo: bool) -> bool {
        if redo {
            !self.redo.is_empty()
        } else {
            !self.nodes.is_empty()
        }
    }
}

impl Workspace {
    pub(in crate::workspace) fn update_bezier_hover(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let hover = self.bounds.get().contains(&position).then_some(position);
        if let Some(draft) = &mut self.bezier_draft
            && draft.hover != hover
        {
            draft.hover = hover;
            cx.notify();
        }
    }

    fn bezier_preview_nodes(&self) -> Option<[Node; 2]> {
        if self.draw_tool != Some(DrawTool::Shape(ShapeKind::Bezier)) || self.gesture.is_some() {
            return None;
        }
        let draft = self.bezier_draft.as_ref()?;
        let cursor = draft.hover?;
        if !self.bounds.get().contains(&cursor) {
            return None;
        }
        let last = *draft.nodes.last()?;
        let p = self.board_point(draft.shape.board, cursor);
        let first = draft.nodes[0];
        let end = if draft.nodes.len() >= 2
            && (first.anchor.x - p.x).hypot(first.anchor.y - p.y) * self.view.zoom <= 8.
        {
            first
        } else {
            Node::corner(p)
        };
        Some([last, end])
    }

    pub(in crate::workspace) fn bezier_hover_preview(&self) -> Option<AnyElement> {
        let [from, to] = self.bezier_preview_nodes()?;
        let draft = self.bezier_draft.as_ref()?;
        let origin = self.parent_origin(draft.shape.board);
        let screen = |p: Point<f32>| {
            let p = self.view.screen(origin + p);
            point(px(p.x), px(p.y))
        };
        // Only tessellate the provisional segment; the draft's cached mesh stays untouched.
        let mut path = gpui::PathBuilder::stroke(px(1.5));
        path.move_to(screen(from.anchor));
        path.cubic_bezier_to(
            screen(to.anchor),
            screen(from.outgoing),
            screen(to.incoming),
        );
        let path = path.build().ok()?;
        Some(
            div()
                .id("bezier-hover-preview")
                .debug_selector(|| "bezier-hover-preview".into())
                .absolute()
                .inset_0()
                .child(
                    canvas(
                        move |_, _, _| path.clone(),
                        |bounds, path, window, _| {
                            paint_path(&path, bounds.origin, rgb(ACCENT).into(), window);
                        },
                    )
                    .size_full(),
                )
                .into_any_element(),
        )
    }

    pub(super) fn place_bezier(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let board = self
            .bezier_draft
            .as_ref()
            .map(|d| d.shape.board)
            .unwrap_or_else(|| self.board_at(self.board_point(None, position)));
        let p = self.board_point(board, position);
        if let Some(draft) = &mut self.bezier_draft {
            if draft.nodes.len() >= 2
                && (draft.nodes[0].anchor.x - p.x).hypot(draft.nodes[0].anchor.y - p.y)
                    * self.view.zoom
                    <= 8.
            {
                draft.shape.set_bezier(&draft.nodes, true);
                self.finish_bezier(cx);
                cx.stop_propagation();
                return;
            }
            draft.nodes.push(Node::corner(p));
            draft.redo.clear();
            draft.shape.set_bezier(&draft.nodes, false);
        } else {
            let mut shape = Shape::new(
                0,
                board,
                ShapeKind::Bezier,
                Rect {
                    x: p.x,
                    y: p.y,
                    width: 1.,
                    height: 1.,
                },
            );
            shape.fill_enabled = false;
            shape.stroke.enabled = true;
            shape.stroke.width = 2.;
            shape.stroke.align = StrokeAlign::Center;
            let nodes = vec![Node::corner(p)];
            shape.set_bezier(&nodes, false);
            self.select(board, cx);
            self.bezier_draft = Some(BezierDraft {
                shape,
                nodes,
                redo: Vec::new(),
                hover: Some(position),
            });
        }
        self.begin(
            GestureKind::BezierPlace,
            position,
            MouseButton::Left,
            window,
            cx,
        );
    }
    pub(in crate::workspace) fn move_bezier_place(&mut self, position: Point<Pixels>, shift: bool) {
        let Some(board) = self.bezier_draft.as_ref().map(|d| d.shape.board) else {
            return;
        };
        let mut p = self.board_point(board, position);
        let draft = self.bezier_draft.as_mut().unwrap();
        let Some(node) = draft.nodes.last_mut() else {
            return;
        };
        if shift {
            p = drawing::snap_line(node.anchor, p);
        }
        if (p.x - node.anchor.x).hypot(p.y - node.anchor.y) * self.view.zoom < 2. {
            p = node.anchor;
        }
        node.outgoing = p;
        node.incoming = node.anchor + (node.anchor - p);
        draft.shape.set_bezier(&draft.nodes, false);
    }
    pub(in crate::workspace) fn cancel_bezier_place(&mut self) {
        if let Some(draft) = &mut self.bezier_draft {
            draft.nodes.pop();
            draft.shape.set_bezier(&draft.nodes, false);
        }
    }
    pub(in crate::workspace) fn discard_bezier(&mut self) {
        self.bezier_draft = None;
        self.shape_paths.borrow_mut().remove(&0);
    }
    pub(in crate::workspace) fn finish_bezier(&mut self, cx: &mut Context<Self>) {
        let Some(mut draft) = self.bezier_draft.take() else {
            return;
        };
        self.shape_paths.borrow_mut().remove(&0);
        self.draw_tool = None;
        if draft.nodes.len() < 2 {
            cx.notify();
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        draft.shape.id = id;
        draft.shape.name = crate::i18n::message("pen-name", &[("id", id.to_string())]);
        (draft.shape.board, draft.shape.rect) =
            self.parent_for_rect(draft.shape.board, draft.shape.rect);
        self.shapes.push(draft.shape);
        self.history.borrow_mut().break_group();
        self.history.borrow_mut().record(
            vec![Change::Shape {
                id,
                index: self.shapes.len() - 1,
                value: None,
            }],
            None,
        );
        self.select_shape(id, cx);
    }
    pub(in crate::workspace) fn bezier_history(
        &mut self,
        redo: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            self.finish_gesture(window, cx);
        }
        if let Some(draft) = &mut self.bezier_draft {
            if redo {
                if let Some(node) = draft.redo.pop() {
                    draft.nodes.push(node);
                }
            } else if let Some(node) = draft.nodes.pop() {
                draft.redo.push(node);
            }
            if draft.nodes.is_empty() {
                draft.shape.nodes = Default::default();
                draft.shape.points = Default::default();
            } else {
                draft.shape.set_bezier(&draft.nodes, false);
            }
            cx.notify();
        }
    }
    pub(in crate::workspace) fn move_bezier_node(
        &mut self,
        id: usize,
        index: usize,
        part: usize,
        position: Point<Pixels>,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(before) = self.path_before.as_ref() else {
            return;
        };
        if self.gesture.is_some_and(|g| {
            let delta = (position - g.start).map(f32::from);
            delta.x.hypot(delta.y) < 3.
        }) {
            if let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id)
                && shape != before
            {
                shape.clone_from(before);
                self.sync_fields(cx);
                cx.notify();
            }
            return;
        }
        let pivot = crate::rotation::center(before.rect);
        let p = crate::rotation::around(
            self.board_point(before.board, position),
            pivot,
            -before.layer.rotation,
        );
        let mut nodes = before.editable_nodes();
        if part >= 3 {
            let start = crate::rotation::around(
                self.board_point(before.board, self.gesture.unwrap().start),
                pivot,
                -before.layer.rotation,
            );
            let mut delta = p - start;
            if shift && part == 3 {
                if delta.x.abs() >= delta.y.abs() {
                    delta.y = 0.;
                } else {
                    delta.x = 0.;
                }
            }
            let next = (index + 1) % nodes.len();
            if part == 3 {
                nodes[index] = nodes[index].map(|p| p + delta);
                nodes[next] = nodes[next].map(|p| p + delta);
            } else {
                let t = self.vector_segment_t;
                let bend = delta / (3. * t * (1. - t));
                nodes[index].outgoing += bend;
                nodes[next].incoming += bend;
                nodes[index].smooth = false;
                nodes[next].smooth = false;
            }
        } else {
            let node = &mut nodes[index];
            if part == 0 {
                let delta = p - node.anchor;
                node.anchor = p;
                node.incoming += delta;
                node.outgoing += delta;
            } else {
                let p = if shift {
                    drawing::snap_line(node.anchor, p)
                } else {
                    p
                };
                if part == 1 {
                    node.incoming = p;
                } else {
                    node.outgoing = p;
                }
                if node.smooth {
                    let direction = p - node.anchor;
                    let length = direction.x.hypot(direction.y);
                    if length > 0.0001 {
                        let other = if part == 1 {
                            &mut node.outgoing
                        } else {
                            &mut node.incoming
                        };
                        let opposite_length =
                            (other.x - node.anchor.x).hypot(other.y - node.anchor.y);
                        *other = node.anchor - direction * (opposite_length / length);
                    }
                }
            }
        }
        if let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id) {
            shape.apply_vector_nodes(&nodes, before.editable_closed());
            shape.preserve_rotation_pivot(pivot);
        }
        self.sync_fields(cx);
    }
    pub(super) fn bezier_handles(
        &self,
        shape: &Shape,
        outset: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let id = shape.id;
        let nodes: Vec<_> = shape
            .nodes
            .0
            .iter()
            .map(|node| {
                node.map(|p| {
                    point(
                        outset + p.x * shape.rect.width * self.view.zoom,
                        outset + p.y * shape.rect.height * self.view.zoom,
                    )
                })
            })
            .collect();
        // Node stores f32 coordinates; build the control guide in screen-local pixels.
        let mut guide = gpui::PathBuilder::stroke(px(1.));
        if self.vector_edit == Some(id) && !nodes.is_empty() {
            guide.move_to(nodes[0].anchor.map(px));
            for i in 0..nodes.len() - 1 + usize::from(shape.closed) {
                let a = nodes[i];
                let b = nodes[(i + 1) % nodes.len()];
                guide.cubic_bezier_to(b.anchor.map(px), a.outgoing.map(px), b.incoming.map(px));
            }
            if shape.closed {
                guide.close();
            }
        }
        for node in &nodes {
            guide.move_to(point(px(node.incoming.x), px(node.incoming.y)));
            guide.line_to(point(px(node.anchor.x), px(node.anchor.y)));
            guide.line_to(point(px(node.outgoing.x), px(node.outgoing.y)));
        }
        let path = guide.build().ok();
        let mut elements = vec![
            div()
                .debug_selector(move || format!("bezier-guides-{id}"))
                .absolute()
                .inset_0()
                .child(
                    canvas(
                        move |_, _, _| path.clone(),
                        |bounds, path, window, _| {
                            if let Some(path) = path {
                                paint_path(&path, bounds.origin, rgb(ACCENT).into(), window);
                            }
                        },
                    )
                    .size_full(),
                )
                .into_any_element(),
        ];
        let selected_node = self.active_node();
        for (index, node) in nodes.iter().enumerate() {
            let active = selected_node == Some((id, index));
            for (part, p) in [(1, node.incoming), (2, node.outgoing), (0, node.anchor)] {
                if part != 0 && p == node.anchor {
                    continue;
                }
                elements.push(
                    div()
                        .id(("bezier-node", index * 3 + part))
                        .debug_selector(move || format!("bezier-node-{index}-{part}"))
                        .absolute()
                        .left(px(p.x - 6.))
                        .top(px(p.y - 6.))
                        .size(px(12.))
                        .cursor(if self.vector_edit == Some(id) {
                            CursorStyle::Arrow
                        } else {
                            CursorStyle::Crosshair
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .size(px(if part == 0 { 8. } else { 6. }))
                                .when(part != 0 || node.smooth, |el| el.rounded_full())
                                .bg(rgb(if active && part == 0 {
                                    ACCENT
                                } else {
                                    0xffffff
                                }))
                                .border_1()
                                .border_color(rgb(ACCENT)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                let event = &gpui::MouseDownEvent {
                                    position: window.raw_mouse_position(),
                                    ..event.clone()
                                };
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
                                } else if id != 0 {
                                    if !this.layer_editable(id) {
                                        cx.stop_propagation();
                                        return;
                                    }
                                    this.selected_node = Some((id, index));
                                    this.path_before =
                                        this.shapes.iter().find(|s| s.id == id).cloned();
                                    this.begin(
                                        GestureKind::BezierEdit { id, index, part },
                                        event.position,
                                        event.button,
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, _: &gpui::MouseDownEvent, window, cx| {
                                if id != 0 && this.layer_editable(id) {
                                    this.selected_node = Some((id, index));
                                    this.open_context_menu(
                                        Some(id),
                                        false,
                                        window.raw_mouse_position(),
                                        window,
                                        cx,
                                    );
                                }
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                );
            }
        }
        elements
    }
}

#[cfg(test)]
mod tests;
