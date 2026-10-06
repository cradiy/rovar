use crate::{document::Page, scene::layer::LayerState};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

/// Keep the current isolated group's body separate, so backdrop effects sample
/// only preceding artwork from that same compositing scope.
pub(super) struct Stack {
    layers: BTreeMap<usize, LayerState>,
    masks: BTreeSet<usize>,
    parents: BTreeMap<usize, usize>,
    scopes: Vec<(usize, String)>,
}

impl Stack {
    pub fn new(page: &Page) -> Self {
        Self {
            layers: page
                .boards
                .iter()
                .map(|b| (b.id, b.layer))
                .chain(page.shapes.iter().map(|s| (s.id, s.layer)))
                .chain(page.texts.iter().map(|t| (t.id, t.layer)))
                .chain(page.hierarchy.groups.iter().map(|(id, g)| (*id, g.layer)))
                .filter(|(id, state)| {
                    state.composited() || crate::scene::mask::source(&page.hierarchy, *id).is_some()
                })
                .collect(),
            scopes: Vec::new(),
            masks: page
                .hierarchy
                .groups
                .keys()
                .copied()
                .filter(|id| crate::scene::mask::source(&page.hierarchy, *id).is_some())
                .collect(),
            parents: page
                .shapes
                .iter()
                .filter_map(|s| s.board.map(|p| (s.id, p)))
                .chain(page.texts.iter().filter_map(|t| t.board.map(|p| (t.id, p))))
                .chain(
                    page.hierarchy
                        .groups
                        .iter()
                        .filter_map(|(id, g)| g.board.map(|p| (*id, p))),
                )
                .chain(page.hierarchy.parents.iter().map(|(id, p)| (*id, *p)))
                .collect(),
        }
    }
    pub fn advance(&mut self, id: usize, body: &mut String) {
        if self.scopes.is_empty() {
            return;
        }
        let mut ancestors = Vec::new();
        let mut parent = self.parents.get(&id).copied();
        while let Some(id) = parent {
            ancestors.push(id);
            parent = self.parents.get(&id).copied();
        }
        while self
            .scopes
            .last()
            .is_some_and(|(id, _)| !ancestors.contains(id))
        {
            self.close(body);
        }
    }
    pub fn enter(&mut self, id: usize, body: &mut String) {
        if self.layers.contains_key(&id) {
            self.scopes.push((id, std::mem::take(body)));
        }
    }
    fn close(&mut self, body: &mut String) {
        let (id, mut prefix) = self.scopes.pop().unwrap();
        let state = self.layers[&id];
        if self.masks.contains(&id) {
            *body = format!("<g clip-path=\"url(#vector-mask-{id})\">{body}</g>");
        }
        write!(
            prefix,
            "<g opacity=\"{}\" style=\"mix-blend-mode:{};isolation:isolate\">{body}</g>",
            state.opacity,
            state.blend.css()
        )
        .unwrap();
        *body = prefix;
    }
    pub fn finish(&mut self, body: &mut String) {
        while !self.scopes.is_empty() {
            self.close(body);
        }
    }
}
