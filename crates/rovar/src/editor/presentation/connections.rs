use super::*;
use gpui::{PathBuilder, canvas};

pub(super) struct Connection {
    source: usize,
    trigger: Trigger,
    end: Point<f32>,
    target: Option<(usize, Action)>,
}

impl Workspace {
    fn connection_target(&self, source: usize, world: Point<f32>) -> Option<(usize, Action)> {
        let variants = self.prototype_variants(source);
        self.canvas_layer_order().into_iter().rev().find_map(|id| {
            if id == source {
                return None;
            }
            let rect = self.world_bounds(id)?;
            if world.x < rect.x
                || world.y < rect.y
                || world.x > rect.x + rect.width
                || world.y > rect.y + rect.height
            {
                return None;
            }
            if let Some(binding) = self.hierarchy.components.get(&id)
                && binding.master
                && let Ok(target) = uuid::Uuid::parse_str(&binding.component)
                && variants.iter().any(|(id, _)| *id == target)
            {
                return Some((id, Action::ChangeVariant { target }));
            }
            self.boards
                .iter()
                .any(|b| b.id == id)
                .then_some((id, Action::Navigate { target: id }))
        })
    }

    pub(in crate::editor) fn move_connection(&mut self, position: Point<Pixels>) {
        let Some(connection) = &self.presentation.connection else {
            return;
        };
        let source = connection.source;
        let end = self.board_point(None, position);
        let (left, right) = self.canvas_insets();
        let bounds = self.bounds.get();
        let local = position - bounds.origin;
        let over_canvas = bounds.contains(&position)
            && f32::from(local.x) >= left
            && f32::from(local.x) <= f32::from(bounds.size.width) - right;
        let target = over_canvas
            .then(|| self.connection_target(source, end))
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

    fn connection_end(&self, action: Action) -> Option<Rect> {
        match action {
            Action::Navigate { target } => self.world_bounds(target),
            Action::ChangeVariant { target } => self
                .hierarchy
                .components
                .iter()
                .find(|(_, b)| b.master && b.component == target.to_string())
                .and_then(|(id, _)| self.world_bounds(*id)),
            Action::Back => None,
        }
    }

    pub(in crate::editor) fn prototype_connections(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.presentation.tab || self.preview.is_some() || self.preview_read_only() {
            return None;
        }
        let visible: BTreeSet<_> = self.canvas_layer_order().into_iter().collect();
        let mut wires = Vec::new();
        let selected = self.selection_ids();
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
            if !visible.contains(source) {
                continue;
            }
            let Some(rect) = self.world_bounds(*source) else {
                continue;
            };
            let from = port(rect);
            for trigger in [Trigger::Click, Trigger::Hover] {
                let Some(action) = interaction.get(trigger) else {
                    continue;
                };
                if self
                    .presentation
                    .connection
                    .as_ref()
                    .is_some_and(|c| c.source == *source && c.trigger == trigger)
                {
                    continue;
                }
                let (to, direction) = self
                    .connection_end(action)
                    .map_or((from + point(64., -32.), -1.), |r| end_point(from, r));
                wires.push((from, to, direction, trigger, selected.contains(source)));
            }
        }
        if let Some(c) = &self.presentation.connection
            && let Some(rect) = self.world_bounds(c.source)
        {
            let from = port(rect);
            let (to, direction) = c
                .target
                .and_then(|(id, _)| self.world_bounds(id))
                .map_or((screen(c.end), -1.), |r| end_point(from, r));
            wires.push((from, to, direction, c.trigger, true));
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
                        for (from, to, direction, trigger, active) in &wires {
                            let from = from.map(px) + bounds.origin;
                            let to = to.map(px) + bounds.origin;
                            let bend = ((to.x - from.x).abs() * 0.5).clamp(px(40.), px(160.));
                            let mut line = PathBuilder::stroke(px(if *active { 2. } else { 1.5 }));
                            line.move_to(from);
                            line.cubic_bezier_to(
                                to,
                                from + point(bend, px(0.)),
                                to + point(bend * *direction, px(0.)),
                            );
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
            .and_then(|c| c.target)
            .and_then(|(id, _)| self.world_bounds(id))
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
            && let Some(rect) = self.world_bounds(source)
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
