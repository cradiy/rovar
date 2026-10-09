use super::*;
use gpui::{App, RenderOnce};
use std::cell::RefCell;
use uic::components::dropdown::{DropdownPlacement, dropdown};

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
                            .playback_frame_content(self.key.board, available, window, cx)
                            .debug_selector(|| "playback-outgoing-snapshot".into())
                    })
                } else {
                    this.playback_frame_content(self.key.board, available, window, cx)
                }
            })
            .unwrap_or_else(|_| div())
    }
}

impl Workspace {
    fn playback_frame_content(
        &mut self,
        board: usize,
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
                    .children(elements),
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
        let page = self.snapshot_page(cx).0;
        let videos = self.media.videos.clone();
        let scene = cx.new(|cx| {
            let mut scene = Workspace::new(window, cx);
            scene.load_page(page, window, cx);
            scene.enable_preview(false);
            scene.media.videos = videos;
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
            .child(
                div().flex().gap(px(4.)).children(
                    [
                        ("prototype-instant", Transition::Instant),
                        (
                            "prototype-dissolve",
                            Transition::Dissolve { duration_ms: 200 },
                        ),
                    ]
                    .map(|(key, value)| {
                        let selected = current.is_default() == value.is_default();
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
                                    this.set_prototype_transition(id, trigger, value, window, cx);
                                }
                            }))
                    }),
                ),
            )
            .when_some(
                match current {
                    Transition::Dissolve { duration_ms } => Some(duration_ms),
                    Transition::Instant => None,
                },
                |el, duration| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(MUTED.color())
                                    .child(t("prototype-duration")),
                            )
                            .child(
                                dropdown(&self.presentation.menus[3])
                                    .placement(DropdownPlacement::BottomStart)
                                    .w(px(140.))
                                    .p(px(5.))
                                    .rounded(px(10.))
                                    .bg(Color::Input.color())
                                    .border_1()
                                    .border_color(BORDER.color())
                                    .shadow_lg()
                                    .text_size(px(12.))
                                    .text_color(TEXT.color())
                                    .trigger(
                                        div()
                                            .id("prototype-duration")
                                            .debug_selector(|| "prototype-duration".into())
                                            .h(px(30.))
                                            .px(px(8.))
                                            .rounded(px(6.))
                                            .bg(Color::Input.color())
                                            .text_size(px(12.))
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .cursor_pointer()
                                            .child(format!("{duration} ms"))
                                            .child(icon(LucideIcons::ChevronDown, 12.)),
                                    )
                                    .menu(div().flex().flex_col().children(
                                        [100u16, 200, 300, 500, 1000].map(|ms| {
                                            div()
                                                .id(("prototype-duration-option", ms as usize))
                                                .debug_selector(move || {
                                                    format!("prototype-duration-{ms}")
                                                })
                                                .h(px(28.))
                                                .px(px(8.))
                                                .rounded(px(5.))
                                                .flex()
                                                .items_center()
                                                .gap(px(6.))
                                                .cursor_pointer()
                                                .hover(|s| s.bg(Color::Hover.color()))
                                                .when(ms == duration, |el| {
                                                    el.bg(Color::Selected.color())
                                                        .text_color(ACCENT.color())
                                                })
                                                .child(
                                                    icon(LucideIcons::Check, 12.).opacity(
                                                        if ms == duration { 1. } else { 0. },
                                                    ),
                                                )
                                                .child(format!("{ms} ms"))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.presentation.menus[3]
                                                            .update(cx, |menu, cx| {
                                                                menu.close(window, cx)
                                                            });
                                                        this.set_prototype_transition(
                                                            id,
                                                            trigger,
                                                            Transition::Dissolve {
                                                                duration_ms: ms,
                                                            },
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                ))
                                        }),
                                    )),
                            ),
                    )
                },
            )
    }
}
