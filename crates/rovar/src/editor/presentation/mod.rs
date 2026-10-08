//! Authoring and isolated playback of page-local frame navigation.

use super::*;
use crate::{
    i18n::t,
    scene::presentation::{Action, HoverExit, Trigger},
    ui::theme::Color,
};
use std::collections::BTreeSet;
use uic::components::dropdown::DropdownState;
mod chrome;
mod connections;
mod controls;
mod hover_exit;
#[cfg(test)]
mod tests;
mod variants;

pub(super) struct State {
    pub tab: bool,
    trigger: Trigger,
    connection: Option<connections::Connection>,
    pub player: Option<Entity<Workspace>>,
    pub playback: Option<Playback>,
    menus: [Entity<DropdownState>; 3],
}

pub(super) struct Playback {
    owner: gpui::WeakEntity<Workspace>,
    pub current: usize,
    start: usize,
    history: Vec<usize>,
    hotspot_hovered: bool,
    entered: Option<usize>,
    hover: Option<variants::Hover>,
    initial_page: crate::document::Page,
    stage: Rc<Cell<Bounds<Pixels>>>,
    window_size: gpui::Size<Pixels>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        Self {
            tab: false,
            trigger: Trigger::Click,
            connection: None,
            player: None,
            playback: None,
            menus: std::array::from_fn(|_| cx.new(|cx| DropdownState::new(window, cx))),
        }
    }
}

impl Workspace {
    pub(super) fn set_presentation_tab(
        &mut self,
        tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.suspend(window, cx);
        self.presentation.tab = tab;
        if tab {
            self.choose_tool(toolbar::Tool::Move, window, cx);
        }
        cx.notify();
    }

    pub(super) fn presentation_start(&self) -> Option<usize> {
        let visible = |id: &usize| self.boards.iter().any(|b| b.id == *id && !b.layer.hidden);
        self.hierarchy
            .start
            .filter(visible)
            .or_else(|| self.selected.filter(visible))
            .or_else(|| self.canvas_layer_order().into_iter().find(visible))
    }

    pub(super) fn start_presentation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_read_only() || self.presentation_start().is_none() {
            return;
        }
        self.suspend(window, cx);
        self.sync_components(window, cx);
        self.reflow_layout(cx);
        let start = self.presentation_start().unwrap();
        let (mut page, _) = self.snapshot_page(cx);
        for binding in page.hierarchy.components.values_mut() {
            binding.master = false;
        }
        let initial_page = page.clone();
        let definitions = self.components.definitions.clone();
        let sets = self.components.sets.clone();
        let owner = cx.entity().downgrade();
        let player = cx.new(|cx| {
            let mut player = Workspace::new(window, cx);
            player.load_page(page, window, cx);
            player.components.definitions = definitions;
            player.components.sets = sets;
            player.set_selection(BTreeSet::new(), cx);
            for text in &player.texts {
                text.editor.update(cx, |editor, _| editor.editing = false);
            }
            player.presentation.playback = Some(Playback {
                owner,
                current: start,
                start,
                history: Vec::new(),
                hotspot_hovered: false,
                entered: None,
                hover: None,
                initial_page,
                stage: Rc::new(Cell::new(Bounds::default())),
                window_size: window.viewport_size(),
            });
            player
        });
        player.read(cx).focus.clone().focus(window, cx);
        self.presentation.player = Some(player);
        cx.notify();
    }

    fn stop_presentation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(player) = self.presentation.player.take() {
            player.update(cx, |player, cx| player.pause_videos(cx));
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn presentation_view(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(WORKSPACE.color())
            .text_color(TEXT.color())
            .font_family(crate::ui::font::family(cx))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.stop_presentation(window, cx);
                }
                // Keep editing shortcuts out of the isolated playback scene.
                cx.stop_propagation();
                window.prevent_default();
            }))
            .child(self.presentation.player.as_ref().unwrap().clone())
    }

    fn playback_action(&mut self, action: Action, cx: &mut Context<Self>) {
        if let Action::Navigate { target } = action
            && !self
                .boards
                .iter()
                .any(|b| b.id == target && !b.layer.hidden)
        {
            return;
        }
        let Some(playback) = &mut self.presentation.playback else {
            return;
        };
        match action {
            Action::Navigate { target } if target != playback.current => {
                playback.history.push(playback.current);
                playback.current = target;
            }
            Action::Back => {
                if let Some(previous) = playback.history.pop() {
                    playback.current = previous;
                }
            }
            _ => return,
        }
        playback.hotspot_hovered = false;
        playback.entered = None;
        self.pause_videos(cx);
        cx.notify();
    }

    fn playback_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if let Some(playback) = &mut self.presentation.playback
            && playback.hotspot_hovered != hovered
        {
            playback.hotspot_hovered = hovered;
            cx.notify();
        }
    }

    fn playback_hit(&self, world: Point<f32>, trigger: Trigger) -> Option<(usize, Action)> {
        let current = self.presentation.playback.as_ref()?.current;
        let rect = self.boards.iter().find(|b| b.id == current)?.rect;
        if world.x < rect.x
            || world.y < rect.y
            || world.x > rect.x + rect.width
            || world.y > rect.y + rect.height
        {
            return None;
        }
        let order = self.canvas_layer_order();
        let included = order.iter().copied().collect();
        // Hit regions follow stacking. Labels without actions can sit above a button.
        // Editing locks do not disable click actions.
        let hit = order.into_iter().rev().find(|id| {
            self.hierarchy
                .interactions
                .get(id)
                .and_then(|i| i.get(trigger))
                .is_some()
                && (*id == current || self.ancestors(*id).contains(&current))
                && !crate::scene::boolean::consumed(&self.hierarchy, *id, &included)
                && !crate::scene::mask::is_source(&self.hierarchy, *id, &included)
                && self.mask_hit(*id, world)
                && if crate::scene::boolean::is_boolean(&self.hierarchy, *id)
                    || self
                        .shapes
                        .iter()
                        .any(|s| s.id == *id && crate::scene::boolean::supported(s))
                {
                    crate::scene::mask::outline(
                        &self.hierarchy,
                        &self.shapes,
                        &self.boards,
                        &mut self.boolean_cache.borrow_mut(),
                        *id,
                    )
                    .contains(world)
                } else {
                    true
                }
                && self.world_rect(*id).is_some_and(|rect| {
                    let p = crate::scene::rotation::around(
                        world,
                        crate::scene::rotation::center(rect),
                        -self.object_rotation(*id),
                    );
                    p.x >= rect.x
                        && p.y >= rect.y
                        && p.x <= rect.x + rect.width
                        && p.y <= rect.y + rect.height
                })
        })?;
        self.hierarchy
            .interactions
            .get(&hit)?
            .get(trigger)
            .map(|action| (hit, action))
    }

    pub(super) fn playback_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        if self.presentation.playback.as_ref().unwrap().window_size != window.viewport_size() {
            self.leave_playback_hover(window, cx);
            self.playback_hover(false, cx);
            self.presentation.playback.as_mut().unwrap().window_size = window.viewport_size();
        }
        let playback = self.presentation.playback.as_ref().unwrap();
        let board = self
            .boards
            .iter()
            .find(|b| b.id == playback.current)
            .unwrap();
        let rect = board.rect;
        let cursor = if playback.hotspot_hovered {
            gpui::CursorStyle::PointingHand
        } else {
            gpui::CursorStyle::Arrow
        };
        let stage = playback.stage.clone();
        let available = stage.get().size;
        let zoom = ((f32::from(available.width) - 32.).max(1.) / rect.width.max(1.))
            .min((f32::from(available.height) - 32.).max(1.) / rect.height.max(1.));
        self.view = Viewport {
            zoom,
            pan: point(-rect.x * zoom, -rect.y * zoom),
        };
        self.load_visible_media(window, cx);
        let weak = cx.entity().downgrade();
        let pointer = weak.clone();
        let frame_bounds = self.bounds.clone();
        div()
            .on_paint_before_children(move |_, _, window, _| {
                let moving = pointer.clone();
                window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture {
                        return;
                    }
                    let _ = moving.update(cx, |this, cx| {
                        if this
                            .presentation
                            .playback
                            .as_ref()
                            .is_some_and(|p| p.hover.is_some())
                        {
                            this.playback_pointer(event.position, window, cx);
                            cx.stop_propagation();
                        }
                    });
                });
                let leaving = pointer.clone();
                window.on_mouse_event(move |_: &gpui::MouseExitEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture {
                        return;
                    }
                    let _ = leaving.update(cx, |this, cx| {
                        this.playback_hover(false, cx);
                        this.leave_playback_hover(window, cx);
                    });
                });
            })
            .id("playback")
            .debug_selector(|| "playback".into())
            .track_focus(&self.focus)
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .child(self.playback_toolbar(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_paint_before_children(move |bounds, _, _, cx| {
                        if stage.replace(bounds) != bounds {
                            let _ = weak.update(cx, |_, cx| cx.notify());
                        }
                    })
                    .child(
                        div()
                            .relative()
                            .flex_shrink_0()
                            .w(px(rect.width * zoom))
                            .h(px(rect.height * zoom))
                            .overflow_hidden()
                            .on_paint_before_children(move |bounds, _, _, _| {
                                frame_bounds.set(bounds)
                            })
                            .children(self.content_elements(window, cx))
                            .child(
                                div()
                                    .id("playback-surface")
                                    .debug_selector(|| "playback-surface".into())
                                    .absolute()
                                    .inset_0()
                                    .occlude()
                                    .cursor(cursor)
                                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                                    .on_mouse_move(cx.listener(
                                        |this, event: &gpui::MouseMoveEvent, window, cx| {
                                            this.playback_pointer(event.position, window, cx);
                                        },
                                    ))
                                    .on_click(cx.listener(
                                        |this, event: &gpui::ClickEvent, window, cx| {
                                            let world = this.board_point(None, event.position());
                                            if let Some((source, action)) =
                                                this.playback_hit(world, Trigger::Click)
                                            {
                                                this.playback_click(source, action, window, cx);
                                            }
                                            cx.stop_propagation();
                                        },
                                    )),
                            ),
                    ),
            )
    }
}
