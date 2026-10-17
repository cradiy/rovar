use super::*;
use std::{collections::HashMap, rc::Rc};

#[derive(Clone, PartialEq)]
struct Node {
    kind: u8,
    id: usize,
    parent: Option<usize>,
    board: Option<usize>,
    rect: Option<Rect>,
    rotation: f32,
    hidden: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn index_tracks_uncommitted_geometry_hierarchy_and_visibility(cx: &mut gpui::TestAppContext) {
        let handle = crate::editor::tests::open(cx);
        handle
            .update(cx, |w, _, _| {
                let rect = Rect {
                    x: 10.,
                    y: 20.,
                    width: 30.,
                    height: 40.,
                };
                w.shapes
                    .push(Shape::new(1, None, ShapeKind::Rectangle, rect));
                for id in [10, 20] {
                    w.hierarchy.groups.insert(
                        id,
                        crate::scene::layer::LayerGroup {
                            uid: uuid::Uuid::new_v4(),
                            name: "Group".into(),
                            board: None,
                            layer: Default::default(),
                            mask: None,
                            boolean: None,
                        },
                    );
                }
                w.hierarchy.parents.extend([(1, 10), (10, 20)]);
                let original = w.scene_index();
                assert_eq!(original.groups[&20], rect);
                // Insertion and removal can shift cached positions between renders.
                w.shapes
                    .insert(0, Shape::new(2, None, ShapeKind::Rectangle, rect));
                w.shapes[1].layer.locked = true;
                assert!(!w.layer_editable(1));
                assert!(w.layer_editable(2));
                let shape = w.shapes.remove(1);
                assert!(w.object_info(1).is_none());
                assert_eq!(w.object_rect(2), Some((None, rect)));
                w.shapes.clear();
                w.shapes.push(shape);
                w.shapes[0].layer.locked = false;
                w.view.pan.x = 100.;
                w.view.zoom = 0.25;
                w.shapes[0].color = rgb(0xff0000);
                assert!(Rc::ptr_eq(&original, &w.scene_index()));
                // Drag previews have not recorded a history change yet.
                w.shapes[0].rect.x = 50.;
                w.shapes[0].layer.hidden = true;
                let moved = w.scene_index();
                assert_eq!(moved.groups[&20].x, 50.);
                assert!(!moved.order.contains(&1));
                w.hierarchy.parents.insert(1, 20);
                assert!(!w.scene_index().groups.contains_key(&10));
                assert_eq!(w.descendants(&[10].into()), [10].into());
                assert_eq!(w.descendants(&[20].into()), [1, 10, 20].into());
                w.hierarchy.parents.insert(1, 10);
                w.shapes[0].rect = rect;
                w.shapes[0].layer.hidden = false;
                let restored = w.scene_index();
                assert_eq!(restored.groups[&20], rect);
                assert_eq!(restored.order, original.order);
                w.shapes.clear();
                // A conversion may reuse the same ID and geometry in another collection.
                w.boards.push(Artboard {
                    id: 1,
                    uid: uuid::Uuid::new_v4(),
                    name: "Frame".into(),
                    rect,
                    layer: Default::default(),
                    color: rgb(0xffffff),
                    color_style: None,
                    image_fill: Default::default(),
                    fill_mode: FillMode::Solid,
                    gradient: Default::default(),
                });
                assert_eq!(w.object_rect(1), Some((None, rect)));
                assert_eq!(w.board_by_id(1).map(|b| b.rect), Some(rect));
                assert_eq!(w.scene_index().boards.get(&1), Some(&0));
                assert!(!w.scene_index().shapes.contains_key(&1));
                w.boards.clear();
                assert!(w.scene_index().groups.is_empty());
            })
            .unwrap();
    }
}

pub(super) struct Index {
    pub boards: HashMap<usize, usize>,
    pub shapes: HashMap<usize, usize>,
    pub texts: HashMap<usize, usize>,
    pub groups: HashMap<usize, Rect>,
    pub order: Vec<usize>,
    pub included: std::collections::BTreeSet<usize>,
}

#[derive(Default)]
pub(super) struct Cache {
    nodes: Vec<Node>,
    order: Vec<usize>,
    index: Option<Rc<Index>>,
}

impl Workspace {
    // Edits may move vector entries before the next render refreshes the index.
    // Validate cached positions against the live ID and search on a cache miss.
    pub(super) fn board_by_id(&self, id: usize) -> Option<&Artboard> {
        self.scene_index
            .borrow()
            .index
            .as_ref()
            .and_then(|index| index.boards.get(&id))
            .and_then(|i| self.boards.get(*i))
            .filter(|b| b.id == id)
            .or_else(|| self.boards.iter().find(|b| b.id == id))
    }

    pub(super) fn object_info(
        &self,
        id: usize,
    ) -> Option<(Option<usize>, Rect, crate::scene::layer::LayerState)> {
        let cache = self.scene_index.borrow();
        let cached = cache.index.as_ref().and_then(|index| {
            index
                .boards
                .get(&id)
                .and_then(|i| self.boards.get(*i))
                .filter(|b| b.id == id)
                .map(|b| (None, b.rect, b.layer))
                .or_else(|| {
                    index
                        .shapes
                        .get(&id)
                        .and_then(|i| self.shapes.get(*i))
                        .filter(|s| s.id == id)
                        .map(|s| (s.board, s.rect, s.layer))
                })
                .or_else(|| {
                    index
                        .texts
                        .get(&id)
                        .and_then(|i| self.texts.get(*i))
                        .filter(|t| t.id == id)
                        .map(|t| (t.board, t.rect, t.layer))
                })
        });
        cached.or_else(|| {
            self.boards
                .iter()
                .find(|b| b.id == id)
                .map(|b| (None, b.rect, b.layer))
                .or_else(|| {
                    self.shapes
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| (s.board, s.rect, s.layer))
                })
                .or_else(|| {
                    self.texts
                        .iter()
                        .find(|t| t.id == id)
                        .map(|t| (t.board, t.rect, t.layer))
                })
        })
    }

    // Compare geometry directly: previews can change it before history is committed.
    // Pan, zoom, selection and paint changes reuse the index without rebuilding maps.
    pub(super) fn scene_index(&self) -> Rc<Index> {
        let mut cache = self.scene_index.borrow_mut();
        if let Some(index) = &cache.index
            && cache.order == self.hierarchy.order
            && self.geometry_nodes().eq(cache.nodes.iter().cloned())
        {
            return index.clone();
        }
        cache.nodes = self.geometry_nodes().collect();
        cache.order.clone_from(&self.hierarchy.order);
        let order = self.canvas_layer_order();
        let index = Rc::new(Index {
            boards: self
                .boards
                .iter()
                .enumerate()
                .map(|(i, b)| (b.id, i))
                .collect(),
            shapes: self
                .shapes
                .iter()
                .enumerate()
                .map(|(i, s)| (s.id, i))
                .collect(),
            texts: self
                .texts
                .iter()
                .enumerate()
                .map(|(i, t)| (t.id, i))
                .collect(),
            groups: self.group_content_bounds(),
            included: order.iter().copied().collect(),
            order,
        });
        cache.index = Some(index.clone());
        index
    }

    fn geometry_nodes(&self) -> impl Iterator<Item = Node> + '_ {
        self.boards
            .iter()
            .map(|b| (0, b.id, None, Some(b.rect), b.layer))
            .chain(
                self.shapes
                    .iter()
                    .map(|s| (1, s.id, s.board, Some(s.rect), s.layer)),
            )
            .chain(
                self.texts
                    .iter()
                    .map(|t| (2, t.id, t.board, Some(t.rect), t.layer)),
            )
            .chain(
                self.hierarchy
                    .groups
                    .iter()
                    .map(|(id, g)| (3, *id, g.board, None, g.layer)),
            )
            .map(|(kind, id, board, rect, layer)| Node {
                kind,
                id,
                parent: self.hierarchy.parents.get(&id).copied().or(board),
                board,
                rect,
                rotation: layer.rotation,
                hidden: layer.hidden,
            })
    }
}
