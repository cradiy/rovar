use super::*;
use crate::{
    i18n::t,
    scene::{boolean, mask},
};
use std::collections::BTreeSet;
mod paint;
#[cfg(test)]
mod tests;

impl Workspace {
    pub(super) fn mask_overlay(&self, id: usize, element: impl IntoElement) -> gpui::AnyElement {
        if self
            .ancestors(id)
            .iter()
            .any(|id| mask::source(&self.hierarchy, *id).is_some())
        {
            gpui::deferred(element).into_any_element()
        } else {
            element.into_any_element()
        }
    }
    pub(super) fn mask_hit(&self, id: usize, world: Point<f32>) -> bool {
        let included = self.layer_ids().into_iter().collect();
        let mut groups = mask::ancestors(&self.hierarchy, id, &included);
        if mask::source(&self.hierarchy, id).is_some() {
            groups.push(id);
        }
        groups.into_iter().all(|id| {
            let source = mask::source(&self.hierarchy, id).unwrap();
            mask::outline(
                &self.hierarchy,
                &self.shapes,
                &self.boards,
                &mut self.boolean_cache.borrow_mut(),
                source,
            )
            .contains(world)
        })
    }
    pub(super) fn prune_masks(&mut self) {
        for (id, group) in &mut self.hierarchy.groups {
            if group
                .mask
                .is_some_and(|source| self.hierarchy.parents.get(&source) != Some(id))
            {
                group.mask = None;
            }
        }
    }
    pub(super) fn selected_mask(&self) -> Option<usize> {
        let ids = self.selection_ids();
        (ids.len() == 1)
            .then(|| *ids.first().unwrap())
            .filter(|id| mask::source(&self.hierarchy, *id).is_some())
    }

    pub(super) fn can_mask(&self) -> bool {
        let ids = self.selection_ids();
        !self.preview_read_only()
            && self.can_group()
            && ids
                .iter()
                .all(|id| self.layer_editable(*id) && !self.boards.iter().any(|b| b.id == *id))
            && self.mask_candidate().is_some()
    }

    fn mask_candidate(&self) -> Option<usize> {
        let ids = self.selection_ids();
        let parent = self.common_parent(&ids)?;
        let id = self
            .ordered_children(parent)
            .into_iter()
            .find(|id| ids.contains(id))?;
        (boolean::is_boolean(&self.hierarchy, id)
            || self
                .shapes
                .iter()
                .any(|s| s.id == id && boolean::supported(s)))
        .then_some(id)
    }

    pub(super) fn create_mask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_mask() {
            return;
        }
        let source = self.mask_candidate().unwrap();
        self.suspend(window, cx);
        self.group_selection(cx);
        let id = *self.selection_ids().first().unwrap();
        let group = self.hierarchy.groups.get_mut(&id).unwrap();
        group.mask = Some(source);
        group.name = t("mask-group").into();
        self.sync_fields(cx);
        cx.notify();
    }

    pub(super) fn release_mask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self
            .selected_mask()
            .filter(|id| !self.preview_read_only() && self.layer_editable(*id))
        {
            self.suspend(window, cx);
            self.set_selection(BTreeSet::from([id]), cx);
            self.ungroup_selection(cx);
        }
    }

    pub(super) fn mask_controls(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let release = self.selected_mask().is_some();
        if self.preview_read_only() || (!release && !self.can_mask()) {
            return None;
        }
        Some(
            inspector::composition::action(
                "mask-action",
                t(if release { "mask-release" } else { "mask-use" }),
                if release {
                    LucideIcons::Ungroup
                } else {
                    LucideIcons::Scan
                },
            )
            .flex_1()
            .on_click(cx.listener(move |this, _, window, cx| {
                if release {
                    this.release_mask(window, cx);
                } else {
                    this.create_mask(window, cx);
                }
            }))
            .into_any_element(),
        )
    }

    pub(super) fn mask_element(
        &self,
        element: gpui::AnyElement,
        outline: mask::Outline,
    ) -> gpui::AnyElement {
        paint::Masked {
            child: element,
            outline,
            view: self.view,
        }
        .into_any_element()
    }
}
