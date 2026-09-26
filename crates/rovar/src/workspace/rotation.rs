use super::*;
use crate::i18n::t;
use crate::rotation::{self as geometry, center, normalize};

pub(super) fn handle_cursor(handle: Handle, angle: f32) -> gpui::CursorStyle {
    let angle = (handle.1 as f32).atan2(handle.0 as f32).to_degrees() + angle;
    match (angle / 45.).round().rem_euclid(4.) as u8 {
        0 => gpui::CursorStyle::ResizeLeftRight,
        1 => gpui::CursorStyle::ResizeUpLeftDownRight,
        2 => gpui::CursorStyle::ResizeUpDown,
        _ => gpui::CursorStyle::ResizeUpRightDownLeft,
    }
}

#[cfg(test)]
mod tests;

impl Workspace {
    pub(super) fn object_rotation(&self, id: usize) -> f32 {
        self.layer_info(id).map_or(0., |(s, _)| s.rotation)
    }
    pub(super) fn world_bounds(&self, id: usize) -> Option<Rect> {
        self.world_rect(id)
            .map(|r| geometry::bounds(r, self.object_rotation(id)))
    }
    pub(super) fn move_rotation(
        &mut self,
        id: usize,
        original: f32,
        start: Point<Pixels>,
        position: Point<Pixels>,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(rect) = self.world_rect(id) else {
            return;
        };
        let pivot = self.bounds.get().origin + self.view.screen(center(rect)).map(px);
        let a = (start - pivot).map(f32::from);
        let b = (position - pivot).map(f32::from);
        if b.x.hypot(b.y) < 2. {
            return;
        }
        let angle = original + normalize((b.y.atan2(b.x) - a.y.atan2(a.x)).to_degrees());
        let moved = (position - start).map(f32::from);
        let angle = if moved.x.hypot(moved.y) < 3. {
            original
        } else if shift {
            normalize((angle / 15.).round() * 15.)
        } else {
            normalize(angle)
        };
        if let Some(layer) = self.layer_state_mut(id) {
            layer.rotation = angle;
        }
        self.sync_fields(cx);
    }
    pub(super) fn edit_rotation(&mut self, value: &str) -> bool {
        let Some(id) = self.selected_text.or(self.selected_shape) else {
            return false;
        };
        let Some(angle) = value
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && v.abs() <= 1_000_000.)
        else {
            return false;
        };
        if let Some(layer) = self.layer_state_mut(id) {
            let before = *layer;
            layer.rotation = normalize(angle);
            if *layer != before {
                self.history
                    .borrow_mut()
                    .record(vec![Change::Layer { id, value: before }], None);
            }
        }
        true
    }
    pub(super) fn rotation_handles(
        &self,
        id: usize,
        width: f32,
        height: f32,
        outset: f32,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)]
            .into_iter()
            .enumerate()
            .map(|(i, (x, y))| {
                div()
                    .id(("rotate-handle", i))
                    .debug_selector(move || format!("rotate-handle-{i}"))
                    .absolute()
                    .left(px(outset + (x + 1.) * width / 2. + x * 17. - 8.))
                    .top(px(outset + (y + 1.) * height / 2. + y * 17. - 8.))
                    .size(px(16.))
                    .cursor(gpui::CursorStyle::Crosshair)
                    .tooltip(|_, cx| cx.new(|_| toolbar::ToolTip(t("rotate-hint").into())).into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.begin(
                                GestureKind::Rotate {
                                    id,
                                    original: this.object_rotation(id),
                                },
                                window.raw_mouse_position(),
                                event.button,
                                window,
                                cx,
                            );
                        }),
                    )
            })
            .collect()
    }
}
