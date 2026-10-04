use super::*;
use crate::i18n::t;
use crate::scene::bezier;
use crate::ui::theme::Color;
use std::sync::Arc;
#[cfg(test)]
mod tests;

impl Workspace {
    pub(in crate::editor) fn enter_vector_edit(
        &mut self,
        id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.layer_editable(id) || self.gesture.is_some() {
            return;
        }
        if !self
            .shapes
            .iter()
            .any(|s| s.id == id && s.editable_nodes().len() >= 2)
        {
            return;
        }
        self.select_shape(id, cx);
        self.vector_edit = Some(id);
        self.vector_bend = false;
        self.draw_tool = None;
        self.toolbar.hand = false;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(in crate::editor) fn exit_vector_edit(&mut self, cx: &mut Context<Self>) {
        self.vector_edit = None;
        self.vector_hover = None;
        self.selected_node = None;
        self.vector_bend = false;
        cx.notify();
    }

    pub(in crate::editor) fn update_vector_hover(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let hover = (|| {
            let id = self.vector_edit?;
            if self.gesture.is_some()
                || self.space_down
                || self.vector_bend
                || !self.bounds.get().contains(&position)
            {
                return None;
            }
            let shape = self
                .selected_shape()
                .filter(|s| s.id == id && self.layer_editable(id))?;
            let p = crate::scene::rotation::around(
                self.board_point(shape.board, position),
                crate::scene::rotation::center(shape.rect),
                -shape.layer.rotation,
            );
            let nodes = shape.editable_nodes();
            // Existing anchors and handles take precedence over inserting a new node.
            if nodes
                .iter()
                .flat_map(|n| [n.anchor, n.incoming, n.outgoing])
                .any(|n| (n.x - p.x).hypot(n.y - p.y) * self.view.zoom < 10.)
            {
                return None;
            }
            let (index, _) =
                bezier::nearest_segment(&nodes, shape.editable_closed(), p, 8. / self.view.zoom)?;
            Some((id, index))
        })();
        if self.vector_hover != hover {
            self.vector_hover = hover;
            cx.notify();
        }
    }

    pub(in crate::editor) fn vector_hover_preview(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (id, segment) = self.vector_hover?;
        if self.vector_edit != Some(id)
            || self.gesture.is_some()
            || self.space_down
            || self.vector_bend
        {
            return None;
        }
        let shape = self.selected_shape().filter(|s| s.id == id)?;
        let nodes = shape.editable_nodes();
        let a = nodes.get(segment)?;
        let b = nodes
            .get(segment + 1)
            .or_else(|| shape.editable_closed().then(|| &nodes[0]))?;
        // Same t = 0.5 subdivision as the node panel; never use the chord midpoint.
        let p = (a.anchor + b.anchor) * 0.125 + (a.outgoing + b.incoming) * 0.375;
        let p = self.view.screen(
            self.parent_origin(shape.board)
                + crate::scene::rotation::around(
                    p,
                    crate::scene::rotation::center(shape.rect),
                    shape.layer.rotation,
                ),
        );
        Some(
            div()
                .id("vector-insert-preview")
                .debug_selector(|| "vector-insert-preview".into())
                .absolute()
                .left(px(p.x - 6.))
                .top(px(p.y - 6.))
                .size(px(12.))
                .cursor(CursorStyle::DragCopy)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .size(px(8.))
                        .rounded_full()
                        .bg(Color::Handle.color())
                        .border_1()
                        .border_color(ACCENT.color()),
                )
                .tooltip(|_, cx| {
                    cx.new(|_| toolbar::ToolTip(t("insert-midpoint").into()))
                        .into()
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &gpui::MouseDownEvent, window, cx| {
                        this.focus.focus(window, cx);
                        this.insert_bezier_segment(id, segment, 0.5, cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

    pub(super) fn vector_pointer(
        &mut self,
        id: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        self.vector_hover = None;
        if event.click_count >= 2 {
            self.insert_bezier_at(event.position, cx);
        } else {
            let shape = self.selected_shape().unwrap();
            let p = crate::scene::rotation::around(
                self.board_point(shape.board, event.position),
                crate::scene::rotation::center(shape.rect),
                -shape.layer.rotation,
            );
            let segment = bezier::nearest_segment(
                &shape.editable_nodes(),
                shape.editable_closed(),
                p,
                8. / self.view.zoom,
            );
            self.selected_node = None;
            if let Some((index, t)) = segment {
                self.vector_segment_t = t.clamp(0.05, 0.95);
                self.path_before = self.selected_shape().cloned();
                self.begin(
                    GestureKind::BezierEdit {
                        id,
                        index,
                        part: if self.vector_bend { 4 } else { 3 },
                    },
                    event.position,
                    event.button,
                    window,
                    cx,
                );
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn vector_handle_shape(&self, shape: &Shape) -> Shape {
        let mut handles = shape.clone();
        handles.nodes = bezier::Nodes(Arc::new(
            shape
                .editable_nodes()
                .iter()
                .map(|n| {
                    n.map(|p| {
                        point(
                            (p.x - shape.rect.x) / shape.rect.width.max(1.),
                            (p.y - shape.rect.y) / shape.rect.height.max(1.),
                        )
                    })
                })
                .collect(),
        ));
        handles.closed = shape.editable_closed();
        handles.rect.width = shape.rect.width.max(1.);
        handles.rect.height = shape.rect.height.max(1.);
        handles
    }

    pub(in crate::editor) fn vector_toolbar(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .absolute()
            .bottom(px(94.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                layers::glass_surface()
                    .id("vector-toolbar")
                    .debug_selector(|| "vector-toolbar".into())
                    .occlude()
                    .p(px(6.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(BORDER.color())
                    .shadow_lg()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .children(
                        [
                            (false, "vector-move", t("nodes"), LucideIcons::MousePointer2),
                            (true, "vector-bend", t("bend"), LucideIcons::Spline),
                        ]
                        .into_iter()
                        .map(|(bend, id, title, glyph)| {
                            div()
                                .id(id)
                                .debug_selector(move || id.into())
                                .h(px(32.))
                                .px(px(10.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(12.))
                                .cursor_pointer()
                                .when(self.vector_bend == bend, |el| {
                                    el.bg(Color::Selected.color())
                                })
                                .child(icon(glyph, 16.))
                                .child(title)
                                .tooltip(move |_, cx| {
                                    cx.new(|_| {
                                        toolbar::ToolTip(
                                            if bend {
                                                t("bend-hint")
                                            } else {
                                                t("nodes-hint")
                                            }
                                            .into(),
                                        )
                                    })
                                    .into()
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.vector_bend = bend;
                                    this.vector_hover = None;
                                    this.focus.focus(window, cx);
                                    cx.notify();
                                }))
                        }),
                    )
                    .child(div().w(px(1.)).h(px(20.)).bg(BORDER.color()))
                    .child(
                        div()
                            .id("vector-done")
                            .debug_selector(|| "vector-done".into())
                            .h(px(32.))
                            .px(px(10.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .cursor_pointer()
                            .child(icon(LucideIcons::Check, 16.))
                            .child(t("done"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.exit_vector_edit(cx);
                                this.focus.focus(window, cx);
                            })),
                    ),
            )
    }
}
