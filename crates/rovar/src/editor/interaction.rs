//! Pointer gesture dispatch, capture, and rollback.

use super::*;

pub(super) fn resize_cursor(handle: Handle) -> gpui::CursorStyle {
    use gpui::CursorStyle;
    match (handle.0, handle.1) {
        (0, _) => CursorStyle::ResizeUpDown,
        (_, 0) => CursorStyle::ResizeLeftRight,
        (-1, -1) | (1, 1) => CursorStyle::ResizeUpLeftDownRight,
        _ => CursorStyle::ResizeUpRightDownLeft,
    }
}

#[derive(Clone, Copy)]
pub(super) enum GestureKind {
    Rotate {
        id: usize,
        original: f32,
    },
    LayerSort,
    Marquee,
    SelectionMove,
    CornerRadius,
    ImageCrop {
        original: crate::scene::image_fill::Placement,
    },
    SelectionResize {
        original: Rect,
        handle: Handle,
    },
    Spacing {
        axis: usize,
        original: f32,
    },
    MultiProperty {
        property: Property,
    },
    Draw,
    Panel {
        side: panels::Side,
        original: f32,
        displayed: f32,
        limit: f32,
    },
    Property {
        index: usize,
        original: f32,
    },
    LayoutProperty {
        index: usize,
        original: f32,
    },
    EffectProperty {
        index: usize,
        field: usize,
        original: f32,
    },
    ColorStyleProperty {
        angle: bool,
        original: f32,
    },
    ColorStyleStop {
        left: f32,
        width: f32,
    },
    FillGradientStop {
        id: usize,
        original: f32,
        width: f32,
        inserted: bool,
    },
    GradientMidpoint {
        style: bool,
        id: usize,
        original: f32,
        width: f32,
    },
    GradientSeam {
        style: bool,
        original: f32,
    },
    BezierPlace,
    BezierEdit {
        id: usize,
        index: usize,
        part: usize,
    },
    LineEnd {
        id: usize,
        end: usize,
    },
    Move {
        id: usize,
        original: Rect,
    },
    Resize {
        id: usize,
        original: Rect,
        handle: Handle,
    },
    Text {
        id: usize,
        original: Rect,
        handle: Option<Handle>,
    },
    Shape {
        id: usize,
        original: Rect,
        handle: Option<Handle>,
    },
    Pan {
        original: Point<f32>,
    },
}
#[derive(Clone, Copy)]
pub(super) struct Gesture {
    pub kind: GestureKind,
    pub start: Point<Pixels>,
    pub button: MouseButton,
}

impl Workspace {
    pub(super) fn gesture_cursor(&self) -> Option<gpui::CursorStyle> {
        self.gesture.map(|gesture| match gesture.kind {
            GestureKind::Spacing { axis, .. } => {
                if axis == 0 {
                    gpui::CursorStyle::ResizeLeftRight
                } else {
                    gpui::CursorStyle::ResizeUpDown
                }
            }
            GestureKind::Rotate { .. } => gpui::CursorStyle::Crosshair,
            GestureKind::LayerSort => gpui::CursorStyle::ClosedHand,
            GestureKind::CornerRadius => gpui::CursorStyle::ClosedHand,
            GestureKind::ImageCrop { .. } => gpui::CursorStyle::ClosedHand,
            GestureKind::Panel { .. }
            | GestureKind::Property { .. }
            | GestureKind::LayoutProperty { .. }
            | GestureKind::EffectProperty { .. }
            | GestureKind::ColorStyleProperty { .. }
            | GestureKind::ColorStyleStop { .. }
            | GestureKind::FillGradientStop { .. }
            | GestureKind::GradientMidpoint { .. }
            | GestureKind::GradientSeam { .. }
            | GestureKind::MultiProperty { .. } => gpui::CursorStyle::ResizeLeftRight,
            GestureKind::Resize { handle, .. } | GestureKind::SelectionResize { handle, .. } => {
                resize_cursor(handle)
            }
            GestureKind::Text {
                id,
                handle: Some(handle),
                ..
            }
            | GestureKind::Shape {
                id,
                handle: Some(handle),
                ..
            } => rotation::handle_cursor(handle, self.object_rotation(id)),
            GestureKind::Marquee => gpui::CursorStyle::Arrow,
            GestureKind::BezierEdit { .. } if self.vector_edit.is_some() => {
                gpui::CursorStyle::Arrow
            }
            GestureKind::Draw
            | GestureKind::BezierPlace
            | GestureKind::BezierEdit { .. }
            | GestureKind::LineEnd { .. } => gpui::CursorStyle::Crosshair,
            GestureKind::SelectionMove
            | GestureKind::Move { .. }
            | GestureKind::Text { handle: None, .. }
            | GestureKind::Shape { handle: None, .. }
            | GestureKind::Pan { .. } => gpui::CursorStyle::OpenHand,
        })
    }

    pub(super) fn begin(
        &mut self,
        kind: GestureKind,
        position: Point<Pixels>,
        button: MouseButton,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match kind {
            GestureKind::Rotate { id, .. } => Some(id),
            GestureKind::Move { id, .. }
            | GestureKind::Resize { id, .. }
            | GestureKind::Text { id, .. }
            | GestureKind::Shape { id, .. }
            | GestureKind::LineEnd { id, .. }
            | GestureKind::BezierEdit { id, .. } => Some(id),
            _ => None,
        };
        if target.is_some_and(|id| !self.layer_editable(id)) {
            cx.stop_propagation();
            return;
        }
        self.seal_text_edits(cx);
        self.begin_snapping(kind);
        self.measure_target = None;
        self.gesture = Some(Gesture {
            kind,
            start: position,
            button,
        });
        // A numeric drag inside a color popover must not dismiss its own editor.
        // Pointer capture still routes moves/releases outside the popover to the canvas.
        let popover_focus = matches!(
            kind,
            GestureKind::Property { .. }
                | GestureKind::FillGradientStop { .. }
                | GestureKind::GradientMidpoint { style: false, .. }
                | GestureKind::GradientSeam { style: false, .. }
        )
        .then(|| {
            self.inspector.paint_popovers.iter().find_map(|popover| {
                let state = popover.read(cx);
                let focus = state.focus_handle(cx);
                (state.is_open() && focus.contains_focused(window, cx)).then_some(focus)
            })
        })
        .flatten();
        if !matches!(
            kind,
            GestureKind::ColorStyleProperty { .. }
                | GestureKind::ColorStyleStop { .. }
                | GestureKind::GradientMidpoint { style: true, .. }
                | GestureKind::GradientSeam { style: true, .. }
        ) {
            popover_focus
                .unwrap_or_else(|| self.focus.clone())
                .focus(window, cx);
        }
        if let Some(hitbox) = self.capture.get() {
            window.capture_pointer(hitbox);
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn move_gesture(
        &mut self,
        position: Point<Pixels>,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(gesture) = self.gesture else { return };
        let delta = point(
            f32::from(position.x - gesture.start.x),
            f32::from(position.y - gesture.start.y),
        );
        let snapped = self.snap_delta(delta / self.view.zoom);
        match gesture.kind {
            GestureKind::Rotate { id, original } => {
                self.move_rotation(id, original, gesture.start, position, shift, cx)
            }
            GestureKind::LayerSort => self.move_layer_sort(position),
            GestureKind::Marquee => self.move_marquee(position, cx),
            GestureKind::SelectionMove => self.move_selection(snapped),
            GestureKind::CornerRadius => self.move_corner_radius(delta / self.view.zoom, cx),
            GestureKind::ImageCrop { original } => {
                self.move_image_crop(original, delta / self.view.zoom)
            }
            GestureKind::SelectionResize { original, handle } => {
                self.resize_selection(original, handle, delta / self.view.zoom, shift, cx);
            }
            GestureKind::Spacing { axis, original } => {
                self.move_spacing(axis, original, delta, cx);
            }
            GestureKind::MultiProperty { property } => {
                self.move_multi_property(property, delta.x, shift, cx)
            }
            GestureKind::Panel {
                side,
                displayed,
                limit,
                ..
            } => {
                let width = displayed
                    + if side == panels::Side::Left {
                        delta.x
                    } else {
                        -delta.x
                    };
                let width = width.clamp(side.minimum(), limit);
                if self.panels.width(side) == width {
                    return;
                }
                self.panels.set(side, width);
            }
            GestureKind::Property { index, original } => {
                self.scrub_property(index, original, delta.x, shift, cx)
            }
            GestureKind::LayoutProperty { index, original } => {
                self.scrub_layout_number(index, original, delta.x, shift, cx)
            }
            GestureKind::EffectProperty {
                index,
                field,
                original,
            } => self.scrub_effect_number(index, field, original, delta.x, shift, cx),
            GestureKind::ColorStyleProperty { angle, .. } => {
                self.scrub_style_number(angle, delta.x, shift, self.snapping.bypass, cx)
            }
            GestureKind::ColorStyleStop { left, width, .. } => {
                let mut value = ((f32::from(position.x) - left) / width * 100.).clamp(0., 100.);
                if shift {
                    value = (value / 5.).round() * 5.;
                }
                self.set_style_number(false, value, cx);
            }
            GestureKind::FillGradientStop {
                id,
                original,
                width,
                ..
            } => {
                let mut position = (original + delta.x / width).clamp(0., 1.);
                if shift {
                    position = (position * 20.).round() / 20.;
                }
                self.move_fill_gradient_stop(id, position, cx);
            }
            GestureKind::GradientMidpoint {
                style,
                id,
                original,
                width,
            } => {
                self.set_gradient_midpoint(
                    style,
                    id,
                    (original + delta.x / width).clamp(0.01, 0.99),
                    cx,
                );
            }
            GestureKind::GradientSeam { style, original } => {
                self.set_gradient_seam(
                    style,
                    (original + delta.x.round() / 100.).clamp(0., 0.5),
                    cx,
                );
            }
            GestureKind::Draw => self.move_drawing(position, shift),
            GestureKind::BezierPlace => self.move_bezier_place(position, shift),
            GestureKind::BezierEdit { id, index, part } => {
                self.move_bezier_node(id, index, part, position, shift, cx)
            }
            GestureKind::LineEnd { id, end } => {
                self.move_line_endpoint(id, end, position, shift, cx)
            }
            GestureKind::Pan { original } => self.view.pan = original + delta,
            GestureKind::Text {
                id,
                original,
                handle,
            }
            | GestureKind::Shape {
                id,
                original,
                handle,
            } => {
                let rect = if let Some(handle) = handle {
                    self.resize_with_snapping(id, original, handle, delta / self.view.zoom, shift)
                } else {
                    Rect {
                        x: original.x + snapped.x,
                        y: original.y + snapped.y,
                        ..original
                    }
                };
                if let Some((parent, _)) = self.object_rect(id) {
                    self.set_object_rect(id, parent, rect);
                }
                self.sync_fields(cx);
            }
            GestureKind::Move { id, original } | GestureKind::Resize { id, original, .. } => {
                let rect = if let GestureKind::Resize { handle, .. } = gesture.kind {
                    self.resize_with_snapping(id, original, handle, delta / self.view.zoom, shift)
                } else {
                    Rect {
                        x: original.x + snapped.x,
                        y: original.y + snapped.y,
                        ..original
                    }
                };
                self.set_object_rect(id, None, rect);
                self.sync_fields(cx);
            }
        }
        cx.notify();
    }
    pub(super) fn cancel_gesture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.snapping.clear();
        if let Some(gesture) = self.gesture.take() {
            match gesture.kind {
                GestureKind::Rotate { id, original } => {
                    if let Some(layer) = self.layer_state_mut(id) {
                        layer.rotation = original;
                    }
                }
                GestureKind::LayerSort => self.layer_drag = None,
                GestureKind::Marquee => {
                    if let Some(m) = self.marquee.take() {
                        self.set_selection(m.initial, cx);
                    }
                }
                GestureKind::SelectionMove | GestureKind::MultiProperty { .. } => {
                    let before = std::mem::take(&mut self.batch_before);
                    self.restore_batch(&before, cx);
                    self.batch_values.clear();
                }
                GestureKind::Spacing { .. } => self.finish_spacing(false, cx),
                GestureKind::CornerRadius => self.finish_corner_radius(false, cx),
                GestureKind::ImageCrop { original } => {
                    if let Some(crop) = &mut self.image_crop {
                        crop.image.placement = original;
                    }
                }
                GestureKind::SelectionResize { .. } => self.finish_selection_resize(false, cx),
                GestureKind::Panel { side, original, .. } => self.panels.set(side, original),
                GestureKind::Property { .. } => self.finish_property_scrub(false, cx),
                GestureKind::FillGradientStop { .. } => self.finish_property_scrub(false, cx),
                GestureKind::LayoutProperty { .. } => self.finish_layout_scrub(false, cx),
                GestureKind::EffectProperty { .. } => self.finish_property_scrub(false, cx),
                GestureKind::ColorStyleProperty { angle, original } => {
                    self.set_style_number(angle, original, cx);
                }
                GestureKind::ColorStyleStop { .. } => self.finish_style_stop(false, cx),
                GestureKind::GradientMidpoint {
                    style,
                    id,
                    original,
                    ..
                } => {
                    if style {
                        self.set_gradient_midpoint(true, id, original, cx);
                    } else {
                        self.finish_property_scrub(false, cx);
                    }
                }
                GestureKind::GradientSeam { style, original } => {
                    if style {
                        self.set_gradient_seam(true, original, cx);
                    } else {
                        self.finish_property_scrub(false, cx);
                    }
                }
                GestureKind::Draw => {
                    self.box_draft = None;
                    self.draft = None;
                    self.shape_paths.borrow_mut().remove(&0);
                }
                GestureKind::BezierPlace => self.cancel_bezier_place(),
                GestureKind::LineEnd { id, .. } | GestureKind::BezierEdit { id, .. } => {
                    if let Some(before) = self.path_before.take()
                        && let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id)
                    {
                        *shape = before;
                    }
                }
                GestureKind::Pan { original } => self.view.pan = original,
                GestureKind::Text { id, original, .. } => {
                    if let Some(text) = self.texts.iter_mut().find(|t| t.id == id) {
                        text.rect = original;
                    }
                }
                GestureKind::Shape { id, original, .. } => {
                    if let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id) {
                        shape.rect = original;
                    }
                }
                GestureKind::Move { id, original } | GestureKind::Resize { id, original, .. } => {
                    if let Some(board) = self.boards.iter_mut().find(|b| b.id == id) {
                        board.rect = original;
                    }
                }
            }
            window.release_pointer();
            self.sync_fields(cx);
            cx.notify();
        }
    }
}
