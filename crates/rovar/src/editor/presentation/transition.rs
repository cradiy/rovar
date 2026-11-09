use super::*;
use crate::scene::presentation::{TransitionEffect, WipeDirection};
use gpui::{App, RenderOnce};
use std::cell::RefCell;
use uic::components::dropdown::{DropdownPlacement, dropdown};
mod surface;
pub(super) use surface::Surface;
mod controls;
pub(super) use controls::Controls;
mod effects;

pub(super) fn kind(transition: Transition) -> gpui_effects::TransitionKind {
    use gpui_effects::TransitionKind;
    match transition {
        Transition::Instant | Transition::Dissolve { .. } => TransitionKind::CrossFade,
        Transition::Blur { .. } => TransitionKind::BlurFade,
        Transition::Wipe { direction, .. } => match direction {
            WipeDirection::Left => TransitionKind::WipeLeft,
            WipeDirection::Right => TransitionKind::WipeRight,
            WipeDirection::Up => TransitionKind::WipeUp,
            WipeDirection::Down => TransitionKind::WipeDown,
        },
        Transition::Effect { .. } => TransitionKind::Dissolve,
    }
}

/// A render revision whose outgoing contents are frozen before a variant changes.
/// Only the switch's visible revisions and an active hover restore retain it.
#[derive(Clone, Default)]
pub(super) struct Visual(Rc<RefCell<Option<Entity<Workspace>>>>);

impl PartialEq for Visual {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Visual {}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct FrameKey {
    pub board: usize,
    pub visual: Visual,
    pub overlay: Option<overlay::Overlay>,
}

#[derive(IntoElement)]
pub(super) struct Frame {
    pub workspace: gpui::WeakEntity<Workspace>,
    pub key: FrameKey,
}

impl RenderOnce for Frame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.workspace
            .update(cx, |this, cx| {
                let Some(playback) = &this.presentation.playback else {
                    return div();
                };
                let available = playback.stage.get().size;
                let frozen = (playback.visual != self.key.visual)
                    .then(|| self.key.visual.0.borrow().clone())
                    .flatten();
                if let Some(frozen) = frozen {
                    frozen.update(cx, |scene, cx| {
                        scene
                            .playback_frame_content(
                                self.key.board,
                                self.key.overlay,
                                available,
                                window,
                                cx,
                            )
                            .debug_selector(|| "playback-outgoing-snapshot".into())
                    })
                } else {
                    this.playback_frame_content(
                        self.key.board,
                        self.key.overlay,
                        available,
                        window,
                        cx,
                    )
                }
            })
            .unwrap_or_else(|_| div())
    }
}

impl Workspace {
    fn playback_frame_content(
        &mut self,
        board: usize,
        overlay: Option<overlay::Overlay>,
        available: gpui::Size<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(rect) = self.boards.iter().find(|b| b.id == board).map(|b| b.rect) else {
            return div();
        };
        let zoom = ((f32::from(available.width) - 32.).max(1.) / rect.width.max(1.))
            .min((f32::from(available.height) - 32.).max(1.) / rect.height.max(1.));
        let view = std::mem::replace(
            &mut self.view,
            Viewport {
                zoom,
                pan: point(-rect.x * zoom, -rect.y * zoom),
            },
        );
        let elements = self.content_elements_in_frame(window, cx, Some(board));
        let bars = self.playback_scrollbars(board);
        self.view = view;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .debug_selector(move || format!("playback-frame-{board}"))
                    .relative()
                    .flex_shrink_0()
                    .w(px(rect.width * zoom))
                    .h(px(rect.height * zoom))
                    .overflow_hidden()
                    .children(elements)
                    .children(bars)
                    .when_some(overlay, |el, overlay| {
                        let Some(overlay_rect) = self
                            .boards
                            .iter()
                            .find(|b| b.id == overlay.target)
                            .map(|b| b.rect)
                        else {
                            return el;
                        };
                        let (bounds, overlay_zoom) = overlay::geometry(
                            gpui::size(px(rect.width * zoom), px(rect.height * zoom)),
                            zoom,
                            overlay_rect,
                            overlay.settings.position,
                        );
                        let view = std::mem::replace(
                            &mut self.view,
                            Viewport {
                                zoom: overlay_zoom,
                                pan: point(
                                    -overlay_rect.x * overlay_zoom,
                                    -overlay_rect.y * overlay_zoom,
                                ),
                            },
                        );
                        let elements =
                            self.content_elements_in_frame(window, cx, Some(overlay.target));
                        let bars = self.playback_scrollbars(overlay.target);
                        self.view = view;
                        el.when(overlay.settings.backdrop, |el| {
                            el.child(div().absolute().inset_0().bg(Color::Overlay.color()))
                        })
                        .child(
                            div()
                                .debug_selector(|| "playback-overlay".into())
                                .absolute()
                                .left(px(bounds.x))
                                .top(px(bounds.y))
                                .w(px(bounds.width))
                                .h(px(bounds.height))
                                .overflow_hidden()
                                .children(elements)
                                .children(bars),
                        )
                    }),
            )
    }

    pub(super) fn freeze_playback_visual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = &self.presentation.playback else {
            return;
        };
        if p.transition.is_default()
            || !window.supports_subtree_effects()
            || p.visual.0.borrow().is_some()
        {
            return;
        }
        let visual = p.visual.clone();
        let page = self.snapshot_page_state(cx);
        let videos = self.media.videos.clone();
        let scene = cx.new(|cx| {
            let mut scene = Workspace::new(window, cx);
            scene.load_page(page, window, cx);
            scene.enable_preview(false);
            scene.media.videos = videos;
            scene.presentation.scroll = self.presentation.scroll.clone();
            scene.presentation.raised = self.presentation.raised.clone();
            for text in &scene.texts {
                text.editor.update(cx, |editor, _| editor.editing = false);
            }
            scene
        });
        *visual.0.borrow_mut() = Some(scene);
    }

    pub(super) fn restore_playback_visual(&mut self, visual: Visual) {
        // The restored revision is live again; its frozen copy is no longer needed.
        visual.0.borrow_mut().take();
        self.presentation.playback.as_mut().unwrap().visual = visual;
    }

    pub(in crate::editor) fn set_prototype_transition(
        &mut self,
        id: usize,
        trigger: Trigger,
        transition: Transition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_read_only()
            || !self.layer_editable(id)
            || !transition.valid()
            || !self
                .hierarchy
                .interactions
                .get(&id)
                .is_some_and(|i| i.transition(trigger) != transition && i.get(trigger).is_some())
        {
            return;
        }
        self.suspend(window, cx);
        let before = self.snapshot_hierarchy();
        self.hierarchy
            .interactions
            .get_mut(&id)
            .unwrap()
            .set_transition(trigger, transition);
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(super) fn prototype_transition(&self, source: usize, trigger: Trigger) -> Transition {
        self.hierarchy
            .interactions
            .get(&source)
            .filter(|i| i.get(trigger).is_some())
            .map_or(Transition::Instant, |i| i.transition(trigger))
    }

    pub(super) fn transition_controls(
        &self,
        id: usize,
        trigger: Trigger,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let current = self.prototype_transition(id, trigger);
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .pt(px(6.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t("prototype-transition")),
            )
            .children(
                [
                    (
                        "prototype-transitions-basic",
                        vec![
                            Transition::Instant,
                            Transition::dissolve(200),
                            Transition::blur(200),
                        ],
                    ),
                    (
                        "prototype-transitions-motion",
                        [
                            TransitionEffect::Slide,
                            TransitionEffect::Push,
                            TransitionEffect::ZoomIn,
                            TransitionEffect::ZoomOut,
                            TransitionEffect::Rotate,
                            TransitionEffect::Flip,
                        ]
                        .map(|effect| Transition::effect(effect, 200))
                        .to_vec(),
                    ),
                    (
                        "prototype-transitions-reveal",
                        vec![
                            Transition::wipe(WipeDirection::Right, 200),
                            Transition::effect(TransitionEffect::Iris, 200),
                            Transition::effect(TransitionEffect::Blinds, 200),
                            Transition::effect(TransitionEffect::Pixelate, 200),
                            Transition::effect(TransitionEffect::Noise, 200),
                        ],
                    ),
                ]
                .map(|(category, values)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .pt(px(4.))
                                .text_size(px(11.))
                                .text_color(MUTED.color())
                                .child(t(category)),
                        )
                        .child(div().flex().flex_wrap().gap(px(4.)).children(
                            values.into_iter().map(|value| {
                                let key = value.label();
                                let selected = current.label() == key;
                                div()
                                    .id(key)
                                    .debug_selector(move || key.into())
                                    .w(px(126.))
                                    .flex_shrink_0()
                                    .h(px(28.))
                                    .rounded(px(5.))
                                    .text_size(px(12.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .bg(if selected {
                                        Color::Selected.color()
                                    } else {
                                        Color::Input.color()
                                    })
                                    .child(t(key))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if !selected {
                                            this.commit_transition_duration(window, cx);
                                            let current = this.prototype_transition(id, trigger);
                                            let value = value
                                                .with_duration(current.duration_ms().unwrap_or(200))
                                                .with_easing(current.easing())
                                                .with_direction(
                                                    current.direction().unwrap_or_default(),
                                                );
                                            this.set_prototype_transition(
                                                id, trigger, value, window, cx,
                                            );
                                        }
                                    }))
                            }),
                        ))
                }),
            )
            .when_some(current.direction(), |el, direction| {
                el.child(
                    div().flex().gap(px(4.)).children(
                        [
                            ("prototype-wipe-left", WipeDirection::Left),
                            ("prototype-wipe-right", WipeDirection::Right),
                            ("prototype-wipe-up", WipeDirection::Up),
                            ("prototype-wipe-down", WipeDirection::Down),
                        ]
                        .map(|(key, value)| {
                            let selected = direction == value;
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
                                .bg(if selected {
                                    Color::Selected.color()
                                } else {
                                    Color::Input.color()
                                })
                                .child(t(key))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if !selected {
                                        this.commit_transition_duration(window, cx);
                                        let current = this.prototype_transition(id, trigger);
                                        this.set_prototype_transition(
                                            id,
                                            trigger,
                                            current.with_direction(value),
                                            window,
                                            cx,
                                        );
                                    }
                                }))
                        }),
                    ),
                )
            })
            .when(current.duration_ms().is_some(), |el| {
                el.child(self.transition_timing_controls(id, trigger, current, cx))
            })
    }
}
