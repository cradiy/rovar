use super::*;
use crate::{
    agent::protocol::colors::Target,
    document::Page,
    scene::{
        artboard::FillMode,
        color_styles::{ColorStyle, Palette},
        paint_layers::Fill,
        text::StyleChange,
    },
};

fn paint(style: &ColorStyle) -> p::appearance::Paint {
    appearance::describe_paint(
        style.color,
        if style.gradient.is_some() {
            FillMode::Linear
        } else {
            FillMode::Solid
        },
        &style.gradient.clone().unwrap_or_default(),
        &Default::default(),
        None,
    )
}

pub(super) fn describe(palette: &Palette) -> Value {
    json!(
        palette
            .iter()
            .map(|(id, style)| json!({"id":id,"name":style.name,"paint":paint(style)}))
            .collect::<Vec<_>>()
    )
}

pub(super) fn bindings(page: &Page, id: usize) -> Value {
    let fills = if let Some(b) = page.boards.iter().find(|b| b.id == id) {
        std::iter::once((b.fill_enabled, &b.color_style))
            .chain(b.fill_layers.iter().map(|f| (f.enabled, &f.color_style)))
            .filter(|(enabled, _)| *enabled)
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>()
    } else if let Some(s) = page.shapes.iter().find(|s| s.id == id && s.can_fill()) {
        std::iter::once((s.fill_enabled, &s.color_style))
            .chain(s.fill_layers.iter().map(|f| (f.enabled, &f.color_style)))
            .filter(|(enabled, _)| *enabled)
            .map(|(_, id)| id.clone())
            .collect()
    } else {
        Vec::new()
    };
    let strokes = page
        .shapes
        .iter()
        .find(|s| s.id == id)
        .map(|s| {
            std::iter::once(&s.stroke)
                .chain(&s.stroke_layers)
                .filter(|s| s.enabled)
                .map(|s| s.color_style.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"fills":fills,"strokes":strokes})
}

fn lookup(palette: &Palette, id: &str, refs: &BTreeMap<String, String>) -> Result<String> {
    let id = refs.get(id).map_or(id, String::as_str);
    if !palette.contains_key(id) {
        return Err(Error::new(
            "COLOR_STYLE_NOT_FOUND",
            format!("Unknown document color style: {id}"),
        ));
    }
    Ok(id.to_owned())
}

fn value(page: &Page, name: String, paint: p::appearance::Paint) -> Result<ColorStyle> {
    if !matches!(
        paint,
        p::appearance::Paint::Solid { .. } | p::appearance::Paint::Gradient { .. }
    ) {
        return Err(Error::invalid(
            "Color styles support solid and gradient paints",
        ));
    }
    let fill = appearance::paint(page, &paint)?;
    let style = ColorStyle {
        name,
        color: if fill.fill_mode == FillMode::Linear {
            fill.gradient.stops()[0].color
        } else {
            fill.color
        },
        gradient: (fill.fill_mode == FillMode::Linear).then_some(fill.gradient),
    };
    style
        .validate()
        .map_err(|e| Error::invalid(e.to_string()))?;
    Ok(style)
}

fn bind_fill(fill: &mut Fill, style: &Option<(String, ColorStyle)>) {
    fill.color_style = style.as_ref().map(|(id, _)| id.clone());
    if let Some((_, style)) = style {
        style.apply(&mut fill.color, &mut fill.fill_mode, &mut fill.gradient);
    }
}

fn bind(
    page: &mut Page,
    id: usize,
    target: Target,
    style: Option<(String, ColorStyle)>,
) -> Result<()> {
    model::editable(page, id)?;
    match target {
        Target::Fill { index } => {
            if let Some(b) = page.boards.iter_mut().find(|b| b.id == id) {
                let mut base = Fill::from_board(b);
                let fill = std::iter::once(&mut base)
                    .chain(&mut b.fill_layers)
                    .filter(|f| f.enabled)
                    .nth(index)
                    .ok_or_else(|| Error::invalid("Fill index is outside the current stack"))?;
                bind_fill(fill, &style);
                base.apply_board(b);
            } else if let Some(s) = page.shapes.iter_mut().find(|s| s.id == id && s.can_fill()) {
                let mut base = Fill::from_shape(s);
                let fill = std::iter::once(&mut base)
                    .chain(&mut s.fill_layers)
                    .filter(|f| f.enabled)
                    .nth(index)
                    .ok_or_else(|| Error::invalid("Fill index is outside the current stack"))?;
                bind_fill(fill, &style);
                base.apply_shape(s);
            } else {
                return Err(Error::invalid(
                    "Fill binding requires a frame or closed vector shape",
                ));
            }
        }
        Target::Stroke { index } => {
            let shape = page
                .shapes
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or_else(|| Error::invalid("Stroke binding requires a shape"))?;
            let stroke = std::iter::once(&mut shape.stroke)
                .chain(&mut shape.stroke_layers)
                .filter(|s| s.enabled)
                .nth(index)
                .ok_or_else(|| Error::invalid("Stroke index is outside the current stack"))?;
            stroke.color_style = style.as_ref().map(|(id, _)| id.clone());
            if let Some((_, style)) = &style {
                style.apply(
                    &mut stroke.color,
                    &mut stroke.fill_mode,
                    &mut stroke.gradient,
                );
            }
        }
        Target::Text { start, end } => {
            let text = page
                .texts
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| Error::invalid("Text binding requires a text node"))?;
            let whole = start.is_none() && end.is_none();
            let range = match (start, end) {
                (None, None) => 0..text.content.len(),
                (Some(start), Some(end)) => typography::range(&text.content, start, end)?,
                _ => return Err(Error::invalid("Provide both text offsets or omit both")),
            };
            if !whole && range.is_empty() {
                return Err(Error::invalid(
                    "Text binding requires a nonempty range; omit offsets for an empty box",
                ));
            }
            let change = style.map_or(StyleChange::DetachColorStyle, |(id, style)| {
                StyleChange::ColorStyle(Some(id), style)
            });
            text.styles.apply(range, &change, whole);
        }
    }
    Ok(())
}

pub(super) fn apply(
    page: &mut Page,
    palette: &mut Palette,
    op: p::Operation,
    refs: &BTreeMap<String, String>,
    color_refs: &mut BTreeMap<String, String>,
) -> Result<()> {
    match op {
        p::Operation::CreateColorStyle {
            reference,
            name,
            paint,
        } => {
            let value = value(page, name, paint)?;
            let id = uuid::Uuid::new_v4().to_string();
            palette.insert(id.clone(), value);
            color_refs.insert(reference, id);
        }
        p::Operation::UpdateColorStyle {
            id,
            name,
            paint: updated,
        } => {
            let id = lookup(palette, &id, color_refs)?;
            let old = &palette[&id];
            let mut next = old.clone();
            if let Some(updated) = updated
                && updated != paint(old)
            {
                next = value(page, next.name, updated)?;
            }
            if let Some(name) = name {
                next.name = name;
            }
            next.validate().map_err(|e| Error::invalid(e.to_string()))?;
            palette.insert(id, next);
        }
        p::Operation::DeleteColorStyle { id } => {
            let id = lookup(palette, &id, color_refs)?;
            palette.remove(&id);
        }
        p::Operation::BindColorStyle { id, target, style } => {
            let id = model::lookup(page, &id, refs)?;
            let style = style
                .map(|id| {
                    lookup(palette, &id, color_refs).map(|id| (id.clone(), palette[&id].clone()))
                })
                .transpose()?;
            bind(page, id, target, style)?;
        }
        _ => return Err(Error::invalid("Expected a color style operation")),
    }
    crate::scene::color_styles::resolve_with_baselines(page, palette);
    Ok(())
}
