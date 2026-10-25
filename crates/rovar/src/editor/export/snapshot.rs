use super::*;
use crate::{
    document::Page,
    scene::{boolean, components, layer::LayerState, rotation},
};
use std::{cell::RefCell, collections::HashMap};

/// Export planning uses document data, without activating pages or constructing editor views.
pub(in crate::editor) struct Snapshot {
    pub(super) page: Page,
    pub(super) boolean_cache: RefCell<boolean::Cache>,
    layers: HashMap<usize, LayerState>,
}

impl std::ops::Deref for Snapshot {
    type Target = Page;
    fn deref(&self) -> &Page {
        &self.page
    }
}

impl Snapshot {
    pub(in crate::editor) fn new(page: Page) -> Self {
        let layers = page
            .boards
            .iter()
            .map(|node| (node.id, node.layer))
            .chain(page.shapes.iter().map(|node| (node.id, node.layer)))
            .chain(page.texts.iter().map(|node| (node.id, node.layer)))
            .chain(
                page.hierarchy
                    .groups
                    .iter()
                    .map(|(id, node)| (*id, node.layer)),
            )
            .collect();
        Self {
            page,
            boolean_cache: Default::default(),
            layers,
        }
    }

    pub(super) fn boolean_geometry(&self, id: usize) -> Option<boolean::Geometry> {
        self.boolean_cache
            .borrow_mut()
            .get(&self.hierarchy, &self.shapes, id)
    }

    pub(super) fn parent_origin(&self, board: Option<usize>) -> Point<f32> {
        board
            .and_then(|id| self.boards.iter().find(|board| board.id == id))
            .map_or(point(0., 0.), |board| point(board.rect.x, board.rect.y))
    }

    fn layer(&self, id: usize) -> LayerState {
        self.layers.get(&id).copied().unwrap_or_default()
    }

    pub(super) fn object_rotation(&self, id: usize) -> f32 {
        self.layer(id).rotation
    }

    pub(super) fn descendants(&self, roots: &BTreeSet<usize>) -> BTreeSet<usize> {
        roots
            .iter()
            .flat_map(|id| components::subtree(self, *id))
            .collect()
    }

    pub(super) fn world_rect(&self, id: usize) -> Option<Rect> {
        if let Some(group) = self.hierarchy.groups.get(&id) {
            if let Some(source) = crate::scene::mask::source(&self.hierarchy, id)
                && let Some(bounds) = crate::scene::mask::outline(
                    &self.hierarchy,
                    &self.shapes,
                    &self.boards,
                    &mut self.boolean_cache.borrow_mut(),
                    source,
                )
                .bounds()
            {
                return Some(bounds);
            }
            if boolean::is_boolean(&self.hierarchy, id)
                && let Some(geometry) = self
                    .boolean_geometry(id)
                    .filter(|geometry| !geometry.contours.is_empty())
            {
                let mut rect = geometry.shape.rect;
                let origin = self.parent_origin(geometry.shape.board);
                rect.x += origin.x;
                rect.y += origin.y;
                return Some(rect);
            }
            if let Some(layout) = self.hierarchy.layouts.get(&id) {
                let origin = self.parent_origin(group.board);
                return Some(Rect {
                    x: layout.frame.x + origin.x,
                    y: layout.frame.y + origin.y,
                    ..layout.frame
                });
            }
            return components::subtree(self, id)
                .into_iter()
                .filter(|child| *child != id && !self.hierarchy.groups.contains_key(child))
                .filter_map(|child| {
                    self.world_rect(child)
                        .map(|rect| rotation::bounds(rect, self.object_rotation(child)))
                })
                .reduce(|a, b| {
                    let x = a.x.min(b.x);
                    let y = a.y.min(b.y);
                    Rect {
                        x,
                        y,
                        width: (a.x + a.width).max(b.x + b.width) - x,
                        height: (a.y + a.height).max(b.y + b.height) - y,
                    }
                });
        }
        components::bounds(self, id)
    }

    pub(super) fn paint_order(&self) -> Vec<usize> {
        let ranks: HashMap<_, _> = self
            .hierarchy
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        let mut children: HashMap<Option<usize>, Vec<usize>> = HashMap::new();
        for (id, board) in self
            .boards
            .iter()
            .map(|node| (node.id, None))
            .chain(self.shapes.iter().map(|node| (node.id, node.board)))
            .chain(self.texts.iter().map(|node| (node.id, node.board)))
            .chain(
                self.hierarchy
                    .groups
                    .iter()
                    .map(|(id, node)| (*id, node.board)),
            )
        {
            children
                .entry(self.hierarchy.parents.get(&id).copied().or(board))
                .or_default()
                .push(id);
        }
        for ids in children.values_mut() {
            ids.sort_unstable_by_key(|id| ranks.get(id).map_or((1, *id), |rank| (0, *rank)));
        }
        let mut pending = children.remove(&None).unwrap_or_default();
        pending.reverse();
        let mut result = Vec::new();
        while let Some(id) = pending.pop() {
            if self.layer(id).hidden {
                continue;
            }
            if !self.hierarchy.groups.contains_key(&id)
                || boolean::is_boolean(&self.hierarchy, id)
                || crate::scene::mask::source(&self.hierarchy, id).is_some()
                || self.layer(id).composited()
            {
                result.push(id);
            }
            pending.extend(
                children
                    .remove(&Some(id))
                    .unwrap_or_default()
                    .into_iter()
                    .rev(),
            );
        }
        result
    }

    pub(super) fn layer_name(&self, id: usize) -> String {
        self.hierarchy
            .names
            .get(&id)
            .cloned()
            .or_else(|| self.hierarchy.groups.get(&id).map(|node| node.name.clone()))
            .or_else(|| {
                self.boards
                    .iter()
                    .find(|node| node.id == id)
                    .map(|node| node.name.clone())
            })
            .or_else(|| {
                self.shapes
                    .iter()
                    .find(|node| node.id == id)
                    .map(|node| node.name.clone())
            })
            .or_else(|| {
                self.texts
                    .iter()
                    .find(|node| node.id == id)
                    .map(|node| {
                        node.content
                            .chars()
                            .take(64)
                            .map(|c| if c.is_whitespace() { ' ' } else { c })
                            .collect::<String>()
                    })
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| crate::i18n::message("text-name", &[("id", id.to_string())]))
    }
}
