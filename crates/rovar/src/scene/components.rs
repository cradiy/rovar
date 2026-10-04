use crate::{document::Page, scene::artboard::Rect};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) mod variants;

pub(crate) type Definitions = BTreeMap<String, Definition>;

pub(crate) fn sources(page: &Page) -> Vec<crate::document::AssetSource> {
    page.boards
        .iter()
        .filter_map(|b| b.image_fill.asset.as_ref())
        .chain(page.shapes.iter().flat_map(|s| {
            [s.media.as_ref(), s.image_fill.asset.as_ref()]
                .into_iter()
                .flatten()
        }))
        .map(|a| {
            (
                a.hash.clone(),
                crate::document::AssetSource {
                    hash: a.hash.clone(),
                    name: a.name(),
                    path: a.source.clone(),
                    size: [a.width, a.height],
                },
            )
        })
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Definition {
    pub name: String,
    pub source: Option<String>,
    pub root: usize,
    pub page: Page,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Binding {
    pub component: String,
    pub master: bool,
    /// Definition object IDs mapped to this page's object IDs.
    pub nodes: BTreeMap<usize, usize>,
    pub baseline: serde_json::Value,
}

pub(crate) fn ids(page: &Page) -> BTreeSet<usize> {
    page.boards
        .iter()
        .map(|x| x.id)
        .chain(page.shapes.iter().map(|x| x.id))
        .chain(page.texts.iter().map(|x| x.id))
        .chain(page.hierarchy.groups.keys().copied())
        .collect()
}
pub(crate) fn parent(page: &Page, id: usize) -> Option<usize> {
    page.hierarchy
        .parents
        .get(&id)
        .copied()
        .or_else(|| board(page, id))
}
fn board(page: &Page, id: usize) -> Option<usize> {
    page.shapes
        .iter()
        .find(|x| x.id == id)
        .and_then(|x| x.board)
        .or_else(|| page.texts.iter().find(|x| x.id == id).and_then(|x| x.board))
        .or_else(|| page.hierarchy.groups.get(&id).and_then(|x| x.board))
}
fn origin(page: &Page, id: Option<usize>) -> [f32; 2] {
    id.and_then(|id| page.boards.iter().find(|b| b.id == id))
        .map_or([0., 0.], |b| [b.rect.x, b.rect.y])
}
pub(crate) fn bounds(page: &Page, id: usize) -> Option<Rect> {
    if let Some(b) = page.boards.iter().find(|b| b.id == id) {
        return Some(b.rect);
    }
    let rect = page
        .shapes
        .iter()
        .find(|x| x.id == id)
        .map(|x| x.rect)
        .or_else(|| page.texts.iter().find(|x| x.id == id).map(|x| x.rect))
        .or_else(|| page.hierarchy.layouts.get(&id).map(|x| x.frame));
    if let Some(mut rect) = rect {
        let offset = origin(page, board(page, id));
        rect.x += offset[0];
        rect.y += offset[1];
        return Some(rect);
    }
    let children = ids(page)
        .into_iter()
        .filter(|child| parent(page, *child) == Some(id))
        .filter_map(|child| bounds(page, child))
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
    children.or_else(|| {
        page.hierarchy.groups.contains_key(&id).then_some(Rect {
            x: 0.,
            y: 0.,
            width: 0.,
            height: 0.,
        })
    })
}
pub(crate) fn subtree(page: &Page, root: usize) -> BTreeSet<usize> {
    let all = ids(page);
    let mut included = BTreeSet::from([root]);
    loop {
        let old = included.len();
        for id in &all {
            if parent(page, *id).is_some_and(|p| included.contains(&p)) {
                included.insert(*id);
            }
        }
        if old == included.len() {
            return included;
        }
    }
}
pub(crate) fn extract(page: &Page, root: usize) -> anyhow::Result<Page> {
    let bounds = bounds(page, root).ok_or_else(|| anyhow::anyhow!("Missing component root"))?;
    let included = subtree(page, root);
    let mut out = page.clone();
    out.boards.retain(|x| included.contains(&x.id));
    out.shapes.retain(|x| included.contains(&x.id));
    out.texts.retain(|x| included.contains(&x.id));
    for b in &mut out.boards {
        b.rect.x -= bounds.x;
        b.rect.y -= bounds.y;
    }
    for s in &mut out.shapes {
        if s.board.is_none_or(|id| !included.contains(&id)) {
            let o = origin(page, s.board);
            s.rect.x += o[0] - bounds.x;
            s.rect.y += o[1] - bounds.y;
            s.board = None;
        }
    }
    for t in &mut out.texts {
        if t.board.is_none_or(|id| !included.contains(&id)) {
            let o = origin(page, t.board);
            t.rect.x += o[0] - bounds.x;
            t.rect.y += o[1] - bounds.y;
            t.board = None;
        }
    }
    out.hierarchy.groups.retain(|id, _| included.contains(id));
    for (id, g) in &mut out.hierarchy.groups {
        if g.board.is_none_or(|b| !included.contains(&b)) {
            if let Some(layout) = out.hierarchy.layouts.get_mut(id) {
                let o = origin(page, g.board);
                layout.frame.x += o[0] - bounds.x;
                layout.frame.y += o[1] - bounds.y;
            }
            g.board = None;
        }
    }
    out.hierarchy
        .parents
        .retain(|id, p| included.contains(id) && included.contains(p));
    out.hierarchy.names.retain(|id, _| included.contains(id));
    out.hierarchy.layouts.retain(|id, _| included.contains(id));
    out.hierarchy.sizing.retain(|id, _| included.contains(id));
    if let Some(sizing) = out.hierarchy.sizing.get_mut(&root) {
        sizing.constraints = None;
    }
    out.hierarchy.exports.retain(|id, _| included.contains(id));
    out.hierarchy.effects.retain(|id, _| included.contains(id));
    out.hierarchy.order.retain(|id| included.contains(id));
    out.hierarchy.components.clear();
    out.assets.retain(|a| included.contains(&a.object));
    Ok(out)
}

/// Remap a normalized template into a page without retaining library paths.
pub(crate) fn place(
    template: &Page,
    nodes: &BTreeMap<usize, usize>,
    offset: [f32; 2],
    external: Option<usize>,
) -> Page {
    let mut out = template.clone();
    for b in &mut out.boards {
        b.id = nodes[&b.id];
        b.rect.x += offset[0];
        b.rect.y += offset[1];
    }
    for s in &mut out.shapes {
        s.id = nodes[&s.id];
        if s.board.is_none() {
            s.rect.x += offset[0];
            s.rect.y += offset[1];
        }
        s.board = s.board.map(|p| nodes[&p]).or(external);
    }
    for t in &mut out.texts {
        t.id = nodes[&t.id];
        if t.board.is_none() {
            t.rect.x += offset[0];
            t.rect.y += offset[1];
        }
        t.board = t.board.map(|p| nodes[&p]).or(external);
    }
    out.hierarchy.groups = template
        .hierarchy
        .groups
        .iter()
        .map(|(id, g)| {
            let mut g = g.clone();
            g.board = g.board.map(|p| nodes[&p]).or(external);
            (nodes[id], g)
        })
        .collect();
    out.hierarchy.parents = template
        .hierarchy
        .parents
        .iter()
        .map(|(id, p)| (nodes[id], nodes[p]))
        .collect();
    out.hierarchy.names = template
        .hierarchy
        .names
        .iter()
        .map(|(id, n)| (nodes[id], n.clone()))
        .collect();
    out.hierarchy.layouts = template
        .hierarchy
        .layouts
        .iter()
        .map(|(id, l)| {
            let mut l = l.clone();
            if template
                .hierarchy
                .groups
                .get(id)
                .is_some_and(|g| g.board.is_none())
            {
                l.frame.x += offset[0];
                l.frame.y += offset[1];
            }
            (nodes[id], l)
        })
        .collect();
    out.hierarchy.sizing = template
        .hierarchy
        .sizing
        .iter()
        .map(|(id, s)| (nodes[id], *s))
        .collect();
    out.hierarchy.exports = template
        .hierarchy
        .exports
        .iter()
        .map(|(id, presets)| (nodes[id], presets.clone()))
        .collect();
    out.hierarchy.effects = template
        .hierarchy
        .effects
        .iter()
        .map(|(id, effects)| (nodes[id], effects.clone()))
        .collect();
    out.hierarchy.order = template
        .hierarchy
        .order
        .iter()
        .map(|id| nodes[id])
        .collect();
    out.hierarchy.components.clear();
    for a in &mut out.assets {
        a.object = nodes[&a.object];
    }
    out.next_id = nodes.values().max().copied().unwrap_or(0) + 1;
    out
}

fn merge(
    old: &serde_json::Value,
    new: &serde_json::Value,
    current: &serde_json::Value,
) -> serde_json::Value {
    if old == current {
        return new.clone();
    }
    if let (Some(old), Some(new), Some(current)) =
        (old.as_object(), new.as_object(), current.as_object())
    {
        let mut result = current.clone();
        for key in old.keys().chain(new.keys()).collect::<BTreeSet<_>>() {
            let value = merge(
                old.get(key).unwrap_or(&serde_json::Value::Null),
                new.get(key).unwrap_or(&serde_json::Value::Null),
                current.get(key).unwrap_or(&serde_json::Value::Null),
            );
            if value.is_null() {
                result.remove(key);
            } else {
                result.insert(key.clone(), value);
            }
        }
        return result.into();
    }
    current.clone()
}

fn merge_objects<T: Serialize + serde::de::DeserializeOwned + Clone>(
    current: &mut Vec<T>,
    old: &[T],
    new: &[T],
    id: impl Fn(&T) -> usize,
) -> anyhow::Result<()> {
    let old: BTreeMap<_, _> = old.iter().map(|x| (id(x), x)).collect();
    let next: BTreeMap<_, _> = new.iter().map(|x| (id(x), x)).collect();
    current.retain(|x| !old.contains_key(&id(x)) || next.contains_key(&id(x)));
    for (key, new) in next {
        if let Some(item) = current.iter_mut().find(|x| id(x) == key) {
            if let Some(old) = old.get(&key) {
                let mut before = serde_json::to_value(old)?;
                let after = serde_json::to_value(new)?;
                let now = serde_json::to_value(&*item)?;
                // Text overrides own their style ranges as well as their bytes.
                if before.get("content") != now.get("content") {
                    before["styles"] = serde_json::Value::Null;
                }
                *item = serde_json::from_value(merge(&before, &after, &now))?;
            }
        } else if !old.contains_key(&key) {
            current.push(new.clone());
        }
    }
    Ok(())
}

pub(crate) fn synchronize(pages: &mut [Page], definitions: &mut Definitions) -> anyhow::Result<()> {
    // Refresh document masters before updating instances on any page.
    for page in pages.iter_mut() {
        for (root, mut link) in page.hierarchy.components.clone() {
            if !link.master || !ids(page).contains(&root) {
                continue;
            }
            let Some(definition) = definitions.get_mut(&link.component) else {
                continue;
            };
            let template = extract(page, root)?;
            let mut inverse: BTreeMap<_, _> = link.nodes.iter().map(|(a, b)| (*b, *a)).collect();
            let mut next = definition
                .page
                .next_id
                .max(link.nodes.keys().last().copied().unwrap_or(0) + 1);
            for id in ids(&template) {
                inverse.entry(id).or_insert_with(|| {
                    let id = next;
                    next += 1;
                    id
                });
            }
            definition.page = place(&template, &inverse, [0., 0.], None);
            definition.page.next_id = next.max(definition.page.next_id);
            link.nodes = inverse.into_iter().map(|(a, b)| (b, a)).collect();
            link.baseline = serde_json::to_value(&definition.page)?;
            page.hierarchy.components.insert(root, link);
        }
    }
    // Instances of the same component share one serialized baseline per pass.
    let mut baselines = BTreeMap::new();
    for page in pages {
        for (root, mut link) in page.hierarchy.components.clone() {
            if link.master || !ids(page).contains(&root) {
                continue;
            }
            let Some(definition) = definitions.get(&link.component) else {
                continue;
            };
            let baseline = match baselines.entry(link.component.clone()) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(serde_json::to_value(&definition.page)?)
                }
            };
            if *baseline == link.baseline {
                continue;
            }
            let mut old: Page = serde_json::from_value(link.baseline.clone())?;
            old.upgrade_node_ids();
            anyhow::ensure!(
                ids(&old).iter().all(|id| link.nodes.contains_key(id)),
                "Incomplete component mapping"
            );
            let Some(rect) = bounds(page, root) else {
                continue;
            };
            let external = board(page, root);
            let o = origin(page, external);
            let offset = [rect.x - o[0], rect.y - o[1]];
            let root_uid = page.node_ids()[&root];
            let mut previous = place(&old, &link.nodes, offset, external);
            previous.instance_node_ids(root_uid);
            for id in ids(&definition.page) {
                link.nodes.entry(id).or_insert_with(|| {
                    let id = page.next_id;
                    page.next_id += 1;
                    id
                });
            }
            let mut next = place(&definition.page, &link.nodes, offset, external);
            next.instance_node_ids(root_uid);
            // Preserve real media handles across JSON field merging.
            let media: BTreeMap<_, _> = page
                .boards
                .iter()
                .filter_map(|b| b.image_fill.asset.clone())
                .chain(page.shapes.iter().flat_map(|s| {
                    [s.media.clone(), s.image_fill.asset.clone()]
                        .into_iter()
                        .flatten()
                }))
                .chain(
                    definition
                        .page
                        .boards
                        .iter()
                        .filter_map(|b| b.image_fill.asset.clone()),
                )
                .chain(definition.page.shapes.iter().flat_map(|s| {
                    [s.media.clone(), s.image_fill.asset.clone()]
                        .into_iter()
                        .flatten()
                }))
                .map(|a| (a.hash.clone(), a))
                .collect();
            merge_objects(&mut page.boards, &previous.boards, &next.boards, |x| x.id)?;
            merge_objects(&mut page.shapes, &previous.shapes, &next.shapes, |x| x.id)?;
            merge_objects(&mut page.texts, &previous.texts, &next.texts, |x| x.id)?;
            macro_rules! maps { ($($field:ident),*) => {$({
                let keys: BTreeSet<_>=previous.hierarchy.$field.keys().chain(next.hierarchy.$field.keys()).copied().collect();
                for id in keys {
                    let before=serde_json::to_value(previous.hierarchy.$field.get(&id))?;
                    let after=serde_json::to_value(next.hierarchy.$field.get(&id))?;
                    let now=serde_json::to_value(page.hierarchy.$field.get(&id))?;
                    let value=merge(&before,&after,&now);
                    if value.is_null() { page.hierarchy.$field.remove(&id); }
                    else { page.hierarchy.$field.insert(id,serde_json::from_value(value)?); }
                }
            })*}; }
            maps!(groups, parents, names, layouts, sizing, exports, effects);
            let removed: BTreeSet<_> = ids(&previous).difference(&ids(&next)).copied().collect();
            page.hierarchy.groups.retain(|id, _| !removed.contains(id));
            let old_assets: BTreeMap<_, _> = previous
                .assets
                .iter()
                .map(|a| ((a.object, a.fill), &a.hash))
                .collect();
            page.assets.retain(|a| {
                old_assets.get(&(a.object, a.fill)) != Some(&&a.hash)
                    || next
                        .assets
                        .iter()
                        .any(|n| n.object == a.object && n.fill == a.fill)
            });
            for a in &next.assets {
                let current = page
                    .assets
                    .iter()
                    .position(|x| x.object == a.object && x.fill == a.fill);
                if let Some(i) = current {
                    if old_assets.get(&(a.object, a.fill)) == Some(&&page.assets[i].hash) {
                        page.assets[i] = a.clone();
                    }
                } else if !old_assets.contains_key(&(a.object, a.fill)) {
                    page.assets.push(a.clone());
                }
            }
            let live = ids(page);
            page.hierarchy.parents.retain(|id, parent| {
                live.contains(id) && page.hierarchy.groups.contains_key(parent)
            });
            page.hierarchy.names.retain(|id, _| live.contains(id));
            page.hierarchy.layouts.retain(|id, _| live.contains(id));
            page.hierarchy.sizing.retain(|id, _| live.contains(id));
            page.hierarchy.exports.retain(|id, _| live.contains(id));
            page.hierarchy.effects.retain(|id, _| live.contains(id));
            page.assets.retain(|a| live.contains(&a.object));
            for a in &page.assets {
                let asset = media.get(&a.hash).cloned();
                if a.fill {
                    if let Some(b) = page.boards.iter_mut().find(|b| b.id == a.object) {
                        b.image_fill.asset = asset.clone();
                    }
                    if let Some(s) = page.shapes.iter_mut().find(|s| s.id == a.object) {
                        s.image_fill.asset = asset;
                    }
                } else if let Some(s) = page.shapes.iter_mut().find(|s| s.id == a.object) {
                    s.media = asset;
                }
            }
            let old_ids = ids(&previous);
            let ordered = |p: &Page| {
                let mut order = p.hierarchy.order.clone();
                for id in ids(p) {
                    if !order.contains(&id) {
                        order.push(id);
                    }
                }
                order
            };
            let old_order: Vec<_> = ordered(&previous)
                .into_iter()
                .filter(|id| live.contains(id))
                .collect();
            let current_order: Vec<_> = ordered(page)
                .into_iter()
                .filter(|id| old_ids.contains(id) && live.contains(id))
                .collect();
            let next_order: Vec<_> = ordered(&next)
                .into_iter()
                .filter(|id| live.contains(id))
                .collect();
            if old_order == current_order {
                let mut order = ordered(page);
                let insertion = order
                    .iter()
                    .position(|id| old_ids.contains(id))
                    .unwrap_or(order.len());
                order.retain(|id| !old_ids.contains(id) && !next_order.contains(id));
                let insertion = insertion.min(order.len());
                order.splice(insertion..insertion, next_order.clone());
                page.hierarchy.order = order;
            }
            page.hierarchy.order.retain(|id| live.contains(id));
            for id in &next_order {
                if !page.hierarchy.order.contains(id) {
                    page.hierarchy.order.push(*id);
                }
            }
            link.baseline = baseline.clone();
            page.hierarchy.components.insert(root, link);
        }
    }
    Ok(())
}
