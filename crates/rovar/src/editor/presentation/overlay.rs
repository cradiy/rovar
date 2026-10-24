use super::*;
use crate::scene::presentation::{OverlayPosition, OverlaySettings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Overlay {
    pub target: usize,
    pub settings: OverlaySettings,
}

pub(super) fn geometry(
    available: gpui::Size<Pixels>,
    zoom: f32,
    rect: Rect,
    position: OverlayPosition,
) -> (Rect, f32) {
    let width = f32::from(available.width).max(1.);
    let height = f32::from(available.height).max(1.);
    let scale = zoom
        .min(width / rect.width.max(1.))
        .min(height / rect.height.max(1.));
    let w = rect.width * scale;
    let h = rect.height * scale;
    (
        Rect {
            x: if position == OverlayPosition::Right {
                width - w
            } else {
                (width - w) / 2.
            },
            y: if position == OverlayPosition::Bottom {
                height - h
            } else {
                (height - h) / 2.
            },
            width: w,
            height: h,
        },
        scale,
    )
}

impl Workspace {
    pub(super) fn playback_coordinates(&self) -> (Viewport, Bounds<Pixels>) {
        let bounds = self.bounds.get();
        let overlay = self.presentation.playback.as_ref().and_then(|p| p.overlay);
        if let Some(overlay) = overlay
            && let Some(board) = self.boards.iter().find(|b| b.id == overlay.target)
        {
            let (rect, zoom) = geometry(
                bounds.size,
                self.view.zoom,
                board.rect,
                overlay.settings.position,
            );
            return (
                Viewport {
                    zoom,
                    pan: point(-board.rect.x * zoom, -board.rect.y * zoom),
                },
                Bounds::new(
                    bounds.origin + point(px(rect.x), px(rect.y)),
                    gpui::size(px(rect.width), px(rect.height)),
                ),
            );
        }
        (self.view, bounds)
    }

    pub(super) fn playback_world(&self, position: Point<Pixels>) -> Point<f32> {
        let (view, bounds) = self.playback_coordinates();
        view.world((position - bounds.origin).map(f32::from))
    }

    pub(super) fn dismiss_overlay(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(p) = &mut self.presentation.playback else {
            return false;
        };
        if p.overlay.is_none() {
            return false;
        }
        p.transition = p.overlay_transition;
        self.commit_playback_hover();
        self.playback_action(Action::CloseOverlay, cx);
        true
    }

    pub(super) fn overlay_outside_click(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(overlay) = self.presentation.playback.as_ref().and_then(|p| p.overlay) else {
            return false;
        };
        if self.playback_coordinates().1.contains(&position) {
            return false;
        }
        if overlay.settings.dismiss_outside {
            self.dismiss_overlay(cx);
        }
        true
    }

    pub(super) fn overlay_controls(
        &self,
        id: usize,
        trigger: Trigger,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Some(Action::OpenOverlay { target, settings }) = self
            .hierarchy
            .interactions
            .get(&id)
            .and_then(|i| i.get(trigger))
        else {
            return div();
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .pt(px(6.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t("prototype-overlay-position")),
            )
            .child(
                div().flex().gap(px(4.)).children(
                    [
                        ("prototype-overlay-center", OverlayPosition::Center),
                        ("prototype-overlay-bottom", OverlayPosition::Bottom),
                        ("prototype-overlay-right", OverlayPosition::Right),
                    ]
                    .map(|(key, position)| {
                        div()
                            .id(key)
                            .debug_selector(move || key.into())
                            .flex_1()
                            .h(px(28.))
                            .rounded(px(5.))
                            .text_size(px(12.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .bg(if position == settings.position {
                                Color::Selected.color()
                            } else {
                                Color::Input.color()
                            })
                            .child(t(key))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_prototype_action(
                                    id,
                                    trigger,
                                    Some(Action::OpenOverlay {
                                        target,
                                        settings: OverlaySettings {
                                            position,
                                            ..settings
                                        },
                                    }),
                                    window,
                                    cx,
                                );
                            }))
                    }),
                ),
            )
            .children(
                [
                    ("prototype-overlay-backdrop", settings.backdrop),
                    ("prototype-overlay-outside", settings.dismiss_outside),
                ]
                .map(|(key, checked)| {
                    div()
                        .id(key)
                        .debug_selector(move || key.into())
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.))
                        .cursor_pointer()
                        .child(
                            div()
                                .size(px(16.))
                                .border_1()
                                .border_color(BORDER.color())
                                .rounded(px(3.))
                                .when(checked, |el| {
                                    el.bg(Color::Selected.color()).child(
                                        icon(GpuiIcons::LucideCheck, 14.)
                                            .text_color(ACCENT.color()),
                                    )
                                }),
                        )
                        .child(t(key))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let mut settings = settings;
                            if key == "prototype-overlay-backdrop" {
                                settings.backdrop = !checked;
                            } else {
                                settings.dismiss_outside = !checked;
                            }
                            this.set_prototype_action(
                                id,
                                trigger,
                                Some(Action::OpenOverlay { target, settings }),
                                window,
                                cx,
                            );
                        }))
                }),
            )
    }
}
