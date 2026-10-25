use super::*;
use crate::{
    agent::protocol::appearance as dto,
    document::{AssetUse, Page},
    scene::{
        artboard::{FillMode, GradientStop, LinearGradient},
        image_fill::ImageFill,
        paint_layers::Fill,
        shape::{Stroke, StrokeAlign},
    },
};

fn color(value: &str) -> Result<gpui::Rgba> {
    crate::scene::presentation::property::color(value)
        .ok_or_else(|| Error::invalid("Color must be #RRGGBB or #RRGGBBAA"))
}

pub(super) fn describe_paint(
    color: gpui::Rgba,
    mode: FillMode,
    gradient: &LinearGradient,
    image: &ImageFill,
    image_hash: Option<&str>,
) -> dto::Paint {
    use dto::{GradientKind as K, Paint};
    match mode {
        FillMode::Solid => Paint::Solid {
            color: crate::scene::presentation::property::hex(color),
        },
        FillMode::Linear => Paint::Gradient {
            kind: match gradient.kind {
                gpui::GradientKind::Linear => K::Linear,
                gpui::GradientKind::Radial => K::Radial,
                gpui::GradientKind::Angular => K::Angular,
                gpui::GradientKind::Diamond => K::Diamond,
            },
            angle: gradient.angle,
            seam_width: gradient.seam_width,
            stops: gradient
                .stops()
                .iter()
                .map(|stop| dto::Stop {
                    id: Some(stop.id),
                    position: stop.position,
                    midpoint: stop.midpoint,
                    color: crate::scene::presentation::property::hex(stop.color),
                })
                .collect(),
        },
        FillMode::Points(gradient) => Paint::Points { gradient },
        FillMode::Image => Paint::Image {
            hash: image
                .asset
                .as_ref()
                .map(|a| a.hash.as_str())
                .or(image_hash)
                .unwrap_or_default()
                .into(),
            fit: image.fit,
            placement: image.placement,
            opacity: image.opacity,
        },
    }
}

fn fill_dto(fill: &Fill) -> dto::Fill {
    dto::Fill {
        paint: describe_paint(
            fill.color,
            fill.fill_mode,
            &fill.gradient,
            &fill.image_fill,
            fill.image_hash.as_deref(),
        ),
        opacity: fill.opacity,
        hidden: fill.hidden,
    }
}

fn stroke_dto(stroke: &Stroke) -> dto::Stroke {
    dto::Stroke {
        paint: describe_paint(
            stroke.color,
            stroke.fill_mode,
            &stroke.gradient,
            &ImageFill::default(),
            None,
        ),
        opacity: stroke.opacity,
        hidden: stroke.hidden,
        width: stroke.width,
        align: match stroke.align {
            StrokeAlign::Inside => dto::StrokeAlign::Inside,
            StrokeAlign::Center => dto::StrokeAlign::Center,
            StrokeAlign::Outside => dto::StrokeAlign::Outside,
        },
    }
}

fn fills(page: &Page, id: usize) -> Option<Vec<Fill>> {
    if let Some(board) = page.boards.iter().find(|b| b.id == id) {
        Some(
            std::iter::once(Fill::from_board(board))
                .chain(board.fill_layers.clone())
                .filter(|f| f.enabled)
                .collect(),
        )
    } else {
        page.shapes
            .iter()
            .find(|s| s.id == id && s.can_fill())
            .map(|shape| {
                std::iter::once(Fill::from_shape(shape))
                    .chain(shape.fill_layers.clone())
                    .filter(|f| f.enabled)
                    .collect()
            })
    }
}

pub(super) fn describe(page: &Page, id: usize, properties: &mut Value) {
    properties["blend"] = json!(model::layer(page, id).unwrap().blend);
    properties["effects"] = json!(page.hierarchy.effects.get(&id).cloned().unwrap_or_default());
    if let Some(fills) = fills(page, id) {
        properties["fills"] = json!(fills.iter().map(fill_dto).collect::<Vec<_>>());
    }
    if let Some(shape) = page.shapes.iter().find(|s| s.id == id) {
        properties["strokes"] = json!(
            std::iter::once(&shape.stroke)
                .chain(&shape.stroke_layers)
                .filter(|s| s.enabled)
                .map(stroke_dto)
                .collect::<Vec<_>>()
        );
    }
}

pub(super) fn paint(page: &Page, value: &dto::Paint) -> Result<Fill> {
    use dto::{GradientKind as K, Paint};
    let mut fill = Fill {
        enabled: true,
        hidden: false,
        opacity: 1.,
        color_style: None,
        color: gpui::rgb(0xffffff),
        fill_mode: FillMode::Solid,
        gradient: Default::default(),
        image_fill: Default::default(),
        image_hash: None,
    };
    match value {
        Paint::Solid { color: value } => fill.color = color(value)?,
        Paint::Gradient {
            kind,
            angle,
            stops,
            seam_width,
        } => {
            if !(2..=4096).contains(&stops.len())
                || stops.windows(2).any(|s| s[0].position > s[1].position)
            {
                return Err(Error::invalid(
                    "Gradient needs 2 to 4096 stops sorted by position",
                ));
            }
            let mut next_id = stops
                .iter()
                .filter_map(|s| s.id)
                .max()
                .map_or(Some(0), |id| id.checked_add(1))
                .ok_or_else(|| Error::invalid("Gradient stop ID exhausted"))?;
            let stops = stops
                .iter()
                .map(|stop| {
                    let id = match stop.id {
                        Some(id) => id,
                        None => {
                            let id = next_id;
                            next_id = next_id
                                .checked_add(1)
                                .ok_or_else(|| Error::invalid("Gradient stop ID exhausted"))?;
                            id
                        }
                    };
                    Ok(GradientStop {
                        id,
                        position: stop.position,
                        midpoint: stop.midpoint,
                        color: color(&stop.color)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            fill.gradient = LinearGradient::from_stops(
                *angle,
                match kind {
                    K::Linear => gpui::GradientKind::Linear,
                    K::Radial => gpui::GradientKind::Radial,
                    K::Angular => gpui::GradientKind::Angular,
                    K::Diamond => gpui::GradientKind::Diamond,
                },
                *seam_width,
                stops,
            )
            .map_err(|e| Error::invalid(e.to_string()))?;
            fill.fill_mode = FillMode::Linear;
        }
        Paint::Points { gradient } => fill.fill_mode = FillMode::Points(*gradient),
        Paint::Image {
            hash,
            fit,
            placement,
            opacity,
        } => {
            let asset = page
                .boards
                .iter()
                .filter_map(|b| b.image_fill.asset.as_ref())
                .chain(page.shapes.iter().flat_map(|s| {
                    [s.asset.as_ref(), s.image_fill.asset.as_ref()]
                        .into_iter()
                        .flatten()
                }))
                .chain(page.action_images())
                .chain(
                    page.extra_fills()
                        .filter_map(|f| f.image_fill.asset.as_ref()),
                )
                .find(|asset| &asset.hash == hash && !asset.kind().is_playable())
                .cloned()
                .ok_or_else(|| {
                    Error::invalid(
                        "Image hash must reference an imported image or SVG on this page",
                    )
                })?;
            fill.fill_mode = FillMode::Image;
            fill.image_hash = Some(hash.clone());
            fill.image_fill = ImageFill {
                asset: Some(asset),
                fit: *fit,
                placement: *placement,
                opacity: *opacity,
            };
        }
    }
    fill.validate().map_err(|e| Error::invalid(e.to_string()))?;
    Ok(fill)
}

pub(super) fn apply(page: &mut Page, id: usize, props: &p::Properties) -> Result<()> {
    if props.fill.is_some() && props.fills.is_some() {
        return Err(Error::invalid("fill conflicts with fills"));
    }
    if let Some(values) = &props.fills {
        let old = fills(page, id)
            .ok_or_else(|| Error::invalid("fills requires a frame or a closed vector shape"))?;
        let values = values
            .iter()
            .enumerate()
            .map(|(i, value)| {
                // Preserve dormant paint settings, style bindings and stop allocation on an unchanged paint.
                let mut fill = match old.get(i).filter(|old| fill_dto(old).paint == value.paint) {
                    Some(old) => old.clone(),
                    None => paint(page, &value.paint)?,
                };
                fill.opacity = value.opacity;
                fill.hidden = value.hidden;
                fill.validate().map_err(|e| Error::invalid(e.to_string()))?;
                Ok(fill)
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(board) = page.boards.iter_mut().find(|b| b.id == id) {
            board.fill_layers = values.iter().skip(1).cloned().collect();
            if let Some(first) = values.first() {
                first.apply_board(board);
            } else {
                board.fill_enabled = false;
            }
        } else if let Some(shape) = page.shapes.iter_mut().find(|s| s.id == id) {
            shape.fill_layers = values.iter().skip(1).cloned().collect();
            if let Some(first) = values.first() {
                first.apply_shape(shape);
            } else {
                shape.fill_enabled = false;
            }
        }
        // Primary image fills use AssetUse; additional layers carry their own hashes.
        let asset = page
            .boards
            .iter()
            .find(|b| b.id == id)
            .and_then(|b| b.image_fill.asset.as_ref())
            .or_else(|| {
                page.shapes
                    .iter()
                    .find(|s| s.id == id)
                    .and_then(|s| s.image_fill.asset.as_ref())
            });
        let hash = asset.map(|asset| asset.hash.clone());
        if let Some(hash) = hash {
            if let Some(slot) = page
                .assets
                .iter_mut()
                .find(|asset| asset.object == id && asset.fill)
            {
                slot.hash = hash;
            } else {
                page.assets.push(AssetUse {
                    object: id,
                    fill: true,
                    hash,
                });
            }
        } else {
            page.assets
                .retain(|asset| asset.object != id || !asset.fill);
        }
    }
    if let Some(values) = &props.strokes {
        let shape = page
            .shapes
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| Error::invalid("strokes requires a shape"))?;
        let old: Vec<_> = std::iter::once(&shape.stroke)
            .chain(&shape.stroke_layers)
            .filter(|s| s.enabled)
            .cloned()
            .collect();
        let values = values
            .iter()
            .enumerate()
            .map(|(i, value)| {
                if !value.width.is_finite()
                    || !(0. ..=100_000.).contains(&value.width)
                    || !(0. ..=1.).contains(&value.opacity)
                {
                    return Err(Error::invalid("Invalid stroke width or opacity"));
                }
                let mut stroke = match old
                    .get(i)
                    .filter(|old| stroke_dto(old).paint == value.paint)
                {
                    Some(old) => old.clone(),
                    None => {
                        let fill = paint(page, &value.paint)?;
                        if !matches!(fill.fill_mode, FillMode::Solid | FillMode::Linear) {
                            return Err(Error::invalid("Stroke paint must be solid or gradient"));
                        }
                        Stroke {
                            enabled: true,
                            color: fill.color,
                            fill_mode: fill.fill_mode,
                            gradient: fill.gradient,
                            ..Default::default()
                        }
                    }
                };
                stroke.opacity = value.opacity;
                stroke.hidden = value.hidden;
                stroke.width = value.width;
                stroke.align = match value.align {
                    dto::StrokeAlign::Inside => StrokeAlign::Inside,
                    dto::StrokeAlign::Center => StrokeAlign::Center,
                    dto::StrokeAlign::Outside => StrokeAlign::Outside,
                };
                Ok(stroke)
            })
            .collect::<Result<Vec<_>>>()?;
        let shape = page.shapes.iter_mut().find(|s| s.id == id).unwrap();
        shape.stroke_layers = values.iter().skip(1).cloned().collect();
        if let Some(first) = values.first() {
            shape.stroke = first.clone();
        } else {
            shape.stroke.enabled = false;
        }
    }
    if let Some(effects) = &props.effects {
        if effects.is_empty() {
            page.hierarchy.effects.remove(&id);
        } else {
            page.hierarchy.effects.insert(id, effects.clone());
        }
    }
    Ok(())
}
