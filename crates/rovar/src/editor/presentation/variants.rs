use super::*;

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
                self.activate_prototype(source, action, window, cx);
            }
        }
    }
}
