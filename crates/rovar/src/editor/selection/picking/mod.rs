use super::*;
use crate::i18n::t;
use uic::components::context_menu::{self, ContextMenu, ContextMenuItem};

#[cfg(test)]
mod tests;

impl Workspace {
    pub(in crate::editor) fn layers_at(&self, position: Point<Pixels>) -> Vec<usize> {
        let world = self.board_point(None, position);
        self.canvas_layer_order()
            .into_iter()
            .rev()
            .filter(|id| {
                self.world_rect(*id).is_some_and(|rect| {
                    let p = crate::scene::rotation::around(
                        world,
                        crate::scene::rotation::center(rect),
                        -self.object_rotation(*id),
                    );
                    // Match the canvas' rectangular selection areas. Thin paths
                    // still need a usable, zoom-independent pointer target.
                    let slop_x = ((6. / self.view.zoom - rect.width) / 2.).max(0.);
                    let slop_y = ((6. / self.view.zoom - rect.height) / 2.).max(0.);
                    p.x >= rect.x - slop_x
                        && p.x <= rect.x + rect.width + slop_x
                        && p.y >= rect.y - slop_y
                        && p.y <= rect.y + rect.height + slop_y
                })
            })
            .collect()
    }

    fn pick_glyph(&self, id: usize) -> LucideIcons {
        if let Some(link) = self.hierarchy.components.get(&id) {
            return if link.master {
                LucideIcons::Component
            } else {
                LucideIcons::Diamond
            };
        }
        if self.hierarchy.groups.contains_key(&id) {
            LucideIcons::Group
        } else if self.boards.iter().any(|b| b.id == id) {
            LucideIcons::Frame
        } else if let Some(shape) = self.shapes.iter().find(|s| s.id == id) {
            match shape.kind {
                ShapeKind::Rectangle => LucideIcons::Square,
                ShapeKind::Ellipse => LucideIcons::Circle,
                ShapeKind::Arrow => LucideIcons::ArrowUpRight,
                ShapeKind::Polygon => LucideIcons::Triangle,
                ShapeKind::Star => LucideIcons::Star,
                ShapeKind::Image => LucideIcons::Image,
                ShapeKind::Video => LucideIcons::Film,
                ShapeKind::Line => LucideIcons::Minus,
                ShapeKind::Pen => LucideIcons::Pencil,
                ShapeKind::Bezier => LucideIcons::PenTool,
            }
        } else {
            LucideIcons::Type
        }
    }

    pub(in crate::editor) fn with_layer_picker(
        &self,
        menu: ContextMenu,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> ContextMenu {
        let hits = self.layers_at(position);
        if hits.is_empty() {
            return menu;
        }
        let mut submenu = ContextMenu::new();
        for id in hits {
            let name = self.layer_name(id, cx);
            let path = self
                .ancestors(id)
                .into_iter()
                .rev()
                .map(|id| self.layer_name(id, cx))
                .collect::<Vec<_>>()
                .join(" / ");
            let glyph = self.pick_glyph(id);
            let locked = !self.layer_editable(id);
            let selected = self.is_selected(id);
            let weak = cx.entity().downgrade();
            let hover = weak.clone();
            submenu = submenu.item(
                ContextMenuItem::action_with(
                    move |_, _| {
                        let hover = hover.clone();
                        let details = if path.is_empty() {
                            name.clone()
                        } else {
                            format!("{name}\n{path}")
                        };
                        div()
                            .id(("pick-layer", id))
                            .debug_selector(move || format!("pick-layer-{id}"))
                            .w_full()
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .opacity(if locked { 0.5 } else { 1. })
                            .child(icon(glyph, 14.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div().truncate().line_height(px(14.)).child(name.clone()),
                                    )
                                    .when(!path.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .truncate()
                                                .text_size(px(10.))
                                                .line_height(px(11.))
                                                .text_color(rgb(MUTED))
                                                .child(path.clone()),
                                        )
                                    }),
                            )
                            .when(locked || selected, |el| {
                                el.child(icon(
                                    if locked {
                                        LucideIcons::Lock
                                    } else {
                                        LucideIcons::Check
                                    },
                                    13.,
                                ))
                            })
                            .tooltip(move |_, cx| {
                                cx.new(|_| toolbar::ToolTip(details.clone())).into()
                            })
                            .on_hover(move |hovered, _, cx| {
                                let _ = hover.update(cx, |this, cx| {
                                    let target = if *hovered {
                                        Some(id)
                                    } else if this.pick_hover == Some(id) {
                                        None
                                    } else {
                                        this.pick_hover
                                    };
                                    if this.pick_hover != target {
                                        this.pick_hover = target;
                                        cx.notify();
                                    }
                                });
                            })
                    },
                    move |window, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.pick_hover = None;
                            if this.layer_editable(id) {
                                this.set_selection(BTreeSet::from([id]), cx);
                                this.focus.focus(window, cx);
                            }
                            cx.notify();
                        });
                    },
                )
                .disabled(locked),
            );
        }
        menu.item(ContextMenuItem::submenu_with(
            |_, _| {
                div()
                    .debug_selector(|| "context-select-layer".into())
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon(LucideIcons::Layers, 15.))
                    .child(t("select-layer"))
            },
            submenu,
        ))
        .separator()
    }

    pub(in crate::editor) fn layer_pick_preview(&self, cx: &gpui::App) -> Div {
        let overlay = div().absolute().inset_0();
        let Some(id) = self.pick_hover.filter(|_| context_menu::is_open(cx)) else {
            return overlay;
        };
        let Some(rect) = self.world_rect(id) else {
            return overlay;
        };
        if self
            .layer_info(id)
            .is_none_or(|(own, parent)| self.effective_layer(own, parent).hidden)
        {
            return overlay;
        }
        let view = self.view;
        let angle = self.object_rotation(id);
        overlay
            .debug_selector(|| "layer-pick-preview".into())
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let screen = |p| {
                            bounds.origin
                                + view
                                    .screen(crate::scene::rotation::around(
                                        p,
                                        crate::scene::rotation::center(rect),
                                        angle,
                                    ))
                                    .map(px)
                        };
                        let mut path = gpui::PathBuilder::stroke(px(1.5));
                        path.move_to(screen(point(rect.x, rect.y)));
                        path.line_to(screen(point(rect.x + rect.width, rect.y)));
                        path.line_to(screen(point(rect.x + rect.width, rect.y + rect.height)));
                        path.line_to(screen(point(rect.x, rect.y + rect.height)));
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, rgb(ACCENT));
                        }
                    },
                )
                .size_full(),
            )
    }
}
