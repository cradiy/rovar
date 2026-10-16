use super::model;
use crate::{
    agent::protocol::{self as p, Error, Result, interaction as dto},
    document::Page,
    scene::{components, presentation as scene},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn editable(page: &Page, id: usize) -> Result<()> {
    let mut current = Some(id);
    while let Some(id) = current {
        if model::layer(page, id).is_some_and(|layer| layer.locked) {
            return Err(Error::new("LOCKED", "Node or ancestor is locked"));
        }
        current = components::parent(page, id);
    }
    Ok(())
}

pub(super) fn variants<'a>(
    page: &Page,
    id: usize,
    sets: &'a components::variants::Sets,
) -> Vec<(&'a String, &'a String)> {
    let mut current = Some(id);
    while let Some(id) = current {
        if let Some(binding) = page.hierarchy.components.get(&id) {
            return sets
                .values()
                .find(|set| set.variants.contains_key(&binding.component))
                .into_iter()
                .flat_map(|set| &set.variants)
                .filter(|(id, _)| *id != &binding.component)
                .collect();
        }
        current = components::parent(page, id);
    }
    Vec::new()
}

pub(super) fn describe(page: &Page, id: usize, ids: &BTreeMap<usize, uuid::Uuid>) -> Value {
    let interaction = page
        .hierarchy
        .interactions
        .get(&id)
        .cloned()
        .unwrap_or_default();
    let action = |action: Option<scene::Action>| {
        let Some(action) = action else {
            return json!({"type":"none"});
        };
        let mut value = serde_json::to_value(&action).expect("Action serialization is infallible");
        if let scene::Action::Navigate { target }
        | scene::Action::OpenOverlay { target, .. }
        | scene::Action::Media { target, .. }
        | scene::Action::SetProperty { target, .. } = action
        {
            value["target"] = json!(ids.get(&target));
        }
        value
    };
    json!({"click":{"action":action(interaction.click),"transition":interaction.click_transition},
        "hover":{"action":action(interaction.hover),"transition":interaction.hover_transition,
        "hover_exit":interaction.hover_exit}})
}

pub(super) fn frame(page: &Page, target: &str, refs: &BTreeMap<String, String>) -> Result<usize> {
    let id = model::lookup(page, target, refs)?;
    if !page.boards.iter().any(|frame| frame.id == id) {
        return Err(Error::invalid("Target must be a frame on the active page"));
    }
    Ok(id)
}

pub(super) fn apply(
    page: &mut Page,
    operation: p::Operation,
    refs: &mut BTreeMap<String, String>,
    sets: &components::variants::Sets,
) -> Result<()> {
    let p::Operation::SetInteraction {
        id,
        trigger,
        action,
        transition,
        hover_exit,
    } = operation
    else {
        if let p::Operation::SetStartFrame { target } = operation {
            page.hierarchy.start = target
                .as_deref()
                .map(|target| frame(page, target, refs))
                .transpose()?;
            return Ok(());
        }
        return model::apply(page, operation, refs);
    };
    let id = model::lookup(page, &id, refs)?;
    editable(page, id)?;
    let variant = |target: &str| -> Result<uuid::Uuid> {
        let uid =
            uuid::Uuid::parse_str(target).map_err(|_| Error::invalid("Invalid variant UUID"))?;
        if !variants(page, id, sets)
            .iter()
            .any(|(id, _)| **id == uid.to_string())
        {
            return Err(Error::invalid(
                "Target must be an available variant of the source component",
            ));
        }
        Ok(uid)
    };
    let action = match action {
        dto::Action::SetProperty {
            target,
            mut property,
        } => {
            let target = model::lookup(page, &target, refs)?;
            if !property.valid() || !property.supports(page, target) {
                return Err(Error::invalid("Unsupported property or invalid value"));
            }
            if let Some(paint) = property.paint_mut()
                && let Some(hash) = &paint.image_hash
            {
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
                    .find(|a| &a.hash == hash && !a.kind().is_playable())
                    .cloned()
                    .ok_or_else(|| {
                        Error::invalid(
                            "Image hash must reference an existing image on the active page",
                        )
                    })?;
                paint.image.asset = Some(asset);
            }
            Some(scene::Action::SetProperty { target, property })
        }
        dto::Action::None => None,
        dto::Action::Back => Some(scene::Action::Back),
        dto::Action::CloseOverlay => Some(scene::Action::CloseOverlay),
        dto::Action::Navigate { target } => Some(scene::Action::Navigate {
            target: frame(page, &target, refs)?,
        }),
        dto::Action::OpenOverlay { target, settings } => Some(scene::Action::OpenOverlay {
            target: frame(page, &target, refs)?,
            settings: scene::OverlaySettings {
                position: match settings.position {
                    dto::OverlayPosition::Center => scene::OverlayPosition::Center,
                    dto::OverlayPosition::Bottom => scene::OverlayPosition::Bottom,
                    dto::OverlayPosition::Right => scene::OverlayPosition::Right,
                },
                backdrop: settings.backdrop,
                dismiss_outside: settings.dismiss_outside,
            },
        }),
        dto::Action::ChangeVariant { target } => Some(scene::Action::ChangeVariant {
            target: variant(&target)?,
        }),
        dto::Action::Media { target, command } => {
            let target = model::lookup(page, &target, refs)?;
            let command = media_command(command);
            if !page
                .shapes
                .iter()
                .any(|shape| shape.id == target && command.supports(shape.kind))
            {
                return Err(Error::invalid(
                    "Media command is unsupported for the target or its parameters are out of range",
                ));
            }
            Some(scene::Action::Media { target, command })
        }
    };
    if hover_exit.is_some() && (!matches!(trigger, dto::Trigger::Hover) || action.is_none()) {
        return Err(Error::invalid(
            "hover_exit requires a nonempty hover action",
        ));
    }
    let exit = hover_exit
        .map(|exit| {
            Ok(match exit {
                dto::HoverExit::Restore => scene::HoverExit::Restore,
                dto::HoverExit::Keep => scene::HoverExit::Keep,
                dto::HoverExit::ChangeVariant { target } => scene::HoverExit::ChangeVariant {
                    target: variant(&target)?,
                },
            })
        })
        .transpose()?;
    let transition = transition.map(|transition| match transition {
        dto::Transition::Instant => scene::Transition::Instant,
        dto::Transition::Dissolve { duration_ms } => scene::Transition::Dissolve { duration_ms },
    });
    if transition.is_some_and(|transition| !transition.valid()) {
        return Err(Error::invalid(
            "Dissolve duration must be 50 to 2000 milliseconds",
        ));
    }
    if action.is_none() && transition.is_some() {
        return Err(Error::invalid("A transition requires a nonempty action"));
    }
    let trigger = match trigger {
        dto::Trigger::Click => scene::Trigger::Click,
        dto::Trigger::Hover => scene::Trigger::Hover,
    };
    let interaction = page.hierarchy.interactions.entry(id).or_default();
    interaction.set(trigger, action.clone());
    if let Some(transition) = transition {
        interaction.set_transition(trigger, transition);
    }
    if let Some(exit) = exit {
        interaction.hover_exit = exit;
    }
    if trigger == scene::Trigger::Hover && action.is_none() {
        interaction.hover_exit = scene::HoverExit::Restore;
    }
    if interaction.is_empty() {
        page.hierarchy.interactions.remove(&id);
    }
    Ok(())
}

fn media_command(command: dto::MediaCommand) -> scene::MediaCommand {
    use dto::MediaCommand as D;
    use scene::MediaCommand as S;
    match command {
        D::Play => S::Play,
        D::Pause => S::Pause,
        D::Toggle => S::Toggle,
        D::Restart => S::Restart,
        D::SeekBy { seconds } => S::SeekBy { seconds },
        D::SeekTo { seconds } => S::SeekTo { seconds },
        D::Rate { percent } => S::Rate { percent },
        D::Volume { percent } => S::Volume { percent },
        D::Mute => S::Mute,
        D::Unmute => S::Unmute,
    }
}
