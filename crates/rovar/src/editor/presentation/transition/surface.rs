use super::*;
use crate::scene::presentation::Easing;
use gpui::{
    AnyElement, Element, ElementId, GlobalElementId, HitboxBehavior, InspectorElementId, LayoutId,
};
use std::time::Duration;
use web_time::Instant;

pub(in crate::editor::presentation) struct Surface {
    pub workspace: gpui::WeakEntity<Workspace>,
    pub key: FrameKey,
    pub transition: Transition,
}

struct Motion {
    from: f32,
    target: f32,
    started: Instant,
    duration: Duration,
    easing: Easing,
}

impl Motion {
    fn new(from: f32, target: f32, transition: Transition, now: Instant) -> Self {
        Self {
            from,
            target,
            started: now,
            duration: Duration::from_millis(transition.duration_ms().unwrap_or(0).into()),
            easing: transition.easing(),
        }
    }

    fn sample(&self, now: Instant) -> (f32, bool) {
        let travel = self.duration.as_secs_f32() * (self.target - self.from).abs();
        let elapsed = now.saturating_duration_since(self.started).as_secs_f32();
        if travel == 0. || elapsed >= travel {
            return (self.target, false);
        }
        (
            self.from + (self.target - self.from) * self.easing.sample(elapsed / travel),
            true,
        )
    }

    fn update(&mut self, target: f32, transition: Transition, now: Instant) -> (f32, bool) {
        let (current, _) = self.sample(now);
        let duration = Duration::from_millis(transition.duration_ms().unwrap_or(0).into());
        if self.target != target || self.duration != duration || self.easing != transition.easing()
        {
            *self = Self::new(current, target, transition, now);
        }
        self.sample(now)
    }
}

struct State {
    from: FrameKey,
    to: Option<FrameKey>,
    from_slot: usize,
    to_slot: usize,
    next_slot: usize,
    goal: f32,
    motion: Motion,
}

impl State {
    fn settle(&mut self, progress: f32) {
        if let Some(to) = self.to.take()
            && progress == 1.
        {
            self.from = to;
            self.from_slot = self.to_slot;
        }
    }

    fn update(
        &mut self,
        key: &FrameKey,
        transition: Transition,
        enabled: bool,
        now: Instant,
    ) -> (f32, bool) {
        if !enabled || transition.is_default() {
            if self.to.as_ref() == Some(key) {
                self.from_slot = self.to_slot;
            } else if &self.from != key {
                self.from_slot = self.next_slot;
                self.next_slot = self.next_slot.wrapping_add(1);
            }
            self.from = key.clone();
            self.to = None;
            self.motion = Motion::new(1., 1., transition, now);
            return (1., false);
        }
        let (progress, active) = self.motion.sample(now);
        if !active {
            self.settle(progress);
        }
        if let Some(to) = &self.to {
            if key == &self.from {
                self.goal = 0.;
            } else if key == to {
                self.goal = 1.;
            }
            let (progress, active) = self.motion.update(self.goal, transition, now);
            if active {
                return (progress, true);
            }
            self.settle(progress);
        }
        if key != &self.from {
            self.to = Some(key.clone());
            self.to_slot = self.next_slot;
            self.next_slot = self.next_slot.wrapping_add(1);
            self.goal = 1.;
            self.motion = Motion::new(0., 1., transition, now);
            (0., true)
        } else {
            (1., false)
        }
    }
}

impl IntoElement for Surface {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Surface {
    type RequestLayoutState = (AnyElement, bool);
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some("playback-transition".into())
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let now = cx.background_executor().now();
        let enabled = window.supports_subtree_effects();
        let (from, to, slots, progress, active) =
            window.with_element_state(id.unwrap(), |state: Option<State>, _| {
                let mut state = state.unwrap_or_else(|| State {
                    from: self.key.clone(),
                    to: None,
                    from_slot: 0,
                    to_slot: 1,
                    next_slot: 1,
                    goal: 1.,
                    motion: Motion::new(1., 1., self.transition, now),
                });
                let (progress, active) = state.update(&self.key, self.transition, enabled, now);
                (
                    (
                        state.from.clone(),
                        state.to.clone(),
                        [state.from_slot, state.to_slot],
                        progress,
                        active,
                    ),
                    state,
                )
            });
        if active {
            window.request_animation_frame();
        }
        let from = div().id(("revision", slots[0])).size_full().child(Frame {
            workspace: self.workspace.clone(),
            key: from,
        });
        let (to, progress) = if let Some(to) = to {
            (
                div()
                    .id(("revision", slots[1]))
                    .size_full()
                    .child(Frame {
                        workspace: self.workspace.clone(),
                        key: to,
                    })
                    .into_any_element(),
                progress,
            )
        } else {
            (div().into_any_element(), 0.)
        };
        let mut child = match self.transition {
            Transition::Effect {
                effect, direction, ..
            } if active && progress > 0. && progress < 1. && effect != TransitionEffect::Noise => {
                div()
                    .id("motion-transition")
                    .relative()
                    .size_full()
                    .child(effects::Blend::new(from, to, effect, direction, progress))
                    .into_any_element()
            }
            _ => gpui_effects::subtree_transition("transition", from, to)
                .size_full()
                .progress(progress)
                .kind(kind(self.transition))
                .into_any_element(),
        };
        (child.request_layout(window, cx), (child, active))
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        state.0.prepaint(window, cx);
        if state.1 {
            window.insert_hitbox(bounds, HitboxBehavior::BlockMouse);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        state.0.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_drive_progress_and_reverse_continuously_for_remaining_travel() {
        let now = Instant::now();
        for (easing, halfway) in [
            (Easing::Linear, 0.5),
            (Easing::EaseIn, 0.125),
            (Easing::EaseOut, 0.875),
            (Easing::EaseInOut, 0.5),
        ] {
            let transition = Transition::blur(400).with_easing(easing);
            let mut motion = Motion::new(0., 1., transition, now);
            let middle = now + Duration::from_millis(200);
            let (progress, active) = motion.sample(middle);
            assert!(active);
            assert!((progress - halfway).abs() < 0.00001);
            let (reversed, active) = motion.update(0., transition, middle);
            assert!(active);
            assert_eq!(reversed, progress);
            let half_return = Duration::from_secs_f32(0.2 * progress);
            let (returning, active) = motion.sample(middle + half_return);
            assert!(active);
            assert!((returning - progress * (1. - easing.sample(0.5))).abs() < 0.00001);
            assert_eq!(
                motion.sample(middle + half_return * 2 + Duration::from_millis(1)),
                (0., false)
            );
            assert_eq!(easing.sample(0.), 0.);
            assert_eq!(easing.sample(1.), 1.);
        }
    }
}
