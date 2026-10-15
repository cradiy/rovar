use super::model;
use crate::{
    agent::protocol::{Error, Result},
    document::Page,
    scene::{artboard::Rect, components},
};

/// Plain groups derive their bounds from members. Transform all members using
/// their original page coordinates, then resolve board-local coordinates.
pub(super) fn transform_group(page: &mut Page, id: usize, target: Rect) -> Result<()> {
    let original = components::bounds(page, id)
        .filter(|rect| rect.width > 0. && rect.height > 0.)
        .ok_or_else(|| Error::invalid("Cannot transform a group with empty bounds"))?;
    let ids = components::subtree(page, id);
    let mut rectangles = Vec::new();
    for child in ids {
        model::editable(page, child)?;
        if page.hierarchy.layouts.contains_key(&child) {
            return Err(Error::new(
                "LAYOUT_CONTROLLED",
                "Group contains auto layout; edit its layout frame separately",
            ));
        }
        if page.hierarchy.groups.contains_key(&child) {
            continue;
        }
        let rect = components::bounds(page, child)
            .ok_or_else(|| Error::invalid("Missing group member bounds"))?;
        let rect = Rect {
            x: target.x + (rect.x - original.x) * target.width / original.width,
            y: target.y + (rect.y - original.y) * target.height / original.height,
            width: rect.width * target.width / original.width,
            height: rect.height * target.height / original.height,
        };
        model::validate_rect(rect)?;
        rectangles.push((child, rect));
    }
    // Update boards first so descendants use their new origins exactly once.
    for (id, rect) in &rectangles {
        if let Some(board) = page.boards.iter_mut().find(|board| board.id == *id) {
            board.rect = *rect;
        }
    }
    for (id, rect) in rectangles {
        let rect = model::local_rect(page, model::board(page, id), rect);
        if let Some(shape) = page.shapes.iter_mut().find(|shape| shape.id == id) {
            shape.rect = rect;
        }
        if let Some(text) = page.texts.iter_mut().find(|text| text.id == id) {
            text.rect = rect;
        }
    }
    Ok(())
}
