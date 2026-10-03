use crate::{
    document::Page,
    scene::artboard::{MAX_SIZE, Rect},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use taffy::prelude::*;
mod constraints;
#[cfg(test)]
mod tests;
pub(crate) use constraints::{Constraint, Constraints};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Axis {
    #[default]
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Align {
    #[default]
    Start,
    Center,
    End,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Mode {
    #[default]
    Fixed,
    Hug,
    Fill,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sizing {
    pub width: Mode,
    pub height: Mode,
    pub absolute: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Constraints>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Container {
    pub axis: Axis,
    pub gap: f32,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub line_gap: f32,
    /// Top, right, bottom, left.
    pub padding: [f32; 4],
    pub main: Align,
    pub cross: Align,
    /// Groups have an explicit frame in their board's coordinate system.
    pub frame: Rect,
}
impl Container {
    pub fn new(frame: Rect) -> Self {
        Self {
            axis: Axis::Horizontal,
            gap: 12.,
            wrap: false,
            line_gap: 12.,
            padding: [12.; 4],
            main: Align::Start,
            cross: Align::Start,
            frame,
        }
    }
}

pub(crate) fn validate(page: &Page, ids: &std::collections::BTreeSet<usize>) -> anyhow::Result<()> {
    for (id, layout) in &page.hierarchy.layouts {
        anyhow::ensure!(
            page.hierarchy.groups.contains_key(id) || page.boards.iter().any(|b| b.id == *id),
            "Layout requires a container"
        );
        anyhow::ensure!(
            std::iter::once(layout.gap)
                .chain(std::iter::once(layout.line_gap))
                .chain(layout.padding)
                .all(|n| n.is_finite() && (0. ..=MAX_SIZE).contains(&n)),
            "Invalid layout spacing"
        );
        anyhow::ensure!(
            [
                layout.frame.x,
                layout.frame.y,
                layout.frame.width,
                layout.frame.height
            ]
            .iter()
            .all(|n| n.is_finite())
                && layout.frame.width >= 1.
                && layout.frame.height >= 1.,
            "Invalid layout frame"
        );
    }
    anyhow::ensure!(
        page.hierarchy.sizing.keys().all(|id| ids.contains(id)),
        "Missing layout item"
    );
    for sizing in page.hierarchy.sizing.values() {
        if let Some(constraints) = sizing.constraints {
            constraints.validate()?;
        }
    }
    Ok(())
}

/// Layout uses the same text shaping and flex engine as the live editor.
pub(crate) fn resolve(page: &mut Page, text_system: &gpui::WindowTextSystem) -> anyhow::Result<()> {
    if page.hierarchy.layouts.is_empty() && page.hierarchy.sizing.is_empty() {
        return Ok(());
    }
    let boards: BTreeMap<_, _> = page.boards.iter().map(|b| (b.id, b.rect)).collect();
    let mut rects = BTreeMap::new();
    let mut parents = BTreeMap::new();
    let mut hidden = BTreeMap::new();
    for (id, board, mut rect, hide) in page
        .boards
        .iter()
        .map(|b| (b.id, None, b.rect, b.layer.hidden))
        .chain(
            page.shapes
                .iter()
                .map(|s| (s.id, s.board, s.rect, s.layer.hidden)),
        )
        .chain(
            page.texts
                .iter()
                .map(|t| (t.id, t.board, t.rect, t.layer.hidden)),
        )
    {
        if let Some(origin) = board.and_then(|id| boards.get(&id)) {
            rect.x += origin.x;
            rect.y += origin.y;
        }
        rects.insert(id, rect);
        parents.insert(id, page.hierarchy.parents.get(&id).copied().or(board));
        hidden.insert(id, hide);
    }
    for (id, group) in &page.hierarchy.groups {
        parents.insert(*id, page.hierarchy.parents.get(id).copied().or(group.board));
        hidden.insert(*id, group.layer.hidden);
        if let Some(layout) = page.hierarchy.layouts.get(id) {
            let mut rect = layout.frame;
            if let Some(origin) = group.board.and_then(|id| boards.get(&id)) {
                rect.x += origin.x;
                rect.y += origin.y;
            }
            rects.insert(*id, rect);
        }
    }
    let ranks: BTreeMap<_, _> = page
        .hierarchy
        .order
        .iter()
        .enumerate()
        .map(|(rank, id)| (*id, rank))
        .collect();
    let mut children: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
    for (id, parent) in &parents {
        children.entry(*parent).or_default().push(*id);
    }
    for ids in children.values_mut() {
        ids.sort_by_key(|id| ranks.get(id).map_or((1, *id), |r| (0, *r)));
    }
    fn bounds(
        id: usize,
        children: &BTreeMap<Option<usize>, Vec<usize>>,
        rects: &mut BTreeMap<usize, Rect>,
    ) -> Rect {
        if let Some(rect) = rects.get(&id) {
            return *rect;
        }
        let rect = children
            .get(&Some(id))
            .into_iter()
            .flatten()
            .map(|child| bounds(*child, children, rects))
            .reduce(|a, b| {
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                Rect {
                    x,
                    y,
                    width: (a.x + a.width).max(b.x + b.width) - x,
                    height: (a.y + a.height).max(b.y + b.height) - y,
                }
            })
            .unwrap_or(Rect {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            });
        rects.insert(id, rect);
        rect
    }
    for id in parents.keys() {
        bounds(*id, &children, &mut rects);
    }
    struct Tree<'a> {
        page: &'a Page,
        children: &'a BTreeMap<Option<usize>, Vec<usize>>,
        rects: &'a BTreeMap<usize, Rect>,
        hidden: &'a BTreeMap<usize, bool>,
        nodes: BTreeMap<usize, NodeId>,
        taffy: TaffyTree<usize>,
    }
    impl Tree<'_> {
        fn build(&mut self, id: usize, parent: Option<usize>) -> anyhow::Result<NodeId> {
            let rect = self.rects[&id];
            let sizing = self
                .page
                .hierarchy
                .sizing
                .get(&id)
                .copied()
                .unwrap_or_default();
            let parent_layout = parent.and_then(|p| self.page.hierarchy.layouts.get(&p));
            let flowing = parent_layout.is_some() && !sizing.absolute;
            let own = self.page.hierarchy.layouts.get(&id);
            let dimension = |mode, value| match mode {
                Mode::Fixed => length(value),
                Mode::Hug => auto(),
                Mode::Fill if flowing => auto(),
                Mode::Fill => length(value),
            };
            let mut style = Style {
                display: if self.hidden[&id] {
                    Display::None
                } else if own.is_some() {
                    Display::Flex
                } else {
                    Display::Block
                },
                size: Size {
                    width: dimension(sizing.width, rect.width),
                    height: dimension(sizing.height, rect.height),
                },
                min_size: Size {
                    width: length(if flowing || own.is_some() {
                        1_f32
                    } else {
                        0_f32
                    }),
                    height: length(if flowing || own.is_some() {
                        1_f32
                    } else {
                        0_f32
                    }),
                },
                max_size: Size {
                    width: length(MAX_SIZE),
                    height: length(MAX_SIZE),
                },
                flex_shrink: 0.,
                ..Default::default()
            };
            if let Some(layout) = parent_layout.filter(|_| flowing) {
                let (main, cross) = if layout.axis == Axis::Horizontal {
                    (sizing.width, sizing.height)
                } else {
                    (sizing.height, sizing.width)
                };
                if main == Mode::Fill {
                    style.flex_grow = 1.;
                    style.flex_basis = length(0_f32);
                }
                if cross == Mode::Fill {
                    style.align_self = Some(AlignItems::Stretch);
                }
            } else if let Some(parent) = parent {
                style.position = Position::Absolute;
                style.inset.left = length(rect.x - self.rects[&parent].x);
                style.inset.top = length(rect.y - self.rects[&parent].y);
                if let Some(constraints) = sizing.constraints {
                    constraints.apply(&mut style);
                }
            }
            if let Some(layout) = own {
                style.flex_direction = if layout.axis == Axis::Horizontal {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                };
                style.gap = Size {
                    width: length(if layout.wrap && layout.axis == Axis::Vertical {
                        layout.line_gap
                    } else {
                        layout.gap
                    }),
                    height: length(if layout.wrap && layout.axis == Axis::Horizontal {
                        layout.line_gap
                    } else {
                        layout.gap
                    }),
                };
                style.flex_wrap = if layout.wrap {
                    FlexWrap::Wrap
                } else {
                    FlexWrap::NoWrap
                };
                style.align_content = Some(AlignContent::Start);
                style.padding = taffy::Rect {
                    top: length(layout.padding[0]),
                    right: length(layout.padding[1]),
                    bottom: length(layout.padding[2]),
                    left: length(layout.padding[3]),
                };
                style.align_items = Some(match layout.cross {
                    Align::Start => AlignItems::Start,
                    Align::Center => AlignItems::Center,
                    Align::End => AlignItems::End,
                });
                style.justify_content = Some(match layout.main {
                    Align::Start => JustifyContent::Start,
                    Align::Center => JustifyContent::Center,
                    Align::End => JustifyContent::End,
                });
            }
            let children = self
                .children
                .get(&Some(id))
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|child| self.build(child, Some(id)))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let node = if children.is_empty() {
                self.taffy.new_leaf_with_context(style, id)?
            } else {
                self.taffy.new_with_children(style, &children)?
            };
            self.nodes.insert(id, node);
            Ok(node)
        }
        fn collect(
            &self,
            id: usize,
            parent: Option<usize>,
            x: f32,
            y: f32,
            output: &mut BTreeMap<usize, Rect>,
        ) -> anyhow::Result<()> {
            if self.hidden[&id] {
                return Ok(());
            }
            let layout = self.taffy.layout(self.nodes[&id])?;
            let mut rect = Rect {
                x: x + layout.location.x,
                y: y + layout.location.y,
                width: layout.size.width,
                height: layout.size.height,
            };
            if let Some(parent) = parent
                && let Some(sizing) = self.page.hierarchy.sizing.get(&id)
                && (sizing.absolute || !self.page.hierarchy.layouts.contains_key(&parent))
                && let Some(constraints) = sizing.constraints
            {
                let parent_rect = output[&parent];
                if constraints.horizontal == Constraint::Center {
                    rect.x = parent_rect.x
                        + (parent_rect.width - rect.width) / 2.
                        + constraints.rect.x
                        + (constraints.rect.width - constraints.parent_size[0]) / 2.;
                }
                if constraints.vertical == Constraint::Center {
                    rect.y = parent_rect.y
                        + (parent_rect.height - rect.height) / 2.
                        + constraints.rect.y
                        + (constraints.rect.height - constraints.parent_size[1]) / 2.;
                }
            }
            output.insert(id, rect);
            for child in self.children.get(&Some(id)).into_iter().flatten() {
                self.collect(*child, Some(id), rect.x, rect.y, output)?;
            }
            Ok(())
        }
    }
    let mut tree = Tree {
        page,
        children: &children,
        rects: &rects,
        hidden: &hidden,
        nodes: BTreeMap::new(),
        taffy: TaffyTree::new(),
    };
    tree.taffy.disable_rounding();
    let mut output = BTreeMap::new();
    for id in children.get(&None).into_iter().flatten() {
        let node = tree.build(*id, None)?;
        tree.taffy.compute_layout_with_measure(
            node,
            Size::MAX_CONTENT,
            |known, available, _, context, _| {
                let Some(id) = context else { return Size::ZERO };
                if let Some(text) = page.texts.iter().find(|t| t.id == *id) {
                    let width = known.width.unwrap_or(match available.width {
                        AvailableSpace::Definite(w) => w,
                        _ => MAX_SIZE,
                    });
                    let [width, height] = crate::scene::text::measure_content(
                        &text.content,
                        &text.styles,
                        width,
                        text_system,
                    );
                    Size {
                        width: known.width.unwrap_or(width),
                        height: known.height.unwrap_or(height),
                    }
                } else {
                    Size {
                        width: rects[id].width,
                        height: rects[id].height,
                    }
                }
            },
        )?;
        tree.collect(*id, None, rects[id].x, rects[id].y, &mut output)?;
    }
    for board in &mut page.boards {
        if let Some(rect) = output.get(&board.id) {
            board.rect = *rect;
        }
    }
    let local = |id, board: Option<usize>| {
        output.get(&id).map(|rect| {
            let mut rect = *rect;
            if let Some(origin) = board.and_then(|id| output.get(&id).or_else(|| boards.get(&id))) {
                rect.x -= origin.x;
                rect.y -= origin.y;
            }
            rect
        })
    };
    for shape in &mut page.shapes {
        if let Some(rect) = local(shape.id, shape.board) {
            shape.rect = rect;
        }
    }
    for text in &mut page.texts {
        if let Some(rect) = local(text.id, text.board) {
            text.rect = rect;
        }
    }
    for (id, layout) in &mut page.hierarchy.layouts {
        if let Some(group) = page.hierarchy.groups.get(id)
            && let Some(rect) = local(*id, group.board)
        {
            layout.frame = rect;
        }
    }
    Ok(())
}
