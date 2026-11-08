use super::*;
use crate::scene::presentation::ScrollAxis;
use std::collections::HashMap;

mod controls;
pub(super) mod paint;
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(in crate::editor) struct State {
    pub active: bool,
    pub scrolling: bool,
    pub offsets: HashMap<usize, Point<f32>>,
    pub nodes: HashMap<usize, Node>,
    pub(in crate::editor::presentation) press: Option<input::Press>,
    pub(in crate::editor::presentation) index: Option<Rc<super::super::scene_index::Index>>,
    settings: std::collections::BTreeMap<usize, (ScrollAxis, bool)>,
    pub(in crate::editor::presentation) dirty: bool,
}

#[derive(Clone)]
pub(in crate::editor) struct Node {
    pub rect: Rect,
    pub offset: Point<f32>,
    pub clip: Option<Bounds<f32>>,
    pub range: Option<Range>,
    pub(in crate::editor::presentation) rotation: f32,
    pub(in crate::editor::presentation) ancestors: Vec<usize>,
}

#[derive(Clone, Copy)]
pub(in crate::editor) struct Range {
    pub min: Point<f32>,
    pub max: Point<f32>,
    pub axis: ScrollAxis,
}

pub(super) fn bounds(rect: Rect) -> Bounds<f32> {
    Bounds::new(point(rect.x, rect.y), gpui::size(rect.width, rect.height))
}

impl Node {
    pub fn displayed(&self) -> Bounds<f32> {
        let mut rect = bounds(self.rect);
        rect.origin -= self.offset;
        rect
    }
}

impl Workspace {
    pub(in crate::editor) fn refresh_playback_scroll(&mut self) {
        if !self.presentation.scroll.active {
            return;
        }
        let index = self.scene_index();
        let settings = self
            .hierarchy
            .interactions
            .iter()
            .filter(|(_, i)| !i.scroll.is_none() || i.fixed)
            .map(|(id, i)| (*id, (i.scroll, i.fixed)))
            .collect();
        let geometry_changed = !self
            .presentation
            .scroll
            .index
            .as_ref()
            .is_some_and(|old| Rc::ptr_eq(old, &index))
            || self.presentation.scroll.settings != settings;
        if !geometry_changed && !self.presentation.scroll.dirty {
            return;
        }
        self.presentation.scroll.dirty = false;
        self.presentation.scroll.scrolling = settings.values().any(|(axis, _)| !axis.is_none());
        let order = index.order.clone();
        let mut nodes: HashMap<_, _> = if geometry_changed {
            order
                .iter()
                .filter_map(|id| {
                    Some((
                        *id,
                        Node {
                            rect: if self.hierarchy.groups.get(id).is_some_and(|group| {
                                group.mask.is_none()
                                    && group.boolean.is_none()
                                    && !self.hierarchy.layouts.contains_key(id)
                            }) {
                                index.groups.get(id).copied()?
                            } else {
                                self.world_rect(*id)?
                            },
                            offset: point(0., 0.),
                            clip: None,
                            rotation: self.object_rotation(*id),
                            ancestors: self.ancestors(*id),
                            range: self
                                .hierarchy
                                .interactions
                                .get(id)
                                .filter(|i| !i.scroll.is_none())
                                .map(|i| Range {
                                    min: point(0., 0.),
                                    max: point(0., 0.),
                                    axis: i.scroll,
                                }),
                        },
                    ))
                })
                .collect()
        } else {
            std::mem::take(&mut self.presentation.scroll.nodes)
        };
        // Nested viewports contribute their own frame, not their overflowing content.
        if geometry_changed {
            for id in &order {
                if self.hierarchy.groups.contains_key(id)
                    && !self.hierarchy.layouts.contains_key(id)
                    && !crate::scene::boolean::is_boolean(&self.hierarchy, *id)
                {
                    continue;
                }
                let Some(node) = nodes.get(id) else {
                    continue;
                };
                let extent = crate::scene::rotation::bounds(node.rect, node.rotation);
                let ancestors = node.ancestors.clone();
                let mut fixed = self.hierarchy.interactions.get(id).is_some_and(|i| i.fixed);
                for parent in ancestors {
                    if let Some(container) = nodes.get_mut(&parent)
                        && let Some(range) = &mut container.range
                    {
                        if !fixed {
                            range.min.x = range.min.x.min(extent.x - container.rect.x);
                            range.min.y = range.min.y.min(extent.y - container.rect.y);
                            range.max.x = range.max.x.max(
                                extent.x + extent.width - container.rect.x - container.rect.width,
                            );
                            range.max.y = range.max.y.max(
                                extent.y + extent.height - container.rect.y - container.rect.height,
                            );
                        }
                        break;
                    }
                    fixed |= self
                        .hierarchy
                        .interactions
                        .get(&parent)
                        .is_some_and(|i| i.fixed);
                }
            }
        }
        let state = &mut self.presentation.scroll;
        state
            .offsets
            .retain(|id, _| nodes.get(id).is_some_and(|n| n.range.is_some()));
        for (id, node) in &mut nodes {
            if let Some(range) = &mut node.range {
                if !range.axis.horizontal() {
                    range.min.x = 0.;
                    range.max.x = 0.;
                }
                if !range.axis.vertical() {
                    range.min.y = 0.;
                    range.max.y = 0.;
                }
                let offset = state.offsets.entry(*id).or_insert(point(0., 0.));
                offset.x = offset.x.clamp(range.min.x, range.max.x);
                offset.y = offset.y.clamp(range.min.y, range.max.y);
            }
        }
        // Parents precede children; all geometry is expressed in document units.
        for id in order {
            let mut offset = point(0., 0.);
            let mut clip: Option<Bounds<f32>> = None;
            let mut fixed = self
                .hierarchy
                .interactions
                .get(&id)
                .is_some_and(|i| i.fixed);
            let ancestors = nodes
                .get(&id)
                .map(|n| n.ancestors.clone())
                .unwrap_or_default();
            for parent in ancestors {
                if let Some(container) = nodes.get(&parent).filter(|n| n.range.is_some()) {
                    let viewport = container.displayed();
                    clip = Some(clip.map_or(viewport, |clip| clip.intersect(&viewport)));
                    if !fixed {
                        offset += self.presentation.scroll.offsets[&parent];
                    }
                    fixed = false;
                }
                fixed |= self
                    .hierarchy
                    .interactions
                    .get(&parent)
                    .is_some_and(|i| i.fixed);
            }
            if let Some(node) = nodes.get_mut(&id) {
                node.offset = offset;
                node.clip = clip;
            }
        }
        self.presentation.scroll.nodes = nodes;
        self.presentation.scroll.index = Some(index);
        self.presentation.scroll.settings = settings;
    }

    pub(in crate::editor) fn playback_scroll_offset(&self, id: usize) -> Point<f32> {
        self.presentation
            .scroll
            .nodes
            .get(&id)
            .filter(|_| self.presentation.scroll.active)
            .map_or(point(0., 0.), |node| node.offset)
    }

    pub(in crate::editor) fn playback_visible(&self, id: usize, viewport: Rect) -> bool {
        let Some(node) = self.presentation.scroll.nodes.get(&id) else {
            return true;
        };
        let mut rect = crate::scene::rotation::bounds(node.rect, node.rotation);
        rect.x -= node.offset.x;
        rect.y -= node.offset.y;
        let margin = self.effect_padding(id) + 32. / self.view.zoom;
        let area = Bounds::new(
            point(rect.x - margin, rect.y - margin),
            gpui::size(rect.width + margin * 2., rect.height + margin * 2.),
        );
        let area = node.clip.map_or(area, |clip| area.intersect(&clip));
        !area.intersect(&bounds(viewport)).is_empty()
    }

    pub(in crate::editor::presentation) fn scroll_by(
        &mut self,
        containers: &[usize],
        mut delta: Point<f32>,
    ) -> bool {
        let mut changed = false;
        for id in containers {
            let Some(range) = self.presentation.scroll.nodes.get(id).and_then(|n| n.range) else {
                continue;
            };
            let offset = self
                .presentation
                .scroll
                .offsets
                .entry(*id)
                .or_insert(point(0., 0.));
            let old = *offset;
            offset.x = (old.x + delta.x).clamp(range.min.x, range.max.x);
            offset.y = (old.y + delta.y).clamp(range.min.y, range.max.y);
            let consumed = *offset - old;
            delta -= consumed;
            changed |= consumed != point(0., 0.);
        }
        if changed {
            self.presentation.scroll.dirty = true;
            self.refresh_playback_scroll();
        }
        changed
    }

    pub(super) fn playback_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.leave_playback_hover(window, cx);
        let world = self.playback_world(event.position);
        let containers = self.playback_scroll_containers(world);
        let delta = event
            .delta
            .pixel_delta(px(24.))
            .map(|v| -f32::from(v) / self.playback_coordinates().0.zoom);
        let delta = if event.modifiers.shift && delta.x == 0. {
            point(delta.y, 0.)
        } else {
            delta
        };
        if delta != point(0., 0.)
            && let Some(press) = &mut self.presentation.scroll.press
        {
            press.moved = true;
        }
        if self.scroll_by(&containers, delta) {
            self.playback_hover(false, cx);
            cx.notify();
        }
        cx.stop_propagation();
    }

    pub(super) fn playback_scroll_containers(&self, world: Point<f32>) -> Vec<usize> {
        self.playback_target(world)
            .into_iter()
            .flat_map(|id| std::iter::once(id).chain(self.ancestors(id)))
            .filter(|id| {
                self.presentation
                    .scroll
                    .nodes
                    .get(id)
                    .is_some_and(|n| n.range.is_some())
            })
            .collect()
    }

    pub(super) fn playback_scroll_to(
        &mut self,
        target: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for container in self.ancestors(target) {
            let Some(range) = self
                .presentation
                .scroll
                .nodes
                .get(&container)
                .and_then(|n| n.range)
            else {
                continue;
            };
            let Some(target_rect) = self
                .presentation
                .scroll
                .nodes
                .get(&target)
                .map(Node::displayed)
            else {
                break;
            };
            let viewport = self.presentation.scroll.nodes[&container].displayed();
            let old = self.presentation.scroll.offsets[&container];
            // A fixed branch does not move with this viewport.
            let mut fixed = false;
            for id in std::iter::once(target)
                .chain(self.ancestors(target))
                .take_while(|id| *id != container)
            {
                if self
                    .presentation
                    .scroll
                    .nodes
                    .get(&id)
                    .is_some_and(|n| n.range.is_some())
                {
                    fixed = false;
                }
                fixed |= self
                    .hierarchy
                    .interactions
                    .get(&id)
                    .is_some_and(|i| i.fixed);
            }
            if fixed {
                continue;
            }
            let next = point(
                if range.axis.horizontal() {
                    (old.x + target_rect.origin.x - viewport.origin.x)
                        .clamp(range.min.x, range.max.x)
                } else {
                    old.x
                },
                if range.axis.vertical() {
                    (old.y + target_rect.origin.y - viewport.origin.y)
                        .clamp(range.min.y, range.max.y)
                } else {
                    old.y
                },
            );
            self.presentation.scroll.offsets.insert(container, next);
            self.presentation.scroll.dirty = true;
            self.refresh_playback_scroll();
        }
        self.playback_hover(false, cx);
        cx.notify();
    }
}
