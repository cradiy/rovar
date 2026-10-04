use super::*;
use crate::ui::theme::Color;
use crate::{i18n::t, scene::artboard::LinearGradient};
use std::{cell::Cell, rc::Rc};

impl Workspace {
    pub(super) fn gradient_track(
        &self,
        style: bool,
        gradient: &LinearGradient,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut ramp = gradient.clone();
        ramp.angle = 90.;
        ramp.kind = gpui::GradientKind::Linear;
        let active = if style {
            self.colors.dialog.as_ref().unwrap().active_stop
        } else {
            self.inspector.active_stop
        };
        let prefix = if style { "style" } else { "fill" };
        let bounds = if style {
            self.colors.ramp_bounds.clone()
        } else {
            Rc::new(Cell::new(gpui::Bounds::<Pixels>::default()))
        };
        let paint_bounds = bounds.clone();
        let ramp_bounds = bounds.clone();
        div()
            .on_paint_before_children(move |rect, _, _, _| paint_bounds.set(rect))
            .id(gpui::SharedString::from(format!("{prefix}-gradient-track")))
            .debug_selector(move || format!("{prefix}-gradient-track"))
            .flex_1()
            .min_w_0()
            .mx(px(9.))
            .relative()
            .h(px(64.))
            .child(self.gradient_midpoints(style, gradient, cx))
            .child(
                div()
                    .id(gpui::SharedString::from(format!("{prefix}-gradient-ramp")))
                    .debug_selector(move || format!("{prefix}-gradient-ramp"))
                    .absolute()
                    .top(px(18.))
                    .left_0()
                    .right_0()
                    .h(px(22.))
                    .rounded(px(5.))
                    .bg(gpui::checkerboard(Color::Checker.color(), 5.))
                    .cursor(gpui::CursorStyle::Crosshair)
                    .tooltip(|_, cx| cx.new(|_| toolbar::ToolTip(t("stop-add").into())).into())
                    .child(div().size_full().rounded(px(5.)).bg(ramp.background()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            if style {
                                this.begin_style_stop(None, event, window, cx);
                            } else {
                                this.begin_fill_gradient_stop(
                                    None,
                                    ramp_bounds.get(),
                                    event,
                                    window,
                                    cx,
                                );
                            }
                        }),
                    ),
            )
            .children(
                gradient
                    .stops()
                    .iter()
                    .filter(|s| s.id != active)
                    .chain(gradient.stops().iter().filter(|s| s.id == active))
                    .map(|stop| {
                        let id = stop.id;
                        let bounds = bounds.clone();
                        let hint = format!(
                            "{} {}%",
                            t("stop-position"),
                            inspector::number(stop.position * 100.)
                        );
                        div()
                            .id((gpui::SharedString::from(format!("{prefix}-stop")), id))
                            .debug_selector(move || format!("{prefix}-stop-{id}"))
                            .absolute()
                            .left(gpui::relative(stop.position))
                            .ml(px(-8.))
                            .top(px(38.))
                            .w(px(16.))
                            .h(px(22.))
                            .rounded(px(5.))
                            .border_2()
                            .border_color(if id == active {
                                ACCENT.color()
                            } else {
                                Color::Border.color()
                            })
                            .bg(Color::Panel.color())
                            .p(px(2.))
                            .cursor(gpui::CursorStyle::ResizeLeftRight)
                            .hover(|s| s.border_color(TEXT.color()))
                            .child(
                                div()
                                    .size_full()
                                    .rounded(px(2.))
                                    .overflow_hidden()
                                    .bg(gpui::checkerboard(Color::Checker.color(), 4.))
                                    .child(div().size_full().bg(stop.color)),
                            )
                            .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(hint.clone())).into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event, window, cx| {
                                    if style {
                                        this.begin_style_stop(Some(id), event, window, cx);
                                    } else {
                                        this.begin_fill_gradient_stop(
                                            Some(id),
                                            bounds.get(),
                                            event,
                                            window,
                                            cx,
                                        );
                                    }
                                }),
                            )
                    }),
            )
    }

    pub(super) fn reveal_gradient_stop(&self, cx: &gpui::App) {
        if let Some(index) = self.fill_state(cx).and_then(|(_, g)| {
            g.stops()
                .iter()
                .position(|s| s.id == self.inspector.active_stop)
        }) {
            self.inspector.gradient_stop_scroll.scroll_to_item(index);
        }
    }

    fn begin_fill_gradient_stop(
        &mut self,
        id: Option<usize>,
        bounds: gpui::Bounds<Pixels>,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let width = f32::from(bounds.size.width);
        if self.gesture.is_some() || width <= 0. {
            return;
        }
        let Some((_, gradient)) = self.fill_state(cx) else {
            return;
        };
        let position = (f32::from(event.position.x - bounds.left()) / width).clamp(0., 1.);
        let id = id.or_else(|| {
            gradient
                .stops()
                .iter()
                .filter(|s| (s.position - position).abs() * width < 8.)
                .min_by(|a, b| {
                    (a.position - position)
                        .abs()
                        .total_cmp(&(b.position - position).abs())
                })
                .map(|s| s.id)
        });
        self.seal_text_edits(cx);
        self.history.borrow_mut().begin_preview();
        let inserted = id.is_none();
        let id = id.unwrap_or_else(|| {
            self.mutate_gradient(|g| g.add_stop_at(position), cx)
                .flatten()
                .unwrap()
        });
        let original = gradient.stop(id).map_or(position, |s| s.position);
        self.inspector.active_stop = id;
        self.sync_fields(cx);
        self.reveal_gradient_stop(cx);
        self.begin(
            GestureKind::FillGradientStop {
                id,
                original,
                width,
                inserted,
            },
            event.position,
            event.button,
            window,
            cx,
        );
    }

    pub(super) fn move_fill_gradient_stop(
        &mut self,
        id: usize,
        position: f32,
        cx: &mut Context<Self>,
    ) {
        if self
            .fill_state(cx)
            .and_then(|(_, g)| g.stop(id).map(|s| s.position))
            == Some(position)
        {
            return;
        }
        self.mutate_gradient(|g| g.set_position(id, position), cx);
        self.sync_fields(cx);
        self.reveal_gradient_stop(cx);
        cx.notify();
    }

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
                    .text_color(if active { ACCENT.color() } else { MUTED.color() })
                    .hover(|s| s.text_color(TEXT.color()))
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
            .bg(Color::Input.color())
            .text_size(px(11.))
            .text_color(MUTED.color())
            .hover(|s| s.bg(Color::Selected.color()).text_color(TEXT.color()))
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .child(t("gradient-seam-width"))
            .child(
                div()
                    .text_color(TEXT.color())
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
                        this.set_gradient_seam(
                            style,
                            crate::scene::artboard::default_seam_width(),
                            cx,
                        );
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
