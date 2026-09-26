use super::*;

const CANVAS_SPACE: f32 = 312.;
const FLOATING_GAPS: f32 = 56.;

pub(super) const PANEL_TOP: f32 = 14.;
pub(super) const PANEL_BOTTOM: f32 = 18.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
}

impl Side {
    pub fn minimum(self) -> f32 {
        match self {
            Self::Left => 216.,
            Self::Right => 256.,
        }
    }

    fn maximum(self) -> f32 {
        match self {
            Self::Left => 420.,
            Self::Right => 480.,
        }
    }
}

#[derive(Clone)]
pub(super) struct Panels {
    left: f32,
    right: f32,
    pub window_width: f32,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            left: 280.,
            right: 288.,
            window_width: 1280.,
        }
    }
}

impl Panels {
    fn preferred(&self, side: Side) -> f32 {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
    }

    pub fn set(&mut self, side: Side, value: f32) {
        match side {
            Side::Left => self.left = value,
            Side::Right => self.right = value,
        }
    }

    // Keep the user's preferred sizes when a smaller window temporarily constrains them.
    pub fn width(&self, side: Side) -> f32 {
        let other = match side {
            Side::Left => self.width(Side::Right),
            Side::Right => Side::Left.minimum(),
        };
        self.preferred(side)
            .clamp(side.minimum(), self.limit(side, other))
    }

    fn limit(&self, side: Side, other: f32) -> f32 {
        (self.window_width - other - CANVAS_SPACE - FLOATING_GAPS)
            .clamp(side.minimum(), side.maximum())
    }
}

impl Workspace {
    pub(super) fn canvas_insets(&self) -> (f32, f32) {
        let left = if self.sidebar.collapsed {
            46.
        } else {
            self.panels.width(Side::Left)
        };
        (left + 28., self.panels.width(Side::Right) + 28.)
    }

    pub(super) fn panel_resize_handle(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = match side {
            Side::Left => "resize-left-panel",
            Side::Right => "resize-right-panel",
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .absolute()
            .top(px(16.))
            .bottom(px(16.))
            .w(px(6.))
            .when(side == Side::Left, |el| el.right_0())
            .when(side == Side::Right, |el| el.left_0())
            .rounded(px(3.))
            .occlude()
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if this.gesture.is_some() {
                        cx.stop_propagation();
                        return;
                    }
                    let other = match side {
                        Side::Left => this.panels.width(Side::Right),
                        Side::Right => this.panels.width(Side::Left),
                    };
                    this.begin(
                        GestureKind::Panel {
                            side,
                            original: this.panels.preferred(side),
                            displayed: this.panels.width(side),
                            limit: this.panels.limit(side, other),
                        },
                        event.position,
                        event.button,
                        window,
                        cx,
                    );
                }),
            )
    }
}

#[cfg(test)]
mod tests;
