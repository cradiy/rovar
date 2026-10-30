use super::inspector::icon_button;
use super::*;
use crate::i18n::t;
use crate::ui::theme::Color;
use gpui::{AnyElement, uniform_list};
#[cfg(not(target_family = "wasm"))]
use gpui_effects::{FrostedGlass, FrostedGlassAppearance, LiquidGlass, LiquidGlassAppearance};
use std::collections::HashSet;
mod state;

#[derive(Clone, Default)]
pub(super) struct Sidebar {
    pub collapsed: bool,
    pub(super) resources: bool,
    pub renaming: Option<usize>,
    pub(in crate::editor) folded: HashSet<usize>,
    rows: std::cell::RefCell<Option<RowsCache>>,
}

#[derive(Clone)]
struct RowsCache {
    index: Rc<super::scene_index::Index>,
    folded: HashSet<usize>,
    rows: Rc<Vec<Layer>>,
}

// Only the enclosing surfaces sample the backdrop; their contents stay sharp.
#[cfg(not(target_family = "wasm"))]
pub(super) fn glass_surface() -> LiquidGlass {
    glass_surface_with_tint(Color::Glass.color())
}

#[cfg(not(target_family = "wasm"))]
pub(super) fn sidebar_surface() -> FrostedGlass {
    FrostedGlass::with_appearance(FrostedGlassAppearance {
        blur_radius: px(32.),
        tint: Color::Panel.color().opacity(0.96).into(),
        edge_width: px(0.),
        sheen: 0.,
        ..if crate::ui::theme::is_dark() {
            FrostedGlassAppearance::dark()
        } else {
            FrostedGlassAppearance::light()
        }
    })
}

#[cfg(not(target_family = "wasm"))]
fn glass_surface_with_tint(tint: gpui::Rgba) -> LiquidGlass {
    LiquidGlass::with_appearance(LiquidGlassAppearance {
        blur_radius: px(16.),
        clarity: 0.12,
        tint: tint.into(),
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

#[cfg(target_family = "wasm")]
pub(super) use glass_surface as sidebar_surface;

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
    fn layer_summary(&self, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(38.))
            .px(px(16.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.))
            .text_color(MUTED.color())
            .child(icon(GpuiIcons::LucideFrame, 14.))
            .child(crate::i18n::message(
                "frames-count",
                &[("count", self.boards.len().to_string())],
            ))
            .child(div().flex_1())
            .child(
                icon_button(
                    "collapse-all-layers",
                    t("collapse-all-layers"),
                    GpuiIcons::LucideListCollapse,
                    false,
                )
                .size(px(26.))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.finish_rename(true, window, cx);
                    this.sidebar.folded = this.hierarchy_children().into_keys().flatten().collect();
                    this.focus.focus(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                })),
            )
    }

    fn layer_rows(&self) -> Rc<Vec<Layer>> {
        let index = self.scene_index();
        if let Some(cached) = self.sidebar.rows.borrow().as_ref()
            && Rc::ptr_eq(&cached.index, &index)
            && cached.folded == self.sidebar.folded
        {
            return cached.rows.clone();
        }
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
        let rows = Rc::new(rows);
        *self.sidebar.rows.borrow_mut() = Some(RowsCache {
            index,
            folded: self.sidebar.folded.clone(),
            rows: rows.clone(),
        });
        rows
    }

    fn layer_row(&self, layer: Layer, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let id = layer.id;
        let label = self.layer_name(id, cx);
        let mask_source =
            self.hierarchy.parents.get(&id).is_some_and(|parent| {
                crate::scene::mask::source(&self.hierarchy, *parent) == Some(id)
            });
        let glyph = match layer.kind {
            LayerKind::Board => GpuiIcons::LucideFrame,
            LayerKind::Text => GpuiIcons::LucideType,
            LayerKind::Group if crate::scene::mask::source(&self.hierarchy, id).is_some() => {
                GpuiIcons::LucideScan
            }
            LayerKind::Group if crate::scene::boolean::is_boolean(&self.hierarchy, id) => {
                GpuiIcons::LucideCombine
            }
            LayerKind::Group => GpuiIcons::LucideGroup,
            LayerKind::Shape => match self.shapes[layer.index].kind {
                ShapeKind::Rectangle => GpuiIcons::LucideSquare,
                ShapeKind::Ellipse => GpuiIcons::LucideCircle,
                ShapeKind::Arrow => GpuiIcons::LucideArrowUpRight,
                ShapeKind::Polygon => GpuiIcons::LucideTriangle,
                ShapeKind::Star => GpuiIcons::LucideStar,
                ShapeKind::Image => GpuiIcons::LucideImage,
                ShapeKind::Svg => GpuiIcons::LucideFileCode,
                ShapeKind::Video => GpuiIcons::LucideFilm,
                ShapeKind::Audio => GpuiIcons::LucideMusic,
                ShapeKind::Line => GpuiIcons::LucideMinus,
                ShapeKind::Pen => GpuiIcons::LucidePencil,
                ShapeKind::Bezier => GpuiIcons::LucidePenTool,
            },
        };
        let glyph = self.hierarchy.components.get(&id).map_or(glyph, |link| {
            if link.master {
                GpuiIcons::LucideComponent
            } else {
                GpuiIcons::LucideDiamond
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
            .rounded(px(10.))
            .border_1()
            .border_color(if active {
                Color::Accent.color().opacity(0.18)
            } else {
                Color::Transparent.color()
            })
            .flex()
            .items_center()
            .gap(px(7.))
            .text_size(px(12.))
            .when(matches!(layer.kind, LayerKind::Board), |el| {
                el.font_weight(FontWeight::MEDIUM)
            })
            .cursor_pointer()
            .when(!effective.editable(), |el| {
                el.cursor(gpui::CursorStyle::Arrow)
            })
            .hover(|s| s.bg(Color::Text.color().opacity(0.045)))
            .when(active, |el| {
                el.bg(Color::Accent.color().opacity(0.12))
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
                                            GpuiIcons::LucideChevronRight
                                        } else {
                                            GpuiIcons::LucideChevronDown
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
            .when(mask_source, |el| {
                el.child(
                    div()
                        .debug_selector(move || format!("layer-mask-source-{id}"))
                        .flex_shrink_0()
                        .h(px(18.))
                        .px(px(5.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .text_size(px(10.))
                        .text_color(ACCENT.color())
                        .bg(Color::Selected.color())
                        .child(t("mask-title")),
                )
            })
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
        let surface = sidebar_surface()
            .id("layers-panel")
            .debug_selector(|| "layers-panel".into())
            .absolute()
            .left(px(14.))
            .top(px(panels::PANEL_TOP))
            .rounded(px(panels::PANEL_RADIUS))
            .border_1()
            .border_color(Color::Text.color().opacity(0.12))
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
            .rounded(px(8.))
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
                        GpuiIcons::LucideChevronsRight
                    } else {
                        GpuiIcons::LucideChevronsLeft
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
        let mut tabs = panels::mode_tabs();
        for (resources, id, label, glyph) in [
            (false, "layers-tab", t("layers"), GpuiIcons::LucideLayers),
            (true, "resources-tab", t("assets"), GpuiIcons::LucideBox),
        ] {
            let active = self.sidebar.resources == resources;
            tabs = tabs.child(
                panels::mode_tab(id, active)
                    .child(icon(glyph, 15.).flex_shrink_0())
                    .child(div().min_w_0().truncate().child(label))
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
            .child(
                div()
                    .h(px(60.))
                    .px(px(12.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(tabs)
                    .child(toggle),
            )
            .when(!self.sidebar.resources, |el| {
                let el = el
                    .child(self.pages_panel(cx))
                    .child(self.organization_actions(cx))
                    .child(self.layer_summary(cx));
                let rows = self.layer_rows();
                if rows.is_empty() {
                    el.child(panels::empty_state(
                        GpuiIcons::LucideLayers,
                        t("layers-empty"),
                    ))
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
