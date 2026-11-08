use super::*;
use gpui::{PathBuilder, canvas};

pub(super) struct Connection {
    source: usize,
    trigger: Trigger,
    end: Point<f32>,
    target: Option<(usize, Action)>,
}

impl Workspace {
    fn connection_frame(&self, id: usize, index: &scene_index::Index) -> usize {
        if index.boards.contains_key(&id) {
            return id;
        }
        let board = self
            .hierarchy
            .groups
            .get(&id)
            .and_then(|group| group.board)
            .or_else(|| self.object_info(id).and_then(|(board, _, _)| board));
        if let Some(board) = board.filter(|board| index.boards.contains_key(board)) {
            return board;
        }
        self.ancestors(id)
            .into_iter()
            .rev()
            .find(|id| index.boards.contains_key(id))
            .unwrap_or(id)
    }

    fn connection_target(
        &self,
        source: usize,
        world: Point<f32>,
        scroll_to: bool,
        property: Option<(
            &crate::scene::presentation::property::Property,
            &crate::document::Page,
        )>,
    ) -> Option<(usize, Action)> {
        let variants = self.prototype_variants(source);
        let index = self.scene_index();
        index.order.iter().copied().rev().find_map(|id| {
            if id == source {
                return None;
            }
            let rect = self.indexed_world_bounds(id, &index)?;
            if world.x < rect.x
                || world.y < rect.y
                || world.x > rect.x + rect.width
                || world.y > rect.y + rect.height
            {
                return None;
            }
            if scroll_to {
                return self
                    .ancestors(id)
                    .iter()
                    .any(|parent| {
                        self.hierarchy
                            .interactions
                            .get(parent)
                            .is_some_and(|i| !i.scroll.is_none())
                    })
                    .then_some((id, Action::ScrollTo { target: id }));
            }
            if let Some((property, page)) = property {
                return property.supports(page, id).then(|| {
                    (
                        id,
                        Action::SetProperty {
                            target: id,
                            property: property.clone(),
                        },
                    )
                });
            }
            if let Some(binding) = self.hierarchy.components.get(&id)
                && binding.master
                && let Ok(target) = uuid::Uuid::parse_str(&binding.component)
                && variants.iter().any(|(id, _)| *id == target)
            {
                return Some((id, Action::ChangeVariant { target }));
            }
            if self
                .shapes
                .iter()
                .any(|s| s.id == id && s.kind.is_playable())
            {
                return Some((
                    id,
                    Action::Media {
                        target: id,
                        command: MediaCommand::Toggle,
                    },
                ));
            }
            self.boards
                .iter()
                .any(|b| b.id == id)
                .then_some((id, Action::Navigate { target: id }))
        })
    }

    pub(in crate::editor) fn move_connection(&mut self, position: Point<Pixels>, cx: &gpui::App) {
        let Some(connection) = &self.presentation.connection else {
            return;
        };
        let source = connection.source;
        let scroll_to = self
            .prototype_action(source, connection.trigger)
            .is_some_and(|a| matches!(a, Action::ScrollTo { .. }));
        let property = self
            .prototype_action(source, connection.trigger)
            .and_then(|action| match action {
                Action::SetProperty { property, .. } => Some(property),
                _ => None,
            });
        let page = property.as_ref().map(|_| self.snapshot_page_state(cx));
        let end = self.board_point(None, position);
        let (left, right) = self.canvas_insets();
        let bounds = self.bounds.get();
        let local = position - bounds.origin;
        let over_canvas = bounds.contains(&position)
            && f32::from(local.x) >= left
            && f32::from(local.x) <= f32::from(bounds.size.width) - right;
        let target = over_canvas
            .then(|| {
                self.connection_target(source, end, scroll_to, property.as_ref().zip(page.as_ref()))
            })
            .flatten();
        if let Some(connection) = &mut self.presentation.connection {
            connection.end = end;
            connection.target = target;
        }
    }

    pub(in crate::editor) fn finish_connection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(connection) = self.presentation.connection.take()
            && let Some((_, action)) = connection.target
        {
            let action = match (
                action,
                self.prototype_action(connection.source, connection.trigger),
            ) {
                (Action::Navigate { target }, Some(Action::OpenOverlay { settings, .. })) => {
                    Action::OpenOverlay { target, settings }
                }
                (Action::Media { target, .. }, Some(Action::Media { command, .. })) => {
                    Action::Media {
                        target,
                        command: if self
                            .shapes
                            .iter()
                            .any(|s| s.id == target && command.supports(s.kind))
                        {
                            command
                        } else {
                            MediaCommand::Toggle
                        },
                    }
                }
                (action, _) => action,
            };
            self.set_prototype_action(
                connection.source,
                connection.trigger,
                Some(action),
                window,
                cx,
            );
        }
    }

    pub(in crate::editor) fn cancel_connection(&mut self) {
        self.presentation.connection = None;
    }

    fn begin_connection(
        &mut self,
        source: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_read_only() || !self.presentation.tab || !self.layer_editable(source) {
            return;
        }
        self.suspend(window, cx);
        self.presentation.connection = Some(Connection {
            source,
            trigger: self.presentation.trigger,
            end: self.board_point(None, event.position),
            target: None,
        });
        self.begin(
            GestureKind::PrototypeConnection,
            event.position,
            event.button,
            window,
            cx,
        );
    }

    fn connection_target_id(&self, action: &Action) -> Option<usize> {
        match action {
            Action::Navigate { target }
            | Action::OpenOverlay { target, .. }
            | Action::Media { target, .. }
            | Action::ScrollTo { target }
            | Action::SetProperty { target, .. } => Some(*target),
            Action::ChangeVariant { target } => {
                let target = target.to_string();
                self.hierarchy
                    .components
                    .iter()
                    .find(|(_, b)| b.master && b.component == target)
                    .map(|(id, _)| *id)
            }
            Action::Back | Action::CloseOverlay | Action::Multiple { .. } => None,
        }
    }

    pub(in crate::editor) fn prototype_connections(
        &self,
        index: &scene_index::Index,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.presentation.tab || self.preview.is_some() || self.preview_read_only() {
            return None;
        }
        let mut wires = Vec::new();
        let selected = self.selection_ids();
        let focused = !selected.is_empty();
        let scope = if focused {
            self.descendants(&selected)
        } else {
            BTreeSet::new()
        };
        let overview = !focused && self.view.zoom < 0.25;
        let mut seen = BTreeSet::new();
        let screen = |p| self.view.screen(p);
        let port = |r: Rect| screen(point(r.x + r.width, r.y + r.height / 2.));
        let end_point = |from: Point<f32>, r: Rect| {
            let left = screen(point(r.x, r.y + r.height / 2.));
            if left.x >= from.x {
                (left, -1.)
            } else {
                (screen(point(r.x + r.width, r.y + r.height / 2.)), 1.)
            }
        };
        for (source, interaction) in &self.hierarchy.interactions {
            if !index.included.contains(source) {
                continue;
            }
            let start = if overview {
                self.connection_frame(*source, index)
            } else {
                *source
            };
            let Some(rect) = self.indexed_world_bounds(start, index) else {
                continue;
            };
            let from = port(rect);
            let paired = interaction.click.is_some() && interaction.hover.is_some();
            for (trigger, action) in [
                (Trigger::Click, &interaction.click),
                (Trigger::Hover, &interaction.hover),
            ] {
                let Some(action) = action else {
                    continue;
                };
                for action in action.steps() {
                    if self
                        .presentation
                        .connection
                        .as_ref()
                        .is_some_and(|c| c.source == *source && c.trigger == trigger)
                    {
                        continue;
                    }
                    let target = self.connection_target_id(action);
                    if focused
                        && !scope.contains(source)
                        && !target.is_some_and(|id| scope.contains(&id))
                    {
                        continue;
                    }
                    let target = target.map(|id| {
                        if overview {
                            self.connection_frame(id, index)
                        } else {
                            id
                        }
                    });
                    if (overview && target == Some(start) && index.boards.contains_key(&start))
                        || !seen.insert((start, target, trigger == Trigger::Hover))
                    {
                        continue;
                    }
                    let (to, direction) = target
                        .and_then(|id| self.indexed_world_bounds(id, index))
                        .map_or((from + point(64., -32.), -1.), |r| end_point(from, r));
                    let active = scope.contains(source) && self.presentation.trigger == trigger;
                    let offset = if paired {
                        if trigger == Trigger::Click { -24. } else { 24. }
                    } else {
                        0.
                    };
                    wires.push((from, to, direction, trigger, active, offset));
                }
            }
        }
        if let Some(c) = &self.presentation.connection
            && let Some(rect) = self.indexed_world_bounds(c.source, index)
        {
            let from = port(rect);
            let (to, direction) = c
                .target
                .as_ref()
                .and_then(|(id, _)| self.indexed_world_bounds(*id, index))
                .map_or((screen(c.end), -1.), |r| end_point(from, r));
            wires.push((from, to, direction, c.trigger, true, 0.));
        }
        let line_count = wires.len();
        let mut overlay = div()
            .debug_selector(move || format!("prototype-wires-{line_count}"))
            .absolute()
            .inset_0()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        for (from, to, direction, trigger, active, offset) in &wires {
                            let from = from.map(px) + bounds.origin;
                            let to = to.map(px) + bounds.origin;
                            let bend = ((to.x - from.x).abs() * 0.5).clamp(px(40.), px(160.));
                            let first = from + point(bend, px(*offset));
                            let last = to + point(bend * *direction, px(*offset));
                            let left = from.x.min(to.x).min(first.x).min(last.x) - px(9.);
                            let right = from.x.max(to.x).max(first.x).max(last.x) + px(9.);
                            let top = from.y.min(to.y).min(first.y).min(last.y) - px(9.);
                            let bottom = from.y.max(to.y).max(first.y).max(last.y) + px(9.);
                            let clip = window.content_mask().bounds;
                            if right < clip.left()
                                || left > clip.right()
                                || bottom < clip.top()
                                || top > clip.bottom()
                            {
                                continue;
                            }
                            let mut line = PathBuilder::stroke(px(if *active { 2. } else { 1.5 }));
                            if *trigger == Trigger::Hover {
                                line = line.dash_array(&[px(5.), px(4.)]);
                            }
                            line.move_to(from);
                            line.cubic_bezier_to(to, first, last);
                            line.move_to(to + point(px(7. * *direction), px(-4.)));
                            line.line_to(to);
                            line.line_to(to + point(px(7. * *direction), px(4.)));
                            let color = if *trigger == Trigger::Click {
                                ACCENT.color()
                            } else {
                                TEXT.color()
                            };
                            if let Ok(path) = line.build() {
                                window.paint_path(
                                    path,
                                    color.opacity(if *active { 1. } else { 0.55 }),
                                );
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        if let Some(rect) = self
            .presentation
            .connection
            .as_ref()
            .and_then(|c| c.target.as_ref())
            .and_then(|(id, _)| self.indexed_world_bounds(*id, index))
        {
            let p = screen(point(rect.x, rect.y));
            overlay = overlay.child(
                div()
                    .debug_selector(|| "prototype-target".into())
                    .absolute()
                    .left(px(p.x))
                    .top(px(p.y))
                    .w(px(rect.width * self.view.zoom))
                    .h(px(rect.height * self.view.zoom))
                    .border_2()
                    .border_color(ACCENT.color()),
            );
        }
        if selected.len() == 1
            && self.gesture.is_none()
            && let Some(source) = selected
                .first()
                .copied()
                .filter(|id| self.layer_editable(*id))
            && let Some(rect) = self.indexed_world_bounds(source, index)
        {
            let p = port(rect);
            overlay = overlay.child(
                div()
                    .id("prototype-port")
                    .debug_selector(|| "prototype-port".into())
                    .absolute()
                    .left(px(p.x - 8.))
                    .top(px(p.y - 8.))
                    .size(px(16.))
                    .rounded_full()
                    .bg(ACCENT.color())
                    .border_2()
                    .border_color(PANEL.color())
                    .cursor_crosshair()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            this.begin_connection(source, event, window, cx)
                        }),
                    ),
            );
        }
        Some(overlay.into_any_element())
    }
}
