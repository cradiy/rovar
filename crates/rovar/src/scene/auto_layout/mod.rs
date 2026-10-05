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
    Grid,
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sizing {
    pub width: Mode,
    pub height: Mode,
    pub absolute: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Constraints>,
    #[serde(default, skip_serializing_if = "Limits::is_empty")]
    pub limits: Limits,
    #[serde(default = "one")]
    pub column_span: u16,
    #[serde(default = "one")]
    pub row_span: u16,
}

fn one() -> u16 {
    1
}
fn default_columns() -> u16 {
    2
}
pub(crate) const MAX_GRID_TRACKS: u16 = 256;

impl Default for Sizing {
    fn default() -> Self {
        Self {
            width: Mode::Fixed,
            height: Mode::Fixed,
            absolute: false,
            constraints: None,
            limits: Limits::default(),
            column_span: 1,
            row_span: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Limits {
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
}

impl Limits {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(self) -> anyhow::Result<()> {
        for (min, max) in [
            (self.min_width, self.max_width),
            (self.min_height, self.max_height),
        ] {
            anyhow::ensure!(
                min.is_none_or(|v| v.is_finite() && (0. ..=MAX_SIZE).contains(&v))
                    && max.is_none_or(|v| v.is_finite() && (1. ..=MAX_SIZE).contains(&v))
                    && min.zip(max).is_none_or(|(min, max)| min <= max),
                "Invalid size limits"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Container {
    pub axis: Axis,
    #[serde(default = "default_columns")]
    pub columns: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_width: Option<f32>,
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
            columns: default_columns(),
            column_width: None,
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
            !crate::scene::boolean::is_boolean(&page.hierarchy, *id),
            "Boolean groups cannot be auto layout containers"
        );
        anyhow::ensure!(
            (1..=MAX_GRID_TRACKS).contains(&layout.columns)
                && layout
                    .column_width
                    .is_none_or(|v| v.is_finite() && (1. ..=MAX_SIZE).contains(&v)),
            "Invalid grid columns"
        );
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
        anyhow::ensure!(
            [sizing.column_span, sizing.row_span]
                .iter()
                .all(|span| (1..=MAX_GRID_TRACKS).contains(span)),
            "Invalid grid span"
        );
        sizing.limits.validate()?;
        if let Some(constraints) = sizing.constraints {
            constraints.validate()?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct GridTracks {
    pub bounds: Rect,
    pub columns: Vec<[f32; 2]>,
    pub rows: Vec<[f32; 2]>,
}

/// Layout uses the same text shaping and layout engine as the live editor.
pub(crate) fn resolve(
    page: &mut Page,
    text_system: &gpui::WindowTextSystem,
) -> anyhow::Result<BTreeMap<usize, GridTracks>> {
    if page.hierarchy.layouts.is_empty() && page.hierarchy.sizing.is_empty() {
        return Ok(BTreeMap::new());
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
    let mut booleans = crate::scene::boolean::Cache::default();
    for (id, group) in &page.hierarchy.groups {
        parents.insert(*id, page.hierarchy.parents.get(id).copied().or(group.board));
        hidden.insert(*id, group.layer.hidden);
        if group.boolean.is_some()
            && let Some(g) = booleans
                .get(&page.hierarchy, &page.shapes, *id)
                .filter(|g| !g.contours.is_empty())
        {
            let mut rect = g.shape.rect;
            if let Some(origin) = group.board.and_then(|id| boards.get(&id)) {
                rect.x += origin.x;
                rect.y += origin.y;
            }
            rects.insert(*id, rect);
        }
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
            let minimum = if flowing || own.is_some() {
                1_f32
            } else {
                0_f32
            };
            let dimension = |mode, value| match mode {
                Mode::Fixed => length(value),
                Mode::Hug => auto(),
                Mode::Fill if flowing => auto(),
                Mode::Fill => length(value),
            };
            let mut style = Style {
                display: if self.hidden[&id] {
                    Display::None
                } else if own.is_some_and(|layout| layout.axis == Axis::Grid) {
                    Display::Grid
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
                    width: length(sizing.limits.min_width.unwrap_or(minimum).max(minimum)),
                    height: length(sizing.limits.min_height.unwrap_or(minimum).max(minimum)),
                },
                max_size: Size {
                    width: length(sizing.limits.max_width.unwrap_or(MAX_SIZE)),
                    height: length(sizing.limits.max_height.unwrap_or(MAX_SIZE)),
                },
                flex_shrink: 0.,
                ..Default::default()
            };
            if let Some(layout) = parent_layout.filter(|_| flowing) {
                if layout.axis == Axis::Grid {
                    style.grid_column = taffy::Line {
                        start: auto(),
                        end: span(sizing.column_span.min(layout.columns)),
                    };
                    style.grid_row = taffy::Line {
                        start: auto(),
                        end: span(sizing.row_span),
                    };
                    if sizing.width == Mode::Fill {
                        style.justify_self = Some(AlignItems::Stretch);
                    }
                    if sizing.height == Mode::Fill {
                        style.align_self = Some(AlignItems::Stretch);
                    }
                } else {
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
                if layout.axis == Axis::Grid {
                    let track = match layout.column_width {
                        Some(width) => length(width),
                        None => minmax(length(0_f32), fr(1_f32)),
                    };
                    style.grid_template_columns = vec![repeat(layout.columns, vec![track])];
                    style.grid_auto_rows = vec![auto()];
                    style.gap = Size {
                        width: length(layout.gap),
                        height: length(layout.line_gap),
                    };
                    style.justify_content = Some(JustifyContent::Start);
                    style.justify_items = Some(match layout.main {
                        Align::Start => AlignItems::Start,
                        Align::Center => AlignItems::Center,
                        Align::End => AlignItems::End,
                    });
                }
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
    let mut grids = BTreeMap::new();
    for (id, layout) in &page.hierarchy.layouts {
        let Some(node) = tree.nodes.get(id) else {
            continue;
        };
        let taffy::DetailedLayoutInfo::Grid(info) = tree.taffy.detailed_layout_info(*node) else {
            continue;
        };
        let tracks = |sizes: &[f32], gutters: &[f32], mut offset: f32| {
            sizes
                .iter()
                .enumerate()
                .map(|(index, size)| {
                    offset += gutters.get(index).copied().unwrap_or_default();
                    let start = offset;
                    offset += size;
                    [start, offset]
                })
                .collect()
        };
        grids.insert(
            *id,
            GridTracks {
                bounds: output[id],
                columns: tracks(
                    &info.columns.sizes,
                    &info.columns.gutters,
                    layout.padding[3],
                ),
                rows: tracks(&info.rows.sizes, &info.rows.gutters, layout.padding[0]),
            },
        );
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
    Ok(grids)
}
