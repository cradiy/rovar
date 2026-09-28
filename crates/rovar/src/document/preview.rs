use super::*;
use crate::scene_render::{self, Output, Scene};

#[cfg(test)]
mod tests;

pub(crate) fn render(
    doc: &Page,
    sources: &[AssetSource],
    text_system: &Arc<gpui::TextSystem>,
) -> Result<Vec<u8>> {
    let mut items = BTreeMap::new();
    for b in &doc.boards {
        items.insert(b.id, (None, b.layer, b.rect));
    }
    for s in &doc.shapes {
        items.insert(s.id, (s.board, s.layer, s.rect));
    }
    for t in &doc.texts {
        items.insert(t.id, (t.board, t.layer, t.rect));
    }
    for (id, g) in &doc.hierarchy.groups {
        items.insert(
            *id,
            (
                g.board,
                g.layer,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 0.,
                    height: 0.,
                },
            ),
        );
    }
    let mut children: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
    for (id, (board, _, _)) in &items {
        children
            .entry(doc.hierarchy.parents.get(id).copied().or(*board))
            .or_default()
            .push(*id);
    }
    let ranks: BTreeMap<_, _> = doc
        .hierarchy
        .order
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();
    for ids in children.values_mut() {
        ids.sort_by_key(|id| ranks.get(id).map_or((1, *id), |rank| (0, *rank)));
    }
    let mut stack = children.get(&None).cloned().unwrap_or_default();
    stack.reverse();
    let mut order = Vec::new();
    while let Some(id) = stack.pop() {
        if items[&id].1.hidden {
            continue;
        }
        if !doc.hierarchy.groups.contains_key(&id) {
            order.push(id);
        }
        if let Some(children) = children.get(&Some(id)) {
            stack.extend(children.iter().rev());
        }
    }
    let world = |id: usize| {
        let (board, _, mut rect) = items[&id];
        if let Some(board) = board {
            rect.x += items[&board].2.x;
            rect.y += items[&board].2.y;
        }
        rect
    };
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for id in &order {
        let mut rect = world(*id);
        if let Some(shape) = doc.shapes.iter().find(|s| s.id == *id && s.stroke.enabled) {
            let outset = if shape.kind.is_path() || shape.kind.is_polygon() {
                shape.stroke.width / 2.
            } else {
                shape.stroke.outset()
            };
            rect.x -= outset;
            rect.y -= outset;
            rect.width += outset * 2.;
            rect.height += outset * 2.;
        }
        let rect = crate::rotation::bounds(rect, items[id].1.rotation);
        bounds = Some(bounds.map_or(
            (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height),
            |(l, t, r, b)| {
                (
                    l.min(rect.x),
                    t.min(rect.y),
                    r.max(rect.x + rect.width),
                    b.max(rect.y + rect.height),
                )
            },
        ));
    }
    let (l, t, r, b) = bounds.unwrap_or((0., 0., 560., 336.));
    let pad = ((r - l).max(b - t) * 0.06).max(4.);

    // A separate layout cache keeps background preview work out of the window's frame cache.
    let text_system = gpui::WindowTextSystem::new(text_system.clone());
    let text = doc
        .texts
        .iter()
        .filter(|t| order.contains(&t.id))
        .map(|t| {
            (
                t.id,
                crate::text::export_fragments(&t.content, &t.styles, t.rect, &text_system),
            )
        })
        .collect();
    let clips = order
        .iter()
        .filter_map(|id| items[id].0.map(|board| (*id, items[&board].2)))
        .collect();
    let svg = Scene {
        document: doc,
        assets: sources,
        order: &order,
        bounds: Rect {
            x: l - pad,
            y: t - pad,
            width: (r - l + pad * 2.).max(1.),
            height: (b - t + pad * 2.).max(1.),
        },
        clips: &clips,
        text: &text,
    }
    .svg(Output::Preview)?;
    let options = scene_render::render_options(!doc.texts.is_empty())?;
    let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
    let mut pixels = resvg::tiny_skia::Pixmap::new(560, 336).context("Invalid thumbnail size")?;
    pixels.fill(resvg::tiny_skia::Color::from_rgba8(18, 20, 25, 255));
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
    );
    Ok(pixels.encode_png()?)
}
