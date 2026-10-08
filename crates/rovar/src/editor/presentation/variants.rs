use super::*;
use crate::scene::{mask, rotation};

pub(super) struct Hover {
    // Capture the original hit region so changing variant geometry or actions
    // cannot cause repeated enter/leave cycles under a stationary pointer.
    region: mask::Outline,
    masks: Vec<mask::Outline>,
    view: Viewport,
    bounds: Bounds<Pixels>,
    restore: Option<HoverSnapshot>,
    root: usize,
    exit: HoverExit,
}

struct HoverSnapshot {
    page: crate::document::Page,
    current: usize,
    history: Vec<usize>,
    view: Viewport,
    bounds: Bounds<Pixels>,
}

impl Hover {
    fn contains(&self, position: Point<Pixels>) -> bool {
        let world = self
            .view
            .world((position - self.bounds.origin).map(f32::from));
        self.bounds.contains(&position)
            && self.region.contains(world)
            && self.masks.iter().all(|mask| mask.contains(world))
    }
}

impl Workspace {
    pub(super) fn prototype_component(&self, source: usize) -> Option<usize> {
        std::iter::once(source)
            .chain(self.ancestors(source))
            .find(|id| self.hierarchy.components.contains_key(id))
    }

    pub(super) fn prototype_variants(&self, source: usize) -> Vec<(uuid::Uuid, String)> {
        let Some(root) = self.prototype_component(source) else {
            return Vec::new();
        };
        let component = &self.hierarchy.components[&root].component;
        self.components
            .sets
            .values()
            .find(|set| set.variants.contains_key(component))
            .into_iter()
            .flat_map(|set| &set.variants)
            .filter(|(id, _)| *id != component)
            .filter_map(|(id, name)| Some((uuid::Uuid::parse_str(id).ok()?, name.clone())))
            .collect()
    }

    pub(super) fn activate_prototype(
        &mut self,
        source: usize,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Action::ChangeVariant { target } = action else {
            self.playback_action(action, cx);
            return;
        };
        if self.presentation.playback.is_none()
            || !self
                .prototype_variants(source)
                .iter()
                .any(|(id, _)| *id == target)
        {
            return;
        }
        let root = self.prototype_component(source).unwrap();
        let (mut page, _) = self.snapshot_page(cx);
        if let Err(error) = crate::scene::components::variants::switch(
            &mut page,
            root,
            &target.to_string(),
            &self.components.definitions,
            &self.components.sets,
        ) {
            eprintln!("Could not switch preview variant: {error:#}");
            return;
        }
        self.pause_videos(cx);
        self.apply_component_page(page, window, cx);
        self.set_selection(BTreeSet::new(), cx);
        cx.notify();
    }

    pub(super) fn playback_pointer(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(hover) = self
            .presentation
            .playback
            .as_ref()
            .and_then(|p| p.hover.as_ref())
        {
            if hover.contains(position) {
                self.playback_hover(true, cx);
                return;
            }
            self.leave_playback_hover(window, cx);
            self.playback_hover(false, cx);
            return;
        }
        let world = self.board_point(None, position);
        let click = self.playback_hit(world, Trigger::Click);
        let hover = self.playback_hit(world, Trigger::Hover);
        self.playback_hover(click.is_some() || hover.is_some(), cx);
        // Keep the entry latch across a variant swap so two hover variants cannot
        // repeatedly switch each other while the pointer remains on the component.
        let entered = hover.map(|(id, _)| self.prototype_component(id).unwrap_or(id));
        let Some(playback) = &mut self.presentation.playback else {
            return;
        };
        if playback.entered != entered {
            playback.entered = entered;
            if let Some((source, action)) = hover {
                if !matches!(action, Action::ChangeVariant { target } if !self.prototype_variants(source).iter().any(|(id, _)| *id == target))
                {
                    let rect = self.world_rect(source).unwrap();
                    let mut region = mask::outline(
                        &self.hierarchy,
                        &self.shapes,
                        &self.boards,
                        &mut self.boolean_cache.borrow_mut(),
                        source,
                    );
                    if region.0.is_empty() {
                        region.0.push(
                            [
                                point(rect.x, rect.y),
                                point(rect.x + rect.width, rect.y),
                                point(rect.x + rect.width, rect.y + rect.height),
                                point(rect.x, rect.y + rect.height),
                            ]
                            .map(|p| {
                                rotation::around(
                                    p,
                                    rotation::center(rect),
                                    self.object_rotation(source),
                                )
                            })
                            .to_vec(),
                        );
                    }
                    let included = self.layer_ids().into_iter().collect();
                    let mut groups = mask::ancestors(&self.hierarchy, source, &included);
                    if mask::source(&self.hierarchy, source).is_some() {
                        groups.push(source);
                    }
                    let masks = groups
                        .into_iter()
                        .map(|id| {
                            mask::outline(
                                &self.hierarchy,
                                &self.shapes,
                                &self.boards,
                                &mut self.boolean_cache.borrow_mut(),
                                mask::source(&self.hierarchy, id).unwrap(),
                            )
                        })
                        .collect();
                    let page = self.snapshot_page(cx).0;
                    let p = self.presentation.playback.as_ref().unwrap();
                    let restore = Some(HoverSnapshot {
                        page,
                        current: p.current,
                        history: p.history.clone(),
                        view: self.view,
                        bounds: self.bounds.get(),
                    });
                    self.presentation.playback.as_mut().unwrap().hover = Some(Hover {
                        region,
                        masks,
                        restore,
                        view: self.view,
                        bounds: self.bounds.get(),
                        root: self.prototype_component(source).unwrap_or(source),
                        exit: self.hierarchy.interactions[&source].hover_exit,
                    });
                }
                self.activate_prototype(source, action, window, cx);
            }
        }
    }

    fn restore_playback_hover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(snapshot) = self
            .presentation
            .playback
            .as_mut()
            .and_then(|p| p.hover.as_mut())
            .and_then(|h| h.restore.take())
        {
            self.restore_hover_snapshot(snapshot, window, cx);
        }
    }

    fn restore_hover_snapshot(
        &mut self,
        snapshot: HoverSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pause_videos(cx);
        self.apply_component_page(snapshot.page, window, cx);
        let p = self.presentation.playback.as_mut().unwrap();
        p.current = snapshot.current;
        p.history = snapshot.history;
        self.view = snapshot.view;
        self.bounds.set(snapshot.bounds);
        cx.notify();
    }

    pub(super) fn commit_playback_hover(&mut self) {
        if let Some(p) = &mut self.presentation.playback {
            p.hover = None;
            p.entered = None;
        }
    }

    pub(super) fn leave_playback_hover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let hover = self.presentation.playback.as_mut().and_then(|p| {
            p.entered = None;
            p.hover.take()
        });
        let Some(hover) = hover else {
            return;
        };
        // A click consumes the temporary state and takes precedence over its exit action.
        let Some(snapshot) = hover.restore else {
            return;
        };
        match hover.exit {
            HoverExit::Keep => (),
            HoverExit::Restore | HoverExit::ChangeVariant { .. } => {
                self.restore_hover_snapshot(snapshot, window, cx);
                if let HoverExit::ChangeVariant { target } = hover.exit {
                    self.activate_prototype(
                        hover.root,
                        Action::ChangeVariant { target },
                        window,
                        cx,
                    );
                }
                cx.notify();
            }
        }
    }

    pub(super) fn playback_click(
        &mut self,
        source: usize,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.prototype_component(source);
        if let Some(p) = &mut self.presentation.playback
            && let Some(hover) = &mut p.hover
            && hover
                .restore
                .as_ref()
                .is_some_and(|s| s.current != p.current)
        {
            // Clicking the destination commits a hovered navigation before the
            // destination's action runs, preserving its place in the history.
            hover.restore = None;
        }
        self.restore_playback_hover(window, cx);
        // An explicit click commits its own action after removing the temporary
        // hover state. The entry latch remains until the pointer leaves.
        let source = if matches!(action, Action::ChangeVariant { .. }) {
            root.unwrap_or(source)
        } else {
            source
        };
        self.activate_prototype(source, action, window, cx);
    }
}
