use super::*;
use crate::i18n::t;
use crate::ui::theme::Color;
use gpui::{AnyElement, uniform_list};
#[cfg(not(target_family = "wasm"))]
use gpui_effects::{LiquidGlass, LiquidGlassAppearance};
use std::collections::HashSet;
mod state;

#[derive(Clone, Default)]
pub(super) struct Sidebar {
    pub collapsed: bool,
    pub(super) resources: bool,
    pub renaming: Option<usize>,
    pub(in crate::editor) folded: HashSet<usize>,
}

// Only the enclosing surfaces sample the backdrop; their contents stay sharp.
#[cfg(not(target_family = "wasm"))]
pub(super) fn glass_surface() -> LiquidGlass {
    LiquidGlass::with_appearance(LiquidGlassAppearance {
        blur_radius: px(16.),
        clarity: 0.12,
        tint: Color::Glass.color().into(),
        refraction: px(3.),
        thickness: px(12.),
        highlight: 0.24,
        edge_shadow: 0.12,
        dispersion: 0.,
        ..if crate::ui::theme::is_dark() {
            LiquidGlassAppearance::dark()
        } else {
            LiquidGlassAppearance::regular()
        }
    })
}

// The current WebGPU backdrop shader fails browser uniformity validation.
// Keep the same panel layout with an opaque material until GPUI fixes it.
#[cfg(target_family = "wasm")]
pub(super) fn glass_surface() -> Div {
    div().bg(PANEL.color())
}

#[derive(Clone, Copy)]
enum LayerKind {
    Board,
    Text,
    Shape,
    Group,
}

#[derive(Clone, Copy)]
struct Layer {
    id: usize,
    index: usize,
    kind: LayerKind,
    depth: usize,
    has_children: bool,
}

impl Workspace {
    fn layer_rows(&self) -> Vec<Layer> {
        use std::collections::HashMap;
        let children = self.hierarchy_children();
        let kinds: HashMap<_, _> = self
            .boards
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id, (LayerKind::Board, i)))
            .chain(
                self.shapes
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (s.id, (LayerKind::Shape, i))),
            )
            .chain(
                self.texts
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (t.id, (LayerKind::Text, i))),
            )
            .chain(
                self.hierarchy
                    .groups
                    .keys()
                    .map(|id| (*id, (LayerKind::Group, 0))),
            )
            .collect();
        fn visit(
            parent: Option<usize>,
            depth: usize,
            children: &HashMap<Option<usize>, Vec<usize>>,
            kinds: &HashMap<usize, (LayerKind, usize)>,
            folded: &HashSet<usize>,
            out: &mut Vec<Layer>,
        ) {
            if let Some(ids) = children.get(&parent) {
                for id in ids.iter().rev() {
                    let (kind, index) = kinds[id];
                    let has_children = children.contains_key(&Some(*id));
                    out.push(Layer {
                        id: *id,
                        index,
                        kind,
                        depth,
                        has_children,
                    });
                    if has_children && !folded.contains(id) {
                        visit(Some(*id), depth + 1, children, kinds, folded, out);
                    }
                }
            }
        }
        let mut rows = Vec::new();
        visit(None, 0, &children, &kinds, &self.sidebar.folded, &mut rows);
        rows
    }

    fn layer_row(&self, layer: Layer, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let id = layer.id;
        let label = self.layer_name(id, cx);
        let glyph = match layer.kind {
            LayerKind::Board => LucideIcons::Frame,
            LayerKind::Text => LucideIcons::Type,
            LayerKind::Group => LucideIcons::Group,
            LayerKind::Shape => match self.shapes[layer.index].kind {
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
            },
        };
        let glyph = self.hierarchy.components.get(&id).map_or(glyph, |link| {
            if link.master {
                LucideIcons::Component
            } else {
                LucideIcons::Diamond
            }
        });
        let active = self.is_selected(id);
        let folded = self.sidebar.folded.contains(&id);
        let (own, parent) = self.layer_info(id).unwrap();
        let row_bounds = self.layer_row_bounds.clone();
        let drop = self
            .layer_drag
            .as_ref()
            .and_then(|drag| drag.target)
            .filter(|(target, _)| *target == id);
        let effective = self.effective_layer(own, parent);
        div()
            .on_paint_before_children(move |bounds, _, _, _| {
                row_bounds.borrow_mut().insert(id, bounds);
            })
            .id(("layer", id))
            .debug_selector(move || format!("layer-{id}"))
            .relative()
            .w_full()
            .h(px(36.))
            .px(px(6.))
            .rounded(px(7.))
            .border_1()
            .border_color(if active {
                Color::Accent.color().opacity(0.2667)
            } else {
                Color::Transparent.color()
            })
            .flex()
            .items_center()
            .gap(px(7.))
            .text_size(px(12.))
            .cursor_pointer()
            .when(!effective.editable(), |el| {
                el.cursor(gpui::CursorStyle::Arrow)
            })
            .hover(|s| s.bg(Color::Accent.color().opacity(0.0863)))
            .when(active, |el| {
                el.bg(Color::Accent.color().opacity(0.1647))
                    .text_color(ACCENT.color())
            })
            .pl(px(6. + layer.depth as f32 * 16.))
            .child(
                div()
                    .w(px(16.))
                    .flex_shrink_0()
                    .when(layer.has_children, |el| {
                        el.child(
                            div()
                                .id(("fold-layer", id))
                                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                                .debug_selector(move || format!("fold-layer-{id}"))
                                .size(px(16.))
                                .rounded(px(3.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .hover(|s| s.bg(Color::Text.color().opacity(0.0941)))
                                .child(
                                    icon(
                                        if folded {
                                            LucideIcons::ChevronRight
                                        } else {
                                            LucideIcons::ChevronDown
                                        },
                                        13.,
                                    )
                                    .text_color(MUTED.color()),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.sidebar.folded.remove(&id) {
                                        this.sidebar.folded.insert(id);
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(icon(glyph, 16.).flex_shrink_0().text_color(if active {
                ACCENT.color()
            } else {
                MUTED.color()
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(effective.hidden, |el| el.opacity(0.45))
                    .when(effective.locked, |el| el.text_color(MUTED.color()))
                    .when(self.sidebar.renaming != Some(id), |el| el.child(label))
                    .when(self.sidebar.renaming == Some(id), |el| {
                        el.child(
                            div()
                                .id("layer-rename")
                                .debug_selector(|| "layer-rename".into())
                                .w_full()
                                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                                .capture_key_down(cx.listener(
                                    |this, event: &gpui::KeyDownEvent, window, cx| {
                                        if event.keystroke.key == "escape" {
                                            this.finish_rename(false, window, cx);
                                            cx.stop_propagation();
                                            window.prevent_default();
                                        }
                                    },
                                ))
                                .child(
                                    uic::components::input::Input::new(&self.rename_input)
                                        .w_full()
                                        .h(px(26.))
                                        .px(px(3.))
                                        .py_0()
                                        .rounded(px(4.))
                                        .text_size(px(12.))
                                        .text_color(TEXT.color())
                                        .bg(Color::Input.color())
                                        .border_color(Color::Input.color())
                                        .appearance(uic::components::input::InputAppearance {
                                            caret_height: px(16.),
                                            ..crate::ui::theme::input_appearance()
                                        }),
                                ),
                        )
                    }),
            )
            .child(self.layer_controls(id, own, parent, cx))
            .when_some(drop, |el, (_, above)| {
                el.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .h(px(2.))
                        .bg(ACCENT.color())
                        .when(above, |el| el.top_0())
                        .when(!above, |el| el.bottom_0()),
                )
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.open_context_menu(Some(id), true, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.layer_mouse_down(id, event, window, cx)
                }),
            )
    }

    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        self.layer_row_bounds.borrow_mut().clear();
        let surface = glass_surface()
            .id("layers-panel")
            .debug_selector(|| "layers-panel".into())
            .absolute()
            .left(px(14.))
            .top(px(panels::PANEL_TOP))
            .rounded(px(16.))
            .border_1()
            .border_color(Color::Accent.color().opacity(0.1882))
            .shadow_lg()
            .occlude()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
        let collapsed = self.sidebar.collapsed;
        let toggle = div()
            .id("toggle-layers")
            .debug_selector(|| "toggle-layers".into())
            .size(px(28.))
            .flex_shrink_0()
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(Color::Accent.color().opacity(0.1333)))
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    toolbar::ToolTip(
                        t(if collapsed {
                            "expand-sidebar"
                        } else {
                            "collapse-sidebar"
                        })
                        .into(),
                    )
                })
                .into()
            })
            .child(
                icon(
                    if self.sidebar.collapsed {
                        LucideIcons::ChevronsRight
                    } else {
                        LucideIcons::ChevronsLeft
                    },
                    17.,
                )
                .text_color(MUTED.color()),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.sidebar.collapsed = !this.sidebar.collapsed;
                cx.stop_propagation();
                cx.notify();
            }));
        if self.sidebar.collapsed {
            return surface.p(px(8.)).child(toggle).into_any_element();
        }
        let mut tabs = div()
            .h(px(56.))
            .px(px(10.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(4.));
        for (resources, id, label, glyph) in [
            (false, "layers-tab", t("layers"), LucideIcons::Layers),
            (true, "resources-tab", t("assets"), LucideIcons::Box),
        ] {
            let active = self.sidebar.resources == resources;
            tabs = tabs.child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .h(px(34.))
                    .flex_1()
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(7.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if active {
                        ACCENT.color()
                    } else {
                        MUTED.color()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(Color::Accent.color().opacity(0.0863)))
                    .when(active, |el| el.bg(Color::Accent.color().opacity(0.1333)))
                    .child(icon(glyph, 17.).text_color(if active {
                        ACCENT.color()
                    } else {
                        MUTED.color()
                    }))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sidebar.resources = resources;
                        cx.stop_propagation();
                        cx.notify();
                    })),
            );
        }
        surface
            .w(px(self.panels.width(panels::Side::Left)))
            .bottom(px(panels::PANEL_BOTTOM))
            .flex()
            .flex_col()
            .child(tabs.child(toggle))
            .child(
                div()
                    .h(px(1.))
                    .flex_shrink_0()
                    .bg(Color::Accent.color().opacity(0.1255)),
            )
            .when(!self.sidebar.resources, |el| {
                let el = el
                    .child(self.pages_panel(cx))
                    .child(self.organization_actions(cx));
                let rows = self.layer_rows();
                if rows.is_empty() {
                    el.child(
                        div()
                            .pt(px(40.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(10.))
                            .child(icon(LucideIcons::Layers, 24.).text_color(Color::Muted.color()))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(MUTED.color())
                                    .child(t("layers-empty")),
                            ),
                    )
                } else {
                    el.child(
                        uniform_list(
                            "layer-list",
                            rows.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|index| this.layer_row(rows[index], cx))
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .flex_1()
                        .min_h_0()
                        .m(px(8.)),
                    )
                }
            })
            .when(self.sidebar.resources, |el| el.child(self.assets_panel(cx)))
            .child(self.panel_resize_handle(panels::Side::Left, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
