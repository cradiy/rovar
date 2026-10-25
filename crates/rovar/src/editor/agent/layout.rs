use super::*;
use crate::{
    agent::protocol::layout as dto,
    document::Page,
    scene::{auto_layout as a, components},
};

fn mode(value: dto::Mode) -> a::Mode {
    match value {
        dto::Mode::Fixed => a::Mode::Fixed,
        dto::Mode::Hug => a::Mode::Hug,
        dto::Mode::Fill => a::Mode::Fill,
    }
}
fn constraint(value: dto::Constraint) -> a::Constraint {
    match value {
        dto::Constraint::Start => a::Constraint::Start,
        dto::Constraint::End => a::Constraint::End,
        dto::Constraint::Center => a::Constraint::Center,
        dto::Constraint::Stretch => a::Constraint::Stretch,
        dto::Constraint::Scale => a::Constraint::Scale,
    }
}
pub(super) fn describe(page: &Page, id: usize, props: &mut Value) {
    let s = page.hierarchy.sizing.get(&id).copied().unwrap_or_default();
    props["sizing"] = json!({"width":format!("{:?}",s.width).to_lowercase(),"height":format!("{:?}",s.height).to_lowercase(),
        "absolute":s.absolute,"min_width":s.limits.min_width,"max_width":s.limits.max_width,
        "min_height":s.limits.min_height,"max_height":s.limits.max_height,"column_span":s.column_span,"row_span":s.row_span});
    props["constraints"]=s.constraints.map_or(Value::Null,|c|json!({
        "horizontal":format!("{:?}",c.horizontal).to_lowercase(),"vertical":format!("{:?}",c.vertical).to_lowercase()}));
}
pub(super) fn sizing(page: &mut Page, id: usize, props: &p::Properties) -> Result<()> {
    if let Some(value) = props.sizing {
        let old = page.hierarchy.sizing.get(&id).copied().unwrap_or_default();
        let value = a::Sizing {
            width: mode(value.width),
            height: mode(value.height),
            absolute: value.absolute,
            column_span: value.column_span,
            row_span: value.row_span,
            constraints: old.constraints,
            limits: a::Limits {
                min_width: value.min_width,
                max_width: value.max_width,
                min_height: value.min_height,
                max_height: value.max_height,
            },
        };
        value
            .limits
            .validate()
            .map_err(|e| Error::invalid(e.to_string()))?;
        if value != old {
            page.hierarchy.sizing.insert(id, value);
        }
    }
    if props.remove_constraints && props.constraints.is_some() {
        return Err(Error::invalid(
            "constraints conflicts with remove_constraints",
        ));
    }
    if props.remove_constraints
        && let Some(s) = page.hierarchy.sizing.get_mut(&id)
    {
        s.constraints = None;
    }
    Ok(())
}

pub(super) fn constraints(page: &mut Page, id: usize, props: &p::Properties) -> Result<()> {
    let old = page.hierarchy.sizing.get(&id).and_then(|s| s.constraints);
    let Some((horizontal, vertical)) = props
        .constraints
        .map(|c| (constraint(c.horizontal), constraint(c.vertical)))
        .or_else(|| props.rect.and(old.map(|c| (c.horizontal, c.vertical))))
    else {
        return Ok(());
    };
    if props.rect.is_none()
        && old.is_some_and(|c| c.horizontal == horizontal && c.vertical == vertical)
    {
        return Ok(());
    }
    let parent = components::parent(page, id)
        .ok_or_else(|| Error::invalid("Constraints require a parent"))?;
    if !(page.boards.iter().any(|b| b.id == parent) || page.hierarchy.layouts.contains_key(&parent))
        || page.hierarchy.layouts.contains_key(&parent)
            && !page.hierarchy.sizing.get(&id).is_some_and(|s| s.absolute)
    {
        return Err(Error::invalid(
            "Constraints require a non-flow child of a frame or layout group",
        ));
    }
    let bounds = components::bounds(page, parent)
        .ok_or_else(|| Error::invalid("Missing parent geometry"))?;
    let mut rect =
        components::bounds(page, id).ok_or_else(|| Error::invalid("Missing child geometry"))?;
    rect.x -= bounds.x;
    rect.y -= bounds.y;
    let s = page.hierarchy.sizing.entry(id).or_default();
    s.constraints = Some(a::Constraints {
        horizontal,
        vertical,
        rect,
        parent_size: [bounds.width, bounds.height],
    });
    if matches!(horizontal, a::Constraint::Stretch | a::Constraint::Scale) {
        s.width = a::Mode::Fixed;
    }
    if matches!(vertical, a::Constraint::Stretch | a::Constraint::Scale) {
        s.height = a::Mode::Fixed;
    }
    Ok(())
}

pub(super) fn validate(page: &Page) -> Result<()> {
    for (&id, s) in &page.hierarchy.sizing {
        let parent = components::parent(page, id);
        let parent_layout = parent.and_then(|p| page.hierarchy.layouts.get(&p));
        let flow = parent_layout.is_some() && !s.absolute;
        if s.constraints.is_some()
            && (flow
                || parent.is_none_or(|p| {
                    !page.boards.iter().any(|b| b.id == p)
                        && !page.hierarchy.layouts.contains_key(&p)
                }))
        {
            return Err(Error::invalid(
                "Constraints require a non-flow child of a frame or layout group; remove_constraints before moving into flow",
            ));
        }
        for (axis, mode) in [s.width, s.height].into_iter().enumerate() {
            let parent_mode =
                parent
                    .and_then(|p| page.hierarchy.sizing.get(&p))
                    .map_or(
                        a::Mode::Fixed,
                        |s| if axis == 0 { s.width } else { s.height },
                    );
            let layout = page.hierarchy.layouts.get(&id);
            let constrained = s.constraints.is_some_and(|c| {
                matches!(
                    if axis == 0 { c.horizontal } else { c.vertical },
                    a::Constraint::Stretch | a::Constraint::Scale
                )
            });
            let valid = match mode {
                a::Mode::Fixed => true,
                a::Mode::Fill => {
                    !constrained
                        && flow
                        && (parent_mode != a::Mode::Hug
                            || axis == 0
                                && parent_layout.is_some_and(|l| {
                                    l.axis == a::Axis::Grid && l.column_width.is_some()
                                }))
                }
                a::Mode::Hug => {
                    !constrained
                        && (layout.is_some() || page.texts.iter().any(|t| t.id == id))
                        && !layout.is_some_and(|l| {
                            if l.axis == a::Axis::Grid {
                                axis == 0 && l.column_width.is_none()
                            } else {
                                l.wrap && (l.axis == a::Axis::Horizontal) == (axis == 0)
                            }
                        })
                }
            };
            if !valid {
                return Err(Error::invalid(format!(
                    "Unsupported sizing mode for node {id} on axis {axis}"
                )));
            }
        }
    }
    Ok(())
}
