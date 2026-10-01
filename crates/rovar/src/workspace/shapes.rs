use super::*;
use crate::shape::{Shape, ShapeKind};
use gpui::{AnyElement, CursorStyle, Path, canvas};
mod bezier_tool;
mod corners;
mod drawing;
pub(super) use bezier_tool::BezierDraft;
pub(super) use corners::State as Corners;
#[cfg(test)]
mod expanded_tests;
mod geometry;
mod inspector;
mod nodes;
mod vector_edit;
pub(super) use drawing::Draft;
use geometry::{Geometry, GeometryKey};
pub(super) use nodes::NodeAction;
use std::{cell::RefCell, collections::HashMap};

pub(super) type ShapePaths = Rc<RefCell<HashMap<usize, Rc<Geometry>>>>;

impl Workspace {
    pub(super) fn selected_shape(&self) -> Option<&Shape> {
        self.shapes
            .iter()
            .find(|s| Some(s.id) == self.selected_shape)
    }
    pub(super) fn edit_shape<R>(&mut self, change: impl FnOnce(&mut Shape) -> R) -> Option<R> {
        let index = self
            .shapes
            .iter()
            .position(|s| Some(s.id) == self.selected_shape)?;
        let before = self.shapes[index].clone();
        let result = change(&mut self.shapes[index]);
        let shape = &mut self.shapes[index];
        if shape.color != before.color
            || shape.fill_mode != before.fill_mode
            || shape.gradient != before.gradient
        {
            shape.color_style = None;
        }
        if shape.stroke.color != before.stroke.color
            || shape.stroke.fill_mode != before.stroke.fill_mode
            || shape.stroke.gradient != before.stroke.gradient
        {
            shape.stroke.color_style = None;
        }
        if before != self.shapes[index] {
            let mut changes: Vec<_> = self
                .fix_layout_size(before.id, before.rect, self.shapes[index].rect)
                .into_iter()
                .collect();
            changes.push(Change::Shape {
                id: before.id,
                index,
                value: Some(before),
            });
            self.history.borrow_mut().record(changes, None);
        }
        Some(result)
    }
    pub(super) fn select_shape(&mut self, id: usize, cx: &mut Context<Self>) {
        if !self.layer_editable(id) {
            return;
        }
        let Some(board) = self.shapes.iter().find(|s| s.id == id).map(|s| s.board) else {
            return;
        };
        let changed = self.selected_shape != Some(id);
        let duplicate = self
            .duplicate
            .take()
            .filter(|d| d.ids == std::collections::BTreeSet::from([id]));
        self.select(board, cx);
        self.selected_shape = Some(id);
        self.duplicate = duplicate;
        if changed {
            self.stroke_editing = self.selected_shape().unwrap().kind.is_path();
            let shape = self.selected_shape().unwrap();
            self.paint_stops = [
                shape.gradient.stops()[0].id,
                shape.stroke.gradient.stops()[0].id,
            ];
        }
        self.active_stop = self
            .selected_shape()
            .unwrap()
            .paint_gradient(self.stroke_editing)
            .stops()[0]
            .id;
        self.sync_fields(cx);
        cx.notify();
    }
    pub(super) fn content_elements(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let boards: HashMap<_, _> = self.boards.iter().map(|b| (b.id, b)).collect();
        let texts: HashMap<_, _> = self.texts.iter().map(|t| (t.id, t)).collect();
        let shapes: HashMap<_, _> = self.shapes.iter().map(|s| (s.id, s)).collect();
        let mut elements: Vec<_> = self
            .canvas_layer_order()
            .into_iter()
            .filter_map(|id| {
                if let Some(board) = boards.get(&id) {
                    Some((id, self.board_element(board, cx).into_any_element()))
                } else if let Some(text) = texts.get(&id) {
                    Some((id, self.text_element(text, cx).into_any_element()))
                } else if self.hierarchy.groups.contains_key(&id) {
                    self.group_element(id, cx).map(|el| (id, el))
                } else {
                    shapes
                        .get(&id)
                        .map(|s| (id, self.shape_element(s, cx).into_any_element()))
                }
            })
            .collect();
        if let Some(draft) = &self.draft {
            elements.push((
                usize::MAX,
                self.shape_element(&draft.shape, cx).into_any_element(),
            ));
        }
        if let Some(draft) = &self.bezier_draft {
            elements.push((
                usize::MAX,
                self.shape_element(&draft.shape, cx).into_any_element(),
            ));
        }
        elements.into_iter().map(|(_, el)| el).collect()
    }
    fn shape_element(&self, shape: &Shape, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let id = shape.id;
        let origin = self.parent_origin(shape.board);
        let position = self.view.screen(origin + point(shape.rect.x, shape.rect.y));
        let width = shape.rect.width * self.view.zoom;
        let height = shape.rect.height * self.view.zoom;
        let selected = self.selected_shape == Some(id);
        let key = GeometryKey::new(shape, self.view.zoom);
        let outset = key.outset;
        let fill = (shape.fill_enabled && shape.can_fill()).then(|| shape.background());
        let edit_hatch = self.vector_edit == Some(id) && shape.editable_closed();
        let stroke = shape.stroke.background();
        let image_fill =
            (shape.fill_enabled && shape.can_fill() && shape.fill_mode == FillMode::Image)
                .then(|| shape.image_fill.clone());
        let paths = self.shape_paths.clone();
        let surface = canvas(
            move |_, _, _| {
                let mut paths = paths.borrow_mut();
                if paths.get(&id).is_none_or(|cached| cached.key != key) {
                    paths.insert(id, Rc::new(key.clone().build()));
                }
                paths[&id].clone()
            },
            move |bounds, geometry, window, _| {
                if let Some(fill) = fill {
                    paint_path(&geometry.fill, bounds.origin, fill, window);
                }
                if let Some(image_fill) = &image_fill {
                    let image_bounds = Bounds::new(
                        bounds.origin + point(px(outset), px(outset)),
                        gpui::size(px(width), px(height)),
                    );
                    let capture = image_bounds.intersect(&window.content_mask().bounds);
                    if !capture.is_empty() && image_fill.asset.is_some() {
                        window.with_subtree_pair(
                            capture,
                            gpui::EffectShader::wgsl_two_images(include_str!(
                                "shapes/image_mask.wgsl"
                            )),
                            Default::default(),
                            0.,
                            image_fill.opacity,
                            |input, window| match input {
                                gpui::SubtreeInput::First => paint_path(
                                    &geometry.fill,
                                    bounds.origin,
                                    rgb(0xffffff).into(),
                                    window,
                                ),
                                gpui::SubtreeInput::Second => {
                                    image_fill.paint(image_bounds, window)
                                }
                            },
                        );
                    }
                }
                if edit_hatch {
                    // Reuse the actual filled contour, including holes and curves.
                    // This is only a paint overlay; document fills and hitboxes stay intact.
                    let scale = window.raster_scale_factor();
                    paint_path(
                        &geometry.fill,
                        bounds.origin,
                        gpui::pattern_slash(rgb(ACCENT).opacity(0.65), 1.5 * scale, 14. * scale),
                        window,
                    );
                }
                if let Some(path) = &geometry.stroke {
                    paint_path(path, bounds.origin, stroke, window);
                }
            },
        )
        .size_full();
        let element = div()
            .id(("shape", id))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &gpui::MouseDownEvent, window, cx| {
                    this.open_context_menu(
                        Some(id),
                        false,
                        window.raw_mouse_position(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .debug_selector(move || format!("shape-{id}"))
            .absolute()
            .left(px(position.x - outset))
            .top(px(position.y - outset))
            .w(px(width + outset * 2.))
            .h(px(height + outset * 2.))
            .cursor(CursorStyle::OpenHand)
            .when(self.vector_edit == Some(id), |el| {
                el.cursor(CursorStyle::Arrow)
            })
            .when(!self.layer_editable(id), |el| el.cursor(CursorStyle::Arrow))
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
                            this.focus.focus(window, cx);
                            this.select(None, cx);
                            cx.stop_propagation();
                            return;
                        }
                        if this.vector_edit == Some(id) {
                            this.vector_pointer(id, event, window, cx);
                            return;
                        }
                        if this.selection_pointer(id, event, window, cx) {
                            return;
                        }
                        if event.click_count >= 2 && !event.modifiers.shift {
                            if this.shapes.iter().any(|s| s.id == id && s.kind.is_media()) {
                                this.select_shape(id, cx);
                                if this.selected_shape().unwrap().kind == ShapeKind::Video {
                                    this.play_video(id, window, cx);
                                }
                                cx.stop_propagation();
                                return;
                            }
                            this.enter_vector_edit(id, window, cx);
                            cx.stop_propagation();
                            return;
                        }
                        this.select_shape(id, cx);
                        if let Some(shape) = this.selected_shape() {
                            this.begin(
                                GestureKind::Shape {
                                    id,
                                    original: shape.rect,
                                    handle: None,
                                },
                                event.position,
                                event.button,
                                window,
                                cx,
                            );
                        }
                    }
                }),
            )
            .when(!shape.kind.is_media(), |el| el.child(surface))
            .when(shape.kind.is_media(), |el| {
                el.child(self.media_surface(shape, outset))
            })
            .when(
                selected && self.vector_edit != Some(id) && !shape.kind.is_line(),
                |el| {
                    el.child(
                        div()
                            .absolute()
                            .left(px(outset))
                            .top(px(outset))
                            .w(px(width))
                            .h(px(height))
                            .border_1()
                            .border_color(rgb(ACCENT)),
                    )
                    .children(Handle::ALL.into_iter().enumerate().map(|(index, handle)| {
                        let x = (handle.0 as f32 + 1.) * 0.5 * width + outset;
                        let y = (handle.1 as f32 + 1.) * 0.5 * height + outset;
                        let cursor = rotation::handle_cursor(handle, shape.layer.rotation);
                        let corner = handle.0 != 0 && handle.1 != 0;
                        // Reserve the corner targets; the rest of each edge resizes
                        // without adding a visible midpoint or changing the hit slop.
                        let (left, top, hit_width, hit_height) = if handle.0 == 0 {
                            (outset + 6., y - 6., (width - 12.).max(0.), 12.)
                        } else if handle.1 == 0 {
                            (x - 6., outset + 6., 12., (height - 12.).max(0.))
                        } else {
                            (x - 6., y - 6., 12., 12.)
                        };
                        div()
                            .id(("shape-handle", index))
                            .debug_selector(move || format!("shape-handle-{index}"))
                            .absolute()
                            .left(px(left))
                            .top(px(top))
                            .w(px(hit_width))
                            .h(px(hit_height))
                            .cursor(cursor)
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(corner, |el| {
                                el.child(
                                    div()
                                        .debug_selector(move || format!("shape-corner-{index}"))
                                        .size(px(7.))
                                        .bg(rgb(0xffffff))
                                        .border_1()
                                        .border_color(rgb(ACCENT)),
                                )
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
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
                                        } else if let Some(shape) = this.selected_shape() {
                                            this.begin(
                                                GestureKind::Shape {
                                                    id,
                                                    original: shape.rect,
                                                    handle: Some(handle),
                                                },
                                                event.position,
                                                event.button,
                                                window,
                                                cx,
                                            );
                                        }
                                    },
                                ),
                            )
                    }))
                },
            )
            .when(
                selected && shape.kind.is_line() && self.vector_edit != Some(id),
                |el| el.children(self.line_handles(shape, outset, cx)),
            )
            .when(
                self.vector_edit == Some(id) || (shape.kind == ShapeKind::Bezier && id == 0),
                |el| el.children(self.bezier_handles(&self.vector_handle_shape(shape), outset, cx)),
            )
            .when(selected && self.vector_edit != Some(id), |el| {
                el.children(self.rotation_handles(id, width, height, outset, cx))
            });
        crate::rotation::surface(
            element,
            shape.layer.rotation,
            width + outset * 2.,
            height + outset * 2.,
            if self.vector_edit == Some(id) {
                // Control handles can extend well outside the curve's true bounds.
                shape
                    .nodes
                    .0
                    .iter()
                    .flat_map(|n| [n.anchor, n.incoming, n.outgoing])
                    .map(|p| {
                        ((p.x - 0.5) * width).hypot((p.y - 0.5) * height) + 8.
                            - width.min(height) / 2.
                            - outset
                    })
                    .fold(0., f32::max)
            } else {
                0.
            },
        )
    }
}

fn paint_path(
    path: &Path<Pixels>,
    origin: Point<Pixels>,
    color: gpui::Background,
    window: &mut Window,
) {
    let mut path = path.clone();
    path.bounds.origin += origin;
    for vertex in &mut path.vertices {
        vertex.xy_position += origin;
    }
    window.paint_path(path, color);
}

#[cfg(test)]
mod tests;
