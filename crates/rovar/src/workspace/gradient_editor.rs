use super::*;
use crate::{artboard::LinearGradient, i18n::t};
use std::{cell::Cell, rc::Rc};

impl Workspace {
    fn edited_gradient(&self, style: bool, cx: &gpui::App) -> Option<LinearGradient> {
        if style {
            self.colors.dialog.as_ref()?.gradient.clone()
        } else {
            self.fill_state(cx).map(|(_, gradient)| gradient)
        }
    }

    pub(super) fn set_gradient_midpoint(
        &mut self,
        style: bool,
        id: usize,
        value: f32,
        cx: &mut Context<Self>,
    ) {
        if self
            .edited_gradient(style, cx)
            .and_then(|g| g.stop(id).map(|s| s.midpoint))
            == Some(value)
        {
            return;
        }
        if style {
            if let Some(gradient) = self
                .colors
                .dialog
                .as_mut()
                .and_then(|d| d.gradient.as_mut())
            {
                gradient.set_midpoint(id, value);
            }
        } else {
            self.mutate_gradient(|g| g.set_midpoint(id, value), cx);
        }
        cx.notify();
    }

    pub(super) fn gradient_midpoints(
        &self,
        style: bool,
        gradient: &LinearGradient,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bounds = Rc::new(Cell::new(gpui::Bounds::<Pixels>::default()));
        let paint_bounds = bounds.clone();
        div()
            .relative()
            .w_full()
            .h(px(18.))
            .on_paint_before_children(move |rect, _, _, _| paint_bounds.set(rect))
            .children(gradient.stops().windows(2).filter(|p| p[1].position > p[0].position).map(|pair| {
                let id = pair[0].id;
                let midpoint = pair[0].midpoint;
                let span = pair[1].position - pair[0].position;
                let position = pair[0].position + midpoint * span;
                let bounds = bounds.clone();
                let prefix = if style { "style" } else { "fill" };
                let hint = format!("{} {}% · {}", t("gradient-midpoint"), inspector::number(midpoint * 100.), t("gradient-midpoint-reset"));
                let active = self.gesture.is_some_and(|g| matches!(g.kind, GestureKind::GradientMidpoint { style: s, id: i, .. } if s == style && i == id));
                div()
                    .id((gpui::SharedString::from(format!("{prefix}-midpoint")), id))
                    .debug_selector(move || format!("{prefix}-midpoint-{id}"))
                    .absolute()
                    .left(gpui::relative(position))
                    .ml(px(-9.))
                    .w(px(18.))
                    .h(px(18.))
                    .flex().items_center().justify_center()
                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                    .text_color(rgb(if active { ACCENT } else { MUTED }))
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .child(icon(LucideIcons::Diamond, 10.))
                    .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(hint.clone())).into())
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        if this.gesture.is_some() { return; }
                        if event.click_count == 2 {
                            this.set_gradient_midpoint(style, id, 0.5, cx);
                            return;
                        }
                        let width = f32::from(bounds.get().size.width) * span;
                        if width <= 0. { return; }
                        this.begin(GestureKind::GradientMidpoint { style, id, original: midpoint, width }, event.position, event.button, window, cx);
                        if !style { this.history.borrow_mut().begin_preview(); }
                    }))
            }))
    }

    pub(super) fn set_gradient_seam(&mut self, style: bool, value: f32, cx: &mut Context<Self>) {
        if !(0. ..=0.5).contains(&value)
            || self
                .edited_gradient(style, cx)
                .is_none_or(|g| g.seam_width == value)
        {
            return;
        }
        if style {
            if let Some(g) = self
                .colors
                .dialog
                .as_mut()
                .and_then(|d| d.gradient.as_mut())
            {
                g.seam_width = value;
            }
        } else {
            self.mutate_gradient(|g| g.seam_width = value, cx);
        }
        cx.notify();
    }

    pub(super) fn gradient_seam_control(
        &self,
        style: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let original = self.edited_gradient(style, cx).map_or(0., |g| g.seam_width);
        div()
            .id(if style {
                "style-seam-width"
            } else {
                "fill-seam-width"
            })
            .debug_selector(move || {
                if style {
                    "style-seam-width"
                } else {
                    "fill-seam-width"
                }
                .into()
            })
            .h(px(30.))
            .px(px(8.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .gap(px(6.))
            .justify_between()
            .bg(rgb(0x22252d))
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .hover(|s| s.bg(rgb(0x302a40)).text_color(rgb(TEXT)))
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .child(t("gradient-seam-width"))
            .child(
                div()
                    .text_color(rgb(TEXT))
                    .child(format!("{}%", inspector::number(original * 100.))),
            )
            .tooltip(|_, cx| {
                cx.new(|_| toolbar::ToolTip(t("gradient-seam-hint").into()))
                    .into()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if this.gesture.is_some() {
                        return;
                    }
                    if event.click_count == 2 {
                        this.set_gradient_seam(style, crate::artboard::default_seam_width(), cx);
                        return;
                    }
                    this.begin(
                        GestureKind::GradientSeam { style, original },
                        event.position,
                        event.button,
                        window,
                        cx,
                    );
                    if !style {
                        this.history.borrow_mut().begin_preview();
                    }
                }),
            )
    }
}
