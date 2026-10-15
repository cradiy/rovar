use super::model;
use crate::{
    agent::protocol::{self as p, Error, Result},
    asset::{AssetContent, FileAsset},
    document::{AssetUse, Page},
    scene::shape::Shape,
};
use std::{collections::BTreeMap, sync::Arc};

pub(super) fn insert(
    page: &mut Page,
    operation: p::Operation,
    asset: Arc<FileAsset>,
    refs: &mut BTreeMap<String, String>,
) -> Result<()> {
    let p::Operation::Import {
        reference,
        parent,
        rect,
        name,
        source,
    } = operation
    else {
        return Err(Error::invalid("Expected import operation"));
    };
    model::validate_reference(&reference, refs)?;
    let rect = rect.into();
    model::validate_rect(rect)?;
    let parent = parent
        .as_deref()
        .map(|id| model::lookup(page, id, refs))
        .transpose()?;
    if let Some(parent) = parent {
        model::editable(page, parent)?;
        if !page.boards.iter().any(|node| node.id == parent)
            && !page.hierarchy.groups.contains_key(&parent)
        {
            return Err(Error::invalid("Parent must be a frame or group"));
        }
    }
    let board = parent.and_then(|id| {
        if page.boards.iter().any(|node| node.id == id) {
            Some(id)
        } else {
            model::board(page, id)
        }
    });
    let group = parent.filter(|id| page.hierarchy.groups.contains_key(id));
    let rect = model::local_rect(page, board, rect);
    let name = name.unwrap_or_else(|| asset.name());
    if name.trim().is_empty() || name.chars().count() > 200 {
        return Err(Error::invalid(
            "Import name must contain 1 to 200 characters",
        ));
    }
    let id = if matches!(source, p::import::Source::Svg { editable: true, .. }) {
        let Some(AssetContent::Svg(svg)) = asset.content() else {
            return Err(Error::invalid("Expected SVG asset"));
        };
        let nodes = svg
            .editable
            .as_ref()
            .map_err(|reason| Error::new("SVG_NOT_EDITABLE", reason.clone()))?;
        let root = crate::editor::svg::add_group(page, &name, 1., board, group);
        crate::editor::svg::insert_nodes(page, nodes, board, root, rect, svg.size);
        root
    } else {
        let id = page.next_id;
        page.next_id = id
            .checked_add(1)
            .ok_or_else(|| Error::invalid("Node limit reached"))?;
        let mut shape = Shape::new(id, board, asset.kind(), rect);
        shape.name = name;
        shape.fill_enabled = false;
        shape.layer.aspect_locked = true;
        shape.asset = Some(asset.clone());
        page.shapes.push(shape);
        page.assets.push(AssetUse {
            object: id,
            fill: false,
            hash: asset.hash.clone(),
        });
        if let Some(parent) = group {
            page.hierarchy.parents.insert(id, parent);
        }
        page.hierarchy.order.push(id);
        id
    };
    refs.insert(reference, page.node_ids()[&id].to_string());
    Ok(())
}
