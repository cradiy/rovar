use crate::{
    agent::protocol::{self as p, Error, Result},
    document::{Page, Text},
    scene::{
        artboard::{Artboard, FillMode, Rect},
        auto_layout, components,
        layer::{LayerGroup, LayerState},
        shape::{Shape, ShapeKind},
        text::{StyleChange, styles::StyledText},
    },
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn lookup(
    page: &Page,
    reference: &str,
    refs: &BTreeMap<String, String>,
) -> Result<usize> {
    let value = refs.get(reference).map_or(reference, String::as_str);
    let uid = uuid::Uuid::parse_str(value).map_err(|_| {
        Error::invalid(format!("Invalid node ID or unknown reference: {reference}"))
    })?;
    page.node_ids()
        .into_iter()
        .find_map(|(id, value)| (uid == value).then_some(id))
        .ok_or_else(|| {
            Error::new(
                "NODE_NOT_FOUND",
                format!("Node {value} does not exist on this page"),
            )
        })
}

pub(super) fn layer(page: &Page, id: usize) -> Option<LayerState> {
    page.boards
        .iter()
        .find(|n| n.id == id)
        .map(|n| n.layer)
        .or_else(|| page.shapes.iter().find(|n| n.id == id).map(|n| n.layer))
        .or_else(|| page.texts.iter().find(|n| n.id == id).map(|n| n.layer))
        .or_else(|| page.hierarchy.groups.get(&id).map(|n| n.layer))
}

pub(super) fn editable(page: &Page, id: usize) -> Result<()> {
    if page.shapes.iter().any(|n| {
        n.id == id
            && !matches!(
                n.kind,
                ShapeKind::Rectangle | ShapeKind::Ellipse | ShapeKind::Bezier
            )
            && !n.kind.is_asset()
    }) {
        return Err(Error::new(
            "UNSUPPORTED",
            "This shape type is read-only in the agent protocol",
        ));
    }
    let mut current = Some(id);
    while let Some(id) = current {
        if layer(page, id).is_some_and(|n| n.locked) {
            return Err(Error::new("LOCKED", "Node or ancestor is locked"));
        }
        if page.hierarchy.components.contains_key(&id)
            || page
                .hierarchy
                .groups
                .get(&id)
                .is_some_and(|g| g.mask.is_some() || g.boolean.is_some())
        {
            return Err(Error::new(
                "UNSUPPORTED",
                "Component, mask and boolean editing is not exposed",
            ));
        }
        current = components::parent(page, id);
    }
    Ok(())
}

pub(super) fn board(page: &Page, id: usize) -> Option<usize> {
    page.shapes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.board)
        .or_else(|| page.texts.iter().find(|n| n.id == id).and_then(|n| n.board))
        .or_else(|| page.hierarchy.groups.get(&id).and_then(|n| n.board))
}

pub(super) fn local_rect(page: &Page, board: Option<usize>, mut rect: Rect) -> Rect {
    if let Some(b) = board.and_then(|id| page.boards.iter().find(|b| b.id == id)) {
        rect.x -= b.rect.x;
        rect.y -= b.rect.y;
    }
    rect
}

pub(super) fn node(page: &Page, id: usize, identities: &BTreeMap<usize, uuid::Uuid>) -> Value {
    let state = layer(page, id).unwrap();
    let (kind, name, fill) = if let Some(n) = page.boards.iter().find(|n| n.id == id) {
        (
            "frame".to_string(),
            n.name.clone(),
            Some((n.color, n.fill_mode)),
        )
    } else if let Some(n) = page.shapes.iter().find(|n| n.id == id) {
        (
            format!("{:?}", n.kind).to_lowercase(),
            n.name.clone(),
            (n.fill_enabled && n.can_fill()).then_some((n.color, n.fill_mode)),
        )
    } else if let Some(n) = page.texts.iter().find(|n| n.id == id) {
        (
            "text".into(),
            n.content.chars().take(60).collect(),
            Some((n.styles.default.color, n.styles.default.fill_mode)),
        )
    } else {
        (
            "group".into(),
            page.hierarchy.groups[&id].name.clone(),
            None,
        )
    };
    let mut properties = json!({
        "name":page.hierarchy.names.get(&id).unwrap_or(&name),
        "rect":components::bounds(page,id), "opacity":state.opacity,
        "rotation":state.rotation, "hidden":state.hidden,
        "fill":fill.filter(|(_,mode)| matches!(mode,FillMode::Solid)).map(|(color,_)| format!("#{:08X}", u32::from(color))),
    });
    if let Some(n) = page.texts.iter().find(|n| n.id == id) {
        properties["content"] = json!(n.content);
        properties["font_family"] = json!(n.styles.default.family);
        properties["font_size"] = json!(n.styles.default.size);
        properties["font_weight"] = json!(n.styles.default.weight);
    }
    if let Some(n) = page
        .shapes
        .iter()
        .find(|n| n.id == id && n.kind == ShapeKind::Rectangle)
    {
        properties["radius"] = json!(n.radius);
    }
    if let Some(layout) = page.hierarchy.layouts.get(&id) {
        properties["layout"] = json!({"direction":format!("{:?}",layout.axis).to_lowercase(),
            "gap":layout.gap,"padding":layout.padding,
            "main_align":format!("{:?}",layout.main).to_lowercase(),
            "cross_align":format!("{:?}",layout.cross).to_lowercase()});
    }
    let asset = page
        .shapes
        .iter()
        .find(|node| node.id == id)
        .and_then(|node| node.asset.as_ref())
        .map(|asset| {
            let mut info = json!({"name":asset.name(),"hash":asset.hash,"width":asset.width,"height":asset.height});
            if let Some(crate::asset::AssetContent::Svg(svg)) = asset.content() {
                info["svg_editable"] = json!(svg.editable.is_ok());
                info["svg_conversion_error"] = json!(svg.editable.as_ref().err());
            }
            info
        });
    json!({"id":identities[&id],"type":kind,"asset":asset,
        "parent_id":components::parent(page,id).and_then(|id| identities.get(&id)),
        "locked":state.locked,"editable":editable(page,id).is_ok(),"properties":properties,
        "interaction_editable":super::interaction::editable(page,id).is_ok(),
        "action_properties":crate::scene::presentation::property::defaults(page,id),
        "interactions":super::interaction::describe(page,id,identities)})
}

pub(super) fn apply(
    page: &mut Page,
    op: p::Operation,
    refs: &mut BTreeMap<String, String>,
) -> Result<()> {
    match op {
        p::Operation::SetInteraction { .. }
        | p::Operation::SetStartFrame { .. }
        | p::Operation::Import { .. }
        | p::Operation::ReplaceSource { .. } => {
            return Err(Error::invalid(
                "Interaction operation requires document context",
            ));
        }
        p::Operation::Create {
            reference,
            kind,
            parent,
            properties,
        } => {
            validate_reference(&reference, refs)?;
            let parent = parent
                .as_deref()
                .map(|id| lookup(page, id, refs))
                .transpose()?;
            if let Some(parent) = parent {
                editable(page, parent)?;
                if !page.boards.iter().any(|n| n.id == parent)
                    && !page.hierarchy.groups.contains_key(&parent)
                {
                    return Err(Error::invalid("Parent must be a frame or group"));
                }
            }
            if matches!(kind, p::NodeType::Frame) && parent.is_some() {
                return Err(Error::new(
                    "UNSUPPORTED",
                    "New frames must be at the page root",
                ));
            }
            let world: Rect = properties
                .rect
                .ok_or_else(|| Error::invalid("Create requires rect"))?
                .into();
            validate_rect(world)?;
            let board = parent.and_then(|p| {
                if page.boards.iter().any(|n| n.id == p) {
                    Some(p)
                } else {
                    board(page, p)
                }
            });
            let rect = local_rect(page, board, world);
            let id = page.next_id;
            page.next_id = id
                .checked_add(1)
                .ok_or_else(|| Error::invalid("Node limit reached"))?;
            let uid = uuid::Uuid::new_v4();
            match kind {
                p::NodeType::Frame => page.boards.push(Artboard {
                    uid,
                    id,
                    rect,
                    layer: Default::default(),
                    name: "Frame".into(),
                    color: gpui::rgb(0xffffff),
                    color_style: None,
                    fill_mode: FillMode::Solid,
                    gradient: Default::default(),
                    image_fill: Default::default(),
                }),
                p::NodeType::Rectangle | p::NodeType::Ellipse => {
                    let shape_kind = if matches!(kind, p::NodeType::Rectangle) {
                        ShapeKind::Rectangle
                    } else {
                        ShapeKind::Ellipse
                    };
                    let mut shape = Shape::new(id, board, shape_kind, rect);
                    shape.uid = uid;
                    page.shapes.push(shape);
                }
                p::NodeType::Text => page.texts.push(Text {
                    uid,
                    id,
                    board,
                    rect,
                    layer: Default::default(),
                    content: String::new(),
                    styles: StyledText::default(),
                }),
                p::NodeType::Group => {
                    if properties.layout.is_none() {
                        return Err(Error::invalid(
                            "Create group requires layout; its rect defines the layout frame",
                        ));
                    }
                    page.hierarchy.groups.insert(
                        id,
                        LayerGroup {
                            uid,
                            board,
                            name: "Group".into(),
                            layer: Default::default(),
                            mask: None,
                            boolean: None,
                        },
                    );
                }
            }
            if let Some(parent) = parent.filter(|id| page.hierarchy.groups.contains_key(id)) {
                page.hierarchy.parents.insert(id, parent);
            }
            page.hierarchy.order.push(id);
            update(page, id, properties, true)?;
            refs.insert(reference, uid.to_string());
        }
        p::Operation::Update { id, properties } => {
            let id = lookup(page, &id, refs)?;
            editable(page, id)?;
            update(page, id, properties, false)?;
        }
        p::Operation::Delete { id } => {
            let id = lookup(page, &id, refs)?;
            let removed = components::subtree(page, id);
            for id in &removed {
                editable(page, *id)?;
            }
            page.boards.retain(|n| !removed.contains(&n.id));
            page.shapes.retain(|n| !removed.contains(&n.id));
            page.texts.retain(|n| !removed.contains(&n.id));
            page.assets.retain(|n| !removed.contains(&n.object));
            let h = &mut page.hierarchy;
            h.groups.retain(|id, _| !removed.contains(id));
            h.parents
                .retain(|id, parent| !removed.contains(id) && !removed.contains(parent));
            h.order.retain(|id| !removed.contains(id));
            h.names.retain(|id, _| !removed.contains(id));
            h.layouts.retain(|id, _| !removed.contains(id));
            h.sizing.retain(|id, _| !removed.contains(id));
            h.effects.retain(|id, _| !removed.contains(id));
            h.exports.retain(|id, _| !removed.contains(id));
            let remaining = components::ids(page);
            page.hierarchy.retain_interactions(&remaining);
        }
    }
    Ok(())
}

pub(super) fn validate_reference(reference: &str, refs: &BTreeMap<String, String>) -> Result<()> {
    if !reference.starts_with('$')
        || reference.len() < 2
        || reference.len() > 80
        || refs.contains_key(reference)
    {
        return Err(Error::invalid("ref must be a unique $name (2 to 80 bytes)"));
    }
    Ok(())
}

pub(super) fn validate_rect(r: Rect) -> Result<()> {
    if ![r.x, r.y, r.width, r.height]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
        || !(1. ..=100_000.).contains(&r.width)
        || !(1. ..=100_000.).contains(&r.height)
    {
        return Err(Error::invalid(
            "rect requires finite coordinates and sizes in [1, 100000]",
        ));
    }
    Ok(())
}

fn update(page: &mut Page, id: usize, props: p::Properties, creating: bool) -> Result<()> {
    let is_group = page.hierarchy.groups.contains_key(&id);
    let is_text = page.texts.iter().any(|n| n.id == id);
    let is_frame = page.boards.iter().any(|n| n.id == id);
    if !is_text
        && (props.content.is_some()
            || props.font_size.is_some()
            || props.font_family.is_some()
            || props.font_weight.is_some())
    {
        return Err(Error::invalid("Text properties require a text node"));
    }
    if props.radius.is_some()
        && !page
            .shapes
            .iter()
            .any(|n| n.id == id && n.kind == ShapeKind::Rectangle)
    {
        return Err(Error::invalid("radius requires a rectangle"));
    }
    if is_group && props.fill.is_some() {
        return Err(Error::invalid("Groups have no fill"));
    }
    if props.fill.is_some()
        && page
            .shapes
            .iter()
            .any(|shape| shape.id == id && !shape.can_fill())
    {
        return Err(Error::invalid("This shape does not support a solid fill"));
    }
    if props.layout.is_some() && props.remove_layout {
        return Err(Error::invalid("layout conflicts with remove_layout"));
    }
    if (props.layout.is_some() || props.remove_layout) && !(is_group || is_frame) {
        return Err(Error::invalid("Layout requires a frame or group"));
    }
    if let Some(name) = &props.name
        && (name.trim().is_empty() || name.chars().count() > 200)
    {
        return Err(Error::invalid("name must contain 1 to 200 characters"));
    }
    for (value, min, max, name) in [
        (props.opacity, 0., 1., "opacity"),
        (props.rotation, -36000., 36000., "rotation"),
        (props.radius, 0., 100000., "radius"),
        (props.font_size, 1., 1000., "font_size"),
        (props.font_weight, 100., 900., "font_weight"),
    ] {
        if value.is_some_and(|v| !v.is_finite() || !(min..=max).contains(&v)) {
            return Err(Error::invalid(format!("Invalid {name}")));
        }
    }
    let color = props
        .fill
        .as_ref()
        .map(|value| {
            if value.len() != 9 || !value.starts_with('#') {
                return Err(Error::invalid("fill must be #RRGGBBAA"));
            }
            u32::from_str_radix(&value[1..], 16)
                .map(gpui::rgba)
                .map_err(|_| Error::invalid("Invalid fill color"))
        })
        .transpose()?;
    if let Some(rect) = props.rect {
        let rect: Rect = rect.into();
        validate_rect(rect)?;
        if !creating
            && components::parent(page, id).is_some_and(|p| page.hierarchy.layouts.contains_key(&p))
            && !page.hierarchy.sizing.get(&id).is_some_and(|s| s.absolute)
        {
            return Err(Error::new(
                "LAYOUT_CONTROLLED",
                "Node geometry is controlled by parent auto layout",
            ));
        }
        if is_group && !page.hierarchy.layouts.contains_key(&id) && props.layout.is_none() {
            super::geometry::transform_group(page, id, rect)?;
        }
        let rect = local_rect(page, board(page, id), rect);
        if let Some(n) = page.boards.iter_mut().find(|n| n.id == id) {
            n.rect = rect;
        }
        if let Some(n) = page.shapes.iter_mut().find(|n| n.id == id) {
            n.rect = rect;
        }
        if let Some(n) = page.texts.iter_mut().find(|n| n.id == id) {
            n.rect = rect;
        }
        if let Some(n) = page.hierarchy.layouts.get_mut(&id) {
            n.frame = rect;
        }
    }
    let frame = props
        .rect
        .map(Into::into)
        .or_else(|| components::bounds(page, id));
    if let Some(layout) = &props.layout {
        let frame = local_rect(
            page,
            board(page, id),
            frame.ok_or_else(|| Error::invalid("Layout requires rect"))?,
        );
        validate_rect(frame)?;
        let align = |v| match v {
            p::Align::Start => auto_layout::Align::Start,
            p::Align::Center => auto_layout::Align::Center,
            p::Align::End => auto_layout::Align::End,
        };
        let mut container = auto_layout::Container::new(frame);
        container.axis = match layout.direction {
            p::Axis::Horizontal => auto_layout::Axis::Horizontal,
            p::Axis::Vertical => auto_layout::Axis::Vertical,
        };
        container.gap = layout.gap;
        container.padding = layout.padding;
        container.main = align(layout.main_align);
        container.cross = align(layout.cross_align);
        page.hierarchy.layouts.insert(id, container);
    }
    if props.remove_layout {
        page.hierarchy.layouts.remove(&id);
    }
    if let Some(name) = &props.name {
        page.hierarchy.names.insert(id, name.clone());
        if let Some(n) = page.boards.iter_mut().find(|n| n.id == id) {
            n.name = name.clone();
        }
        if let Some(n) = page.shapes.iter_mut().find(|n| n.id == id) {
            n.name = name.clone();
        }
        if let Some(n) = page.hierarchy.groups.get_mut(&id) {
            n.name = name.clone();
        }
    }
    if let Some(n) = page.boards.iter_mut().find(|n| n.id == id) {
        if let Some(color) = color {
            n.color = color;
            n.color_style = None;
            n.fill_mode = FillMode::Solid;
        }
        patch_layer(&mut n.layer, &props);
    }
    if let Some(n) = page.shapes.iter_mut().find(|n| n.id == id) {
        if let Some(color) = color {
            n.color = color;
            n.color_style = None;
            n.fill_mode = FillMode::Solid;
            n.fill_enabled = true;
        }
        if let Some(radius) = props.radius {
            n.radius = radius;
            n.independent_corners = false;
            n.corners = None;
        }
        patch_layer(&mut n.layer, &props);
    }
    if let Some(n) = page.hierarchy.groups.get_mut(&id) {
        patch_layer(&mut n.layer, &props);
    }
    if let Some(n) = page.texts.iter_mut().find(|n| n.id == id) {
        patch_layer(&mut n.layer, &props);
        if let Some(content) = props.content {
            if content.len() > 65536 {
                return Err(Error::invalid("Text exceeds 64 KiB"));
            }
            n.styles.replace(0..n.content.len(), content.len());
            n.content = content;
        }
        let mut changes = Vec::new();
        if let Some(value) = props.font_family {
            changes.push(StyleChange::Family(value.into()));
        }
        if let Some(value) = props.font_size {
            changes.push(StyleChange::Size(value));
        }
        if let Some(value) = props.font_weight {
            changes.push(StyleChange::Weight(value));
        }
        if let Some(value) = color {
            changes.extend([
                StyleChange::Color(value),
                StyleChange::Opacity(value.a),
                StyleChange::FillMode(FillMode::Solid),
            ]);
        }
        for change in changes {
            n.styles.apply(0..n.content.len(), &change, true);
        }
    }
    Ok(())
}

fn patch_layer(layer: &mut LayerState, props: &p::Properties) {
    if let Some(value) = props.opacity {
        layer.opacity = value;
    }
    if let Some(value) = props.rotation {
        layer.rotation = value;
    }
    if let Some(value) = props.hidden {
        layer.hidden = value;
    }
}

/// Layout and container changes must not indirectly mutate protected descendants.
pub(super) fn protect(before: &Page, after: &Page) -> Result<()> {
    let previous = before.node_ids();
    let remaining = after.node_ids();
    for id in previous.keys().copied() {
        let Err(error) = editable(before, id) else {
            continue;
        };
        let unchanged = remaining.contains_key(&id) && {
            let mut old = node(before, id, &previous);
            let mut new = node(after, id, &remaining);
            if super::interaction::editable(before, id).is_ok() {
                old.as_object_mut().unwrap().remove("interactions");
                new.as_object_mut().unwrap().remove("interactions");
            }
            old == new
        };
        if !unchanged {
            return Err(error);
        }
    }
    Ok(())
}
