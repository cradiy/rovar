use super::*;
use crate::ui::theme::Color;

const CANVAS_SPACE: f32 = 312.;
const FLOATING_GAPS: f32 = 56.;

pub(super) const PANEL_TOP: f32 = 14.;
pub(super) const PANEL_BOTTOM: f32 = 18.;
pub(super) const PANEL_RADIUS: f32 = 20.;

pub(super) fn mode_tabs() -> Div {
    div()
        .flex_1()
        .min_w_0()
        .p(px(3.))
        .rounded(px(12.))
        .bg(Color::Workspace.color().opacity(0.5))
        .flex()
        .items_center()
        .gap(px(2.))
}

pub(super) fn mode_tab(id: &'static str, active: bool) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(32.))
        .px(px(6.))
        .flex_1()
        .min_w_0()
        .rounded(px(9.))
        .border_1()
        .border_color(if active {
            Color::Accent.color().opacity(0.16)
        } else {
            Color::Transparent.color()
        })
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if active {
            ACCENT.color()
        } else {
            MUTED.color()
        })
        .cursor_pointer()
        .hover(|s| s.bg(Color::Text.color().opacity(0.04)))
        .when(active, |el| el.bg(Color::Selected.color()))
}

pub(super) fn empty_state(glyph: GpuiIcons, message: &'static str) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .p(px(24.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .text_center()
        .gap(px(16.))
        .child(
            div()
                .size(px(52.))
                .flex_shrink_0()
                .rounded(px(18.))
                .bg(Color::Accent.color().opacity(0.075))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(glyph, 22.).text_color(ACCENT.color())),
        )
        .child(
            div()
                .max_w(px(190.))
                .text_size(px(12.))
                .line_height(px(20.))
                .text_color(MUTED.color())
                .child(message),
        )
}

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
    pub right_collapsed: bool,
    pub window_width: f32,
}

impl Default for Panels {
    fn default() -> Self {
        Self {
            left: 280.,
            right: 288.,
            right_collapsed: false,
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
            Side::Left if self.right_collapsed => 46.,
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
        if self.preview.is_some() {
            return (16., 16.);
        }
        let left = if self.sidebar.collapsed {
            46.
        } else {
            self.panels.width(Side::Left)
        };
        let right = if self.panels.right_collapsed {
            46.
        } else {
            self.panels.width(Side::Right)
        };
        (left + 28., right + 28.)
    }

    pub(super) fn toggle_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_gesture(window, cx);
        self.close_export_menus(window, cx);
        self.close_effect_menus(window, cx);
        for popover in &self.inspector.paint_popovers {
            popover.update(cx, |state, cx| state.close(window, cx));
        }
        self.inspector
            .gradient_menu
            .update(cx, |state, cx| state.close(window, cx));
        self.zoom_menu
            .popover
            .update(cx, |state, cx| state.close(window, cx));
        self.panels.right_collapsed = !self.panels.right_collapsed;
        self.focus.focus(window, cx);
        cx.stop_propagation();
        cx.notify();
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
            .hover(|s| s.bg(Color::Accent.color().opacity(0.18)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if this.gesture.is_some() {
                        cx.stop_propagation();
                        return;
                    }
                    let other = match side {
                        Side::Left if this.panels.right_collapsed => 46.,
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
