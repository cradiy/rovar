use super::*;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) struct Press {
    start: Point<Pixels>,
    last: Point<Pixels>,
    action: Option<(usize, Action)>,
    containers: Vec<usize>,
    bar: Option<scroll::paint::Bar>,
    pub moved: bool,
}

pub(super) struct HitOrder {
    index: std::rc::Weak<scene_index::Index>,
    root: usize,
    layers: Vec<usize>,
    included: BTreeSet<usize>,
}

impl Workspace {
    pub(super) fn playback_target(&self, world: Point<f32>) -> Option<usize> {
        let playback = self.presentation.playback.as_ref()?;
        let root = playback.overlay.map_or(playback.current, |o| o.target);
        if !scroll::bounds(self.world_rect(root)?).contains(&world) {
            return None;
        }
        let index = self.scene_index();
        let cached = self
            .presentation
            .scroll
            .index
            .as_ref()
            .is_some_and(|old| Rc::ptr_eq(old, &index));
        let mut hit_order = playback.hit_order.borrow_mut();
        if hit_order
            .as_ref()
            .is_none_or(|order| order.root != root || order.index.as_ptr() != Rc::as_ptr(&index))
        {
            let layers: Vec<_> = index
                .order
                .iter()
                .copied()
                .filter(|id| *id == root || self.ancestors(*id).contains(&root))
                .collect();
            *hit_order = Some(HitOrder {
                index: Rc::downgrade(&index),
                root,
                included: layers.iter().copied().collect(),
                layers,
            });
        }
        let order = hit_order.as_ref().unwrap();
        let included = &order.included;
        order.layers.iter().rev().copied().find(|id| {
            let node = cached
                .then(|| self.presentation.scroll.nodes.get(id))
                .flatten();
            if self
                .hierarchy
                .interactions
                .get(id)
                .is_some_and(|i| i.pass_through)
                || index
                    .shapes
                    .get(id)
                    .is_some_and(|i| self.shapes[*i].kind == ShapeKind::Audio)
                || crate::scene::boolean::consumed(&self.hierarchy, *id, included)
                || crate::scene::mask::is_source(&self.hierarchy, *id, included)
            {
                return false;
            }
            if self.hierarchy.groups.contains_key(id)
                && !self.hierarchy.layouts.contains_key(id)
                && !crate::scene::boolean::is_boolean(&self.hierarchy, *id)
                && self
                    .hierarchy
                    .interactions
                    .get(id)
                    .is_none_or(|i| i.is_empty())
            {
                return false;
            }
            if self
                .presentation
                .scroll
                .nodes
                .get(id)
                .and_then(|n| n.clip)
                .is_some_and(|clip| !clip.contains(&world))
            {
                return false;
            }
            let Some(rect) = node
                .filter(|_| {
                    self.hierarchy.groups.get(id).is_none_or(|group| {
                        group.mask.is_none()
                            && group.boolean.is_none()
                            && !self.hierarchy.layouts.contains_key(id)
                    })
                })
                .map(|n| n.rect)
                .or_else(|| self.world_rect(*id))
            else {
                return false;
            };
            let local = world + self.playback_scroll_offset(*id);
            let p = crate::scene::rotation::around(
                local,
                crate::scene::rotation::center(rect),
                -node.map_or_else(|| self.object_rotation(*id), |n| n.rotation),
            );
            if !scroll::bounds(rect).contains(&p) {
                return false;
            }
            let mut masks = crate::scene::mask::ancestors(&self.hierarchy, *id, included);
            if crate::scene::mask::source(&self.hierarchy, *id).is_some() {
                masks.push(*id);
            }
            if !masks.into_iter().all(|group| {
                let source = crate::scene::mask::source(&self.hierarchy, group).unwrap();
                self.mask_outline(source)
                    .contains(world + self.playback_scroll_offset(source))
            }) {
                return false;
            }
            if crate::scene::boolean::is_boolean(&self.hierarchy, *id)
                || index
                    .shapes
                    .get(id)
                    .is_some_and(|i| crate::scene::boolean::supported(&self.shapes[*i]))
            {
                crate::scene::mask::outline(
                    &self.hierarchy,
                    &self.shapes,
                    &self.boards,
                    &mut self.boolean_cache.borrow_mut(),
                    *id,
                )
                .contains(local)
            } else {
                true
            }
        })
    }

    pub(super) fn playback_interactions(
        &self,
        hit: usize,
    ) -> impl Iterator<Item = (usize, &crate::scene::presentation::Interaction)> {
        std::iter::once(hit)
            .chain(self.ancestors(hit))
            .filter_map(|id| {
                let interaction = self.hierarchy.interactions.get(&id)?;
                (!interaction.pass_through).then_some((id, interaction))
            })
    }

    pub(super) fn playback_hit(
        &self,
        world: Point<f32>,
        trigger: Trigger,
    ) -> Option<(usize, Action)> {
        self.playback_action_at(self.playback_target(world), trigger)
    }

    fn playback_action_at(&self, hit: Option<usize>, trigger: Trigger) -> Option<(usize, Action)> {
        self.playback_interactions(hit?)
            .find_map(|(id, interaction)| interaction.get(trigger).map(|action| (id, action)))
    }

    pub(super) fn playback_press(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.presentation.scroll.press = None;
        if self.overlay_outside_click(position, cx) {
            return;
        }
        let world = self.playback_world(position);
        let hit = self.playback_target(world);
        self.presentation.scroll.press = Some(Press {
            start: position,
            last: position,
            action: self.playback_action_at(hit, Trigger::Click),
            containers: self.playback_scroll_containers(hit),
            bar: self.playback_scrollbar(world),
            moved: false,
        });
    }

    pub(super) fn playback_drag(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut press) = self.presentation.scroll.press.take() else {
            return false;
        };
        let distance = position - press.start;
        if distance.x.abs().max(distance.y.abs()) >= px(4.) {
            press.moved = true;
        }
        if press.moved {
            self.leave_playback_hover(window, cx);
            let zoom = self.playback_coordinates().0.zoom;
            let delta = (position - press.last).map(|v| f32::from(v) / zoom);
            let changed = if let Some(bar) = press.bar {
                self.scroll_by(
                    &[bar.container],
                    if bar.vertical {
                        point(0., delta.y * bar.scale)
                    } else {
                        point(delta.x * bar.scale, 0.)
                    },
                )
            } else {
                self.scroll_by(&press.containers, -delta)
            };
            if changed {
                self.playback_hover(false, cx);
                cx.notify();
            }
        }
        press.last = position;
        self.presentation.scroll.press = Some(press);
        true
    }

    pub(super) fn playback_release(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(press) = self.presentation.scroll.press.take() else {
            return;
        };
        let distance = position - press.start;
        if press.moved
            || distance.x.abs().max(distance.y.abs()) >= px(4.)
            || press.bar.is_some()
            || !self.playback_coordinates().1.contains(&position)
        {
            return;
        }
        let world = self.playback_world(position);
        if let Some((source, action)) = press.action
            && self.playback_hit(world, Trigger::Click) == Some((source, action.clone()))
        {
            self.playback_click(source, action, window, cx);
        }
    }
}
