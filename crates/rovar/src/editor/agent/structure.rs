use super::model;
use crate::{
    agent::protocol::{self as p, Error, Result},
    document::Page,
    scene::{bezier, boolean, components, shape::ShapeKind},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate(page: &Page) -> Result<()> {
    for (id, group) in &page.hierarchy.groups {
        if let Some(source) = group.mask
            && !(boolean::is_boolean(&page.hierarchy, source)
                || page
                    .shapes
                    .iter()
                    .any(|s| s.id == source && boolean::supported(s)))
        {
            return Err(Error::invalid("Mask source must remain a closed vector"));
        }
        if group.boolean.is_some()
            && (group.mask.is_some()
                || page.hierarchy.layouts.contains_key(id)
                || components::ids(page).iter().any(|child| {
                    components::parent(page, *child) == Some(*id)
                        && !(boolean::is_boolean(&page.hierarchy, *child)
                            || page
                                .shapes
                                .iter()
                                .any(|s| s.id == *child && boolean::supported(s)))
                }))
        {
            return Err(Error::invalid(
                "Boolean operands must remain closed vectors",
            ));
        }
    }
    Ok(())
}

fn point(value: p::Point) -> Result<gpui::Point<f32>> {
    if [value.x, value.y]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
    {
        return Err(Error::invalid(
            "Coordinates must be finite and within +/-1000000",
        ));
    }
    Ok(gpui::point(value.x, value.y))
}

pub(super) fn set_path(page: &mut Page, id: usize, path: &p::Path) -> Result<()> {
    if !(2..=4096).contains(&path.nodes.len()) {
        return Err(Error::invalid("A path requires 2 to 4096 nodes"));
    }
    let nodes = path
        .nodes
        .iter()
        .map(|n| {
            Ok(bezier::Node {
                anchor: point(n.anchor)?,
                incoming: point(n.incoming.unwrap_or(n.anchor))?,
                outgoing: point(n.outgoing.unwrap_or(n.anchor))?,
                smooth: n.smooth,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let shape = page
        .shapes
        .iter_mut()
        .find(|s| s.id == id && s.kind == ShapeKind::Bezier)
        .ok_or_else(|| Error::invalid("path requires a bezier node"))?;
    shape.nodes = bezier::Nodes(std::sync::Arc::new(nodes));
    shape.closed = path.closed;
    Ok(())
}

fn subtree_editable(page: &Page, id: usize) -> Result<BTreeSet<usize>> {
    let ids = components::subtree(page, id);
    for id in &ids {
        model::editable(page, *id)?;
    }
    Ok(ids)
}

fn target_board(page: &Page, parent: Option<usize>) -> Result<Option<usize>> {
    let Some(parent) = parent else {
        return Ok(None);
    };
    model::editable(page, parent)?;
    if page.boards.iter().any(|b| b.id == parent) {
        return Ok(Some(parent));
    }
    if !page.hierarchy.groups.contains_key(&parent) {
        return Err(Error::invalid("Parent must be a frame or group"));
    }
    Ok(model::board(page, parent))
}

pub(super) fn reparent(
    page: &mut Page,
    id: usize,
    parent: Option<usize>,
    before: Option<usize>,
) -> Result<()> {
    let ids = subtree_editable(page, id)?;
    if parent.is_some_and(|id| ids.contains(&id)) {
        return Err(Error::invalid(
            "A subtree cannot be parented to itself or a descendant",
        ));
    }
    let owner = |mut ancestor: Option<usize>| {
        while let Some(id) = ancestor {
            if page.hierarchy.components.contains_key(&id) {
                return Some(id);
            }
            ancestor = components::parent(page, id);
        }
        None
    };
    let old_owner = owner(components::parent(page, id));
    let new_owner = owner(parent);
    if old_owner != new_owner && old_owner.is_some_and(|id| !page.hierarchy.components[&id].master)
    {
        return Err(Error::new(
            "COMPONENT_BOUNDARY",
            "Detach the instance before moving its members outside the instance",
        ));
    }
    if new_owner.is_some()
        && ids
            .iter()
            .any(|id| page.hierarchy.components.contains_key(id))
    {
        return Err(Error::invalid("Nested components are not supported"));
    }
    let external = target_board(page, parent)?;
    if external.is_some() && page.boards.iter().any(|b| ids.contains(&b.id)) {
        return Err(Error::invalid("Frames cannot be nested inside frames"));
    }
    if let Some(before) = before {
        if ids.contains(&before) || components::parent(page, before) != parent {
            return Err(Error::invalid(
                "before must be a different sibling at the destination",
            ));
        }
        model::editable(page, before)?;
    }
    let old_origin = |board: Option<usize>| {
        board
            .and_then(|id| page.boards.iter().find(|b| b.id == id))
            .map_or(gpui::point(0., 0.), |b| gpui::point(b.rect.x, b.rect.y))
    };
    let origins: BTreeMap<_, _> = ids
        .iter()
        .map(|id| (*id, old_origin(model::board(page, *id))))
        .collect();
    let destination = old_origin(external);
    for shape in &mut page.shapes {
        if ids.contains(&shape.id) && shape.board.is_none_or(|id| !ids.contains(&id)) {
            let delta = origins[&shape.id] - destination;
            shape.rect.x += delta.x;
            shape.rect.y += delta.y;
            shape.board = external;
        }
    }
    for text in &mut page.texts {
        if ids.contains(&text.id) && text.board.is_none_or(|id| !ids.contains(&id)) {
            let delta = origins[&text.id] - destination;
            text.rect.x += delta.x;
            text.rect.y += delta.y;
            text.board = external;
        }
    }
    for (child, group) in &mut page.hierarchy.groups {
        if ids.contains(child) && group.board.is_none_or(|id| !ids.contains(&id)) {
            if let Some(layout) = page.hierarchy.layouts.get_mut(child) {
                let delta = origins[child] - destination;
                layout.frame.x += delta.x;
                layout.frame.y += delta.y;
            }
            group.board = external;
        }
    }
    page.hierarchy.parents.remove(&id);
    if let Some(parent) = parent.filter(|id| page.hierarchy.groups.contains_key(id)) {
        page.hierarchy.parents.insert(id, parent);
    }
    let mut moved: Vec<_> = page
        .hierarchy
        .order
        .iter()
        .filter(|id| ids.contains(id))
        .copied()
        .collect();
    for id in &ids {
        if !moved.contains(id) {
            moved.push(*id);
        }
    }
    page.hierarchy.order.retain(|id| !ids.contains(id));
    let index = before
        .and_then(|id| page.hierarchy.order.iter().position(|v| *v == id))
        .unwrap_or(page.hierarchy.order.len());
    page.hierarchy.order.splice(index..index, moved);
    Ok(())
}

fn mapping(page: &mut Page, ids: BTreeSet<usize>) -> Result<BTreeMap<usize, usize>> {
    let end = page
        .next_id
        .checked_add(ids.len())
        .ok_or_else(|| Error::invalid("Node limit reached"))?;
    let result = ids.into_iter().zip(page.next_id..end).collect();
    page.next_id = end;
    Ok(result)
}

fn append(page: &mut Page, mut fragment: Page) {
    page.boards.append(&mut fragment.boards);
    page.shapes.append(&mut fragment.shapes);
    page.texts.append(&mut fragment.texts);
    page.assets.append(&mut fragment.assets);
    macro_rules! maps { ($($name:ident),*) => {$(page.hierarchy.$name.append(&mut fragment.hierarchy.$name);)*}; }
    maps!(
        groups,
        parents,
        names,
        layouts,
        sizing,
        exports,
        effects,
        interactions,
        components
    );
    page.hierarchy.order.append(&mut fragment.hierarchy.order);
}

pub(super) fn duplicate(page: &mut Page, id: usize, offset: p::Point) -> Result<usize> {
    let offset = point(offset)?;
    let ids = subtree_editable(page, id)?;
    let parent = components::parent(page, id);
    let bounds =
        components::bounds(page, id).ok_or_else(|| Error::invalid("Missing subtree bounds"))?;
    let mut template = components::extract(page, id).map_err(|e| Error::invalid(e.to_string()))?;
    if let Some(sizing) = page.hierarchy.sizing.get(&id) {
        template.hierarchy.sizing.insert(id, *sizing);
    }
    let bindings: Vec<_> = page
        .hierarchy
        .components
        .iter()
        .filter(|(root, _)| ids.contains(root))
        .map(|(root, b)| (*root, b.clone()))
        .collect();
    let mut allocated = ids.clone();
    for (_, binding) in &bindings {
        allocated.extend(binding.nodes.values());
    }
    let nodes = mapping(page, allocated)?;
    let mut fragment = components::place(
        &template,
        &nodes,
        [bounds.x + offset.x, bounds.y + offset.y],
        None,
    );
    fragment.renew_node_ids();
    // extract/place intentionally discard external links for library templates.
    // A same-page copy keeps those links and remaps only internal destinations.
    fragment.hierarchy.interactions = page
        .hierarchy
        .interactions
        .iter()
        .filter(|(id, _)| ids.contains(id))
        .filter_map(|(id, action)| {
            Some((
                nodes[id],
                action.remap(|target| Some(nodes.get(&target).copied().unwrap_or(target)))?,
            ))
        })
        .collect();
    for (root, mut binding) in bindings {
        binding.master = false;
        for id in binding.nodes.values_mut() {
            *id = nodes[id];
        }
        fragment.hierarchy.components.insert(nodes[&root], binding);
    }
    let root = nodes[&id];
    append(page, fragment);
    reparent(page, root, parent, None)?;
    Ok(root)
}

pub(super) fn set_mask(page: &mut Page, id: usize, source: Option<usize>) -> Result<()> {
    model::editable(page, id)?;
    let group = page
        .hierarchy
        .groups
        .get(&id)
        .ok_or_else(|| Error::invalid("Mask requires a group"))?;
    if let Some(source) = source {
        model::editable(page, source)?;
        if group.boolean.is_some()
            || page.hierarchy.layouts.contains_key(&id)
            || page.hierarchy.parents.get(&source) != Some(&id)
            || !(boolean::is_boolean(&page.hierarchy, source)
                || page
                    .shapes
                    .iter()
                    .any(|s| s.id == source && boolean::supported(s)))
        {
            return Err(Error::invalid(
                "Mask source must be a closed vector direct child; remove boolean/auto layout first",
            ));
        }
    }
    page.hierarchy.groups.get_mut(&id).unwrap().mask = source;
    Ok(())
}

pub(super) fn set_boolean(
    page: &mut Page,
    id: usize,
    operation: Option<boolean::Operation>,
) -> Result<()> {
    model::editable(page, id)?;
    let group = page
        .hierarchy
        .groups
        .get(&id)
        .ok_or_else(|| Error::invalid("Boolean requires a group"))?;
    if operation.is_some() {
        let children: Vec<_> = components::ids(page)
            .into_iter()
            .filter(|child| components::parent(page, *child) == Some(id))
            .collect();
        if group.mask.is_some()
            || page.hierarchy.layouts.contains_key(&id)
            || children.len() < 2
            || children.iter().any(|id| {
                !(boolean::is_boolean(&page.hierarchy, *id)
                    || page
                        .shapes
                        .iter()
                        .any(|s| s.id == *id && boolean::supported(s)))
            })
        {
            return Err(Error::invalid(
                "Boolean groups require at least two closed vector operands and no mask or auto layout",
            ));
        }
        for child in children {
            model::editable(page, child)?;
        }
    }
    page.hierarchy.groups.get_mut(&id).unwrap().boolean = operation;
    Ok(())
}

pub(super) fn create_component(
    page: &mut Page,
    definitions: &mut components::Definitions,
    id: usize,
    name: String,
) -> Result<String> {
    let ids = subtree_editable(page, id)?;
    let mut ancestor = components::parent(page, id);
    while let Some(id) = ancestor {
        if page.hierarchy.components.contains_key(&id) {
            return Err(Error::invalid("Nested components are not supported"));
        }
        ancestor = components::parent(page, id);
    }
    if name.trim().is_empty()
        || name.chars().count() > 200
        || ids
            .iter()
            .any(|id| page.hierarchy.components.contains_key(id))
    {
        return Err(Error::invalid(
            "Component requires a name and an unbound subtree",
        ));
    }
    let template = components::extract(page, id).map_err(|e| Error::invalid(e.to_string()))?;
    let component = uuid::Uuid::new_v4().to_string();
    let baseline = serde_json::to_value(&template).map_err(|e| Error::invalid(e.to_string()))?;
    page.hierarchy.components.insert(
        id,
        components::Binding {
            component: component.clone(),
            master: true,
            nodes: ids.into_iter().map(|id| (id, id)).collect(),
            baseline,
        },
    );
    definitions.insert(
        component.clone(),
        components::Definition {
            name,
            source: None,
            root: id,
            page: template,
        },
    );
    Ok(component)
}

pub(super) fn instantiate(
    page: &mut Page,
    definition: &components::Definition,
    component: String,
    parent: Option<usize>,
    position: p::Point,
) -> Result<usize> {
    let position = point(position)?;
    target_board(page, parent)?;
    let nodes = mapping(page, components::ids(&definition.page))?;
    let mut fragment = components::place(&definition.page, &nodes, [position.x, position.y], None);
    fragment.renew_node_ids();
    let root = nodes[&definition.root];
    fragment.hierarchy.components.insert(
        root,
        components::Binding {
            component,
            master: false,
            nodes,
            baseline: serde_json::to_value(&definition.page)
                .map_err(|e| Error::invalid(e.to_string()))?,
        },
    );
    append(page, fragment);
    reparent(page, root, parent, None)?;
    Ok(root)
}
