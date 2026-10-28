use super::*;
use crate::render::svg::{self, Output, Scene};

#[cfg(test)]
mod tests;

pub(crate) fn cache(
    path: &Path,
    directory: &Path,
    text_system: &Arc<gpui::TextSystem>,
) -> Result<Option<String>> {
    let Some(original) = super::cache_preview(path, directory)? else {
        return Ok(None);
    };
    let reader = rovar_format::Reader::open(path)?;
    let mut digest = Sha256::new();
    for (key, block) in reader.entries() {
        digest.update(key.len().to_le_bytes());
        digest.update(key.as_bytes());
        digest.update(block.hash);
    }
    let name = format!("{}-transparent.png", hex::encode(digest.finalize()));
    let output = directory.join(&name);
    if rovar_storage::exists(&output) {
        return Ok(Some(name));
    }
    let result = (|| -> Result<Vec<u8>> {
        let bytes = rovar_storage::fs::read(directory.join(&original))?;
        let mut reader =
            image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(560);
        limits.max_image_height = Some(336);
        reader.limits(limits);
        let pixels = reader.decode()?.into_rgba8();
        if pixels.pixels().any(|p| p[3] < 255) {
            return Ok(bytes);
        }
        // Legacy previews baked in a dark canvas. Render the document again;
        // removing matching pixels would also erase artwork of the same color.
        let loaded = super::load(path)?;
        let document = Document::decode(&loaded.json)?;
        let sources = loaded
            .assets
            .values()
            .map(|asset| AssetSource {
                hash: asset.hash.clone(),
                name: asset.name(),
                path: asset.source.clone(),
                size: [asset.width, asset.height],
            })
            .collect::<Vec<_>>();
        render(document.first_page(), &sources, text_system)
    })();
    match result {
        Ok(bytes) => {
            crate::remote::write_atomic(&output, &bytes)?;
            Ok(Some(name))
        }
        Err(error) => {
            eprintln!("Could not refresh document preview: {error:#}");
            Ok(Some(original))
        }
    }
}

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
        if !doc.hierarchy.groups.contains_key(&id)
            || crate::scene::boolean::is_boolean(&doc.hierarchy, id)
            || crate::scene::mask::source(&doc.hierarchy, id).is_some()
            || doc.hierarchy.groups[&id].layer.composited()
        {
            order.push(id);
        }
        if let Some(children) = children.get(&Some(id)) {
            stack.extend(children.iter().rev());
        }
    }
    let included = order.iter().copied().collect();
    order.retain(|id| {
        !crate::scene::boolean::consumed(&doc.hierarchy, *id, &included)
            && !crate::scene::mask::is_source(&doc.hierarchy, *id, &included)
    });
    let mut boolean_cache = crate::scene::boolean::Cache::default();
    let masks =
        crate::scene::mask::outlines(&doc.hierarchy, &doc.shapes, &doc.boards, &mut boolean_cache);
    for (id, outline) in &masks {
        if let Some(bounds) = outline.bounds() {
            items.insert(*id, (None, doc.hierarchy.groups[id].layer, bounds));
        }
    }
    let mut compositions = BTreeMap::new();
    for id in &order {
        if crate::scene::boolean::is_boolean(&doc.hierarchy, *id)
            && let Some(g) = boolean_cache.get(&doc.hierarchy, &doc.shapes, *id)
        {
            items.insert(*id, (g.shape.board, g.shape.layer, g.shape.rect));
            compositions.insert(*id, g);
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
        if doc.hierarchy.groups.contains_key(id)
            && !crate::scene::boolean::is_boolean(&doc.hierarchy, *id)
            && crate::scene::mask::source(&doc.hierarchy, *id).is_none()
        {
            continue;
        }
        let mut rect = world(*id);
        if let Some(shape) = doc
            .shapes
            .iter()
            .find(|s| s.id == *id)
            .or_else(|| compositions.get(id).map(|g| &g.shape))
        {
            let outset = shape.stroke_outset();
            rect.x -= outset;
            rect.y -= outset;
            rect.width += outset * 2.;
            rect.height += outset * 2.;
        }
        let original_center = crate::scene::rotation::center(rect);
        if let Some(effects) = doc.hierarchy.effects.get(id) {
            rect = crate::scene::effects::bounds(rect, effects);
        }
        let center = crate::scene::rotation::around(
            crate::scene::rotation::center(rect),
            original_center,
            items[id].1.rotation,
        );
        let mut rect = crate::scene::rotation::bounds(rect, items[id].1.rotation);
        rect.x = center.x - rect.width / 2.;
        rect.y = center.y - rect.height / 2.;
        let Some(rect) = crate::scene::mask::clip_bounds(
            rect,
            crate::scene::mask::ancestors(&doc.hierarchy, *id, &included)
                .iter()
                .map(|id| masks.get(id).and_then(|o| o.bounds())),
        ) else {
            continue;
        };
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
                crate::scene::text::export_fragments(&t.content, &t.styles, t.rect, &text_system),
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
    .svg(
        Output::Preview,
        (560. / (r - l + pad * 2.).max(1.)).min(336. / (b - t + pad * 2.).max(1.)),
    )?;
    let options = svg::render_options(!doc.texts.is_empty())?;
    crate::render::raster::render(&svg, [560, 336], 1., false, false, &options)
}
