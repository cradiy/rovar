//! Bind the shared inspector controls to an element or a prototype action value.
use super::*;
use crate::scene::presentation::{
    Action, Trigger, paint::Paint, property::Property as ActionProperty,
};

impl Workspace {
    pub(in crate::editor) fn sync_action_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = self
            .inspector_action()
            .map(|(id, trigger, property)| (id, trigger, property.clone()));
        let target = self.action_property_target();
        if self.inspector.action_key == key && self.inspector.action_target == target {
            return;
        }
        let identity = |key: &Option<(usize, Trigger, ActionProperty)>| {
            key.as_ref()
                .map(|(id, trigger, p)| (*id, *trigger, p.key()))
        };
        if identity(&self.inspector.action_key) != identity(&key)
            || self.inspector.action_target != target
        {
            for popover in &self.inspector.paint_popovers {
                popover.update(cx, |p, cx| p.close(window, cx));
            }
            self.inspector.active_stop = 0;
        }
        self.inspector.action_key = key;
        self.inspector.action_target = target;
        self.sync_fields(cx);
        if let Some((_, _, ActionProperty::FontFamily(family))) = self.inspector_action() {
            let family = family.clone();
            self.inspector
                .font_picker
                .update(cx, |picker, cx| picker.set_selected(family.into(), cx));
        }
    }

    pub(in crate::editor) fn import_action_image_fill(&mut self, cx: &mut Context<Self>) {
        let Some((id, trigger, property)) = self.inspector_action() else {
            return;
        };
        let property = property.clone();
        let target = self.action_property_target();
        let generation = self.pages.generation;
        let dialog = crate::platform::prompt_for_files(
            cx,
            false,
            &[
                "image/png",
                "image/jpeg",
                "image/webp",
                "image/gif",
                "image/bmp",
                "image/tiff",
                "image/svg+xml",
            ],
        );
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = dialog.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let asset = crate::asset::FileAsset::load_async(path, cx.background_executor())
                .await
                .and_then(|asset| {
                    anyhow::ensure!(!asset.kind().is_playable(), "{}", t("image-required"));
                    Ok(asset)
                });
            let _ = this.update(cx, |this, cx| {
                if this.pages.generation != generation
                    || this.action_property_target() != target
                    || this.inspector_action() != Some((id, trigger, &property))
                {
                    return;
                }
                match asset {
                    Ok(asset) => {
                        this.history.borrow_mut().break_group();
                        this.edit_action_paint(|b| b.image_fill.asset = Some(asset), cx);
                    }
                    Err(error) => {
                        this.media.error = Some(crate::i18n::message(
                            "image-background-error",
                            &[("error", format!("{error:#}"))],
                        ))
                    }
                }
                this.sync_fields(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::editor) fn inspector_action(&self) -> Option<(usize, Trigger, &ActionProperty)> {
        if !self.presentation.tab || self.selection_ids().len() != 1 {
            return None;
        }
        let id = *self.selection_ids().first()?;
        let interaction = self.hierarchy.interactions.get(&id)?;
        let trigger = self.presentation.trigger;
        let action = match trigger {
            Trigger::Click => &interaction.click,
            Trigger::Hover => &interaction.hover,
        };
        match action.as_ref()? {
            Action::SetProperty { property, .. } => Some((id, trigger, property)),
            _ => None,
        }
    }

    pub(in crate::editor) fn action_paint(&self) -> Option<&Paint> {
        match self.inspector_action()?.2 {
            ActionProperty::Fill(p) | ActionProperty::Stroke(p) => Some(p),
            _ => None,
        }
    }

    pub(in crate::editor) fn action_property_target(&self) -> Option<usize> {
        let (id, trigger, _) = self.inspector_action()?;
        match self.hierarchy.interactions[&id].get(trigger)? {
            Action::SetProperty { target, .. } => Some(target),
            _ => None,
        }
    }

    pub(in crate::editor) fn action_paint_text(&self) -> bool {
        let Some((id, trigger, _)) = self.inspector_action() else {
            return false;
        };
        matches!(self.hierarchy.interactions[&id].get(trigger), Some(Action::SetProperty { target, .. }) if self.texts.iter().any(|t| t.id == target))
    }

    pub(in crate::editor) fn action_paint_image_allowed(&self) -> bool {
        self.action_paint().is_some()
            && !self.action_paint_text()
            && !matches!(
                self.inspector_action().map(|a| a.2),
                Some(ActionProperty::Stroke(_))
            )
    }

    pub(in crate::editor) fn edit_action_property(
        &mut self,
        property: ActionProperty,
        group: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((id, trigger, current)) = self.inspector_action() else {
            return false;
        };
        if !property.valid()
            || property.key() != current.key()
            || self.preview_read_only()
            || !self.layer_editable(id)
        {
            return false;
        }
        if current == &property {
            return true;
        }
        let Some(Action::SetProperty { target, .. }) =
            self.hierarchy.interactions[&id].get(trigger)
        else {
            return false;
        };
        if !property.supports(&self.snapshot_page(cx).0, target) {
            return false;
        }
        let before = self.snapshot_hierarchy();
        self.hierarchy
            .interactions
            .get_mut(&id)
            .unwrap()
            .set(trigger, Some(Action::SetProperty { target, property }));
        self.history.borrow_mut().record(
            vec![before],
            group.then_some(Group::PrototypeColor(id, trigger)),
        );
        cx.notify();
        true
    }

    pub(in crate::editor) fn edit_action_paint<R>(
        &mut self,
        edit: impl FnOnce(&mut Artboard) -> R,
        cx: &mut Context<Self>,
    ) -> Option<R> {
        let mut board = self.action_paint()?.board();
        let result = edit(&mut board);
        let paint = Paint::from_board(&board);
        let property = if matches!(self.inspector_action()?.2, ActionProperty::Stroke(_)) {
            ActionProperty::Stroke(paint)
        } else {
            ActionProperty::Fill(paint)
        };
        self.edit_action_property(property, true, cx)
            .then_some(result)
    }

    pub(in crate::editor) fn action_field_value(&self, index: usize) -> Option<String> {
        if index == 18 {
            let p = self.inspector_action()?.2;
            let encoded = serde_json::to_value(p).ok()?;
            let v = encoded.get("value")?.as_f64()? as f32;
            return Some(number(if matches!(p, ActionProperty::Opacity(_)) {
                v * 100.
            } else {
                v
            }));
        }
        let paint = self.action_paint()?;
        let board = paint.board();
        let color = board.editable_color(self.inspector.active_stop);
        Some(match index {
            5 if paint.mode == FillMode::Image => t("shape-image").into(),
            5 => hex(color),
            6 => number(
                if paint.mode == FillMode::Image {
                    paint.image.opacity
                } else {
                    color.a
                } * 100.,
            ),
            7 => number(paint.gradient.angle),
            8 => number(paint.gradient.stop(self.inspector.active_stop)?.position * 100.),
            19..=21 => {
                let FillMode::Points(g) = paint.mode else {
                    return None;
                };
                let p = g.points[self.inspector.active_stop.min(3)];
                number(
                    match index {
                        19 => p.position.x,
                        20 => p.position.y,
                        _ => p.radius,
                    } * 100.,
                )
            }
            _ => return None,
        })
    }

    pub(in crate::editor) fn edit_action_field(
        &mut self,
        index: usize,
        value: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if index == 18 {
            let Some((_, _, p)) = self.inspector_action() else {
                return false;
            };
            let Ok(v) = value.trim().parse::<f32>() else {
                return false;
            };
            let mut encoded = serde_json::to_value(p).unwrap();
            encoded["value"] = serde_json::json!(if matches!(p, ActionProperty::Opacity(_)) {
                v / 100.
            } else {
                v
            });
            return serde_json::from_value(encoded)
                .ok()
                .is_some_and(|p| self.edit_action_property(p, true, cx));
        }
        let Some(property) = self.field_property(index) else {
            return false;
        };
        let stop = self.inspector.active_stop;
        self.edit_action_paint(
            |board| super::properties::apply_field(board, stop, property, value),
            cx,
        )
        .unwrap_or(false)
    }

    pub(in crate::editor) fn action_numeric_property(&self) -> Option<Property> {
        Some(match self.inspector_action()?.2 {
            ActionProperty::Opacity(_) => Property::LayerOpacity,
            ActionProperty::X(_) => Property::X,
            ActionProperty::Y(_) => Property::Y,
            ActionProperty::Width(_) => Property::Width,
            ActionProperty::Height(_) => Property::Height,
            ActionProperty::Rotation(_) => Property::Rotation,
            ActionProperty::StrokeWidth(_) => Property::StrokeWidth,
            ActionProperty::Radius(_) => Property::Radius,
            ActionProperty::FontSize(_) => Property::FontSize,
            ActionProperty::FontWeight(_) => Property::FontWeight,
            ActionProperty::LineHeight(_) => Property::LineHeight,
            ActionProperty::LetterSpacing(_) => Property::LetterSpacing,
            ActionProperty::Gap(_) | ActionProperty::LineGap(_) | ActionProperty::Padding(_) => {
                Property::Radius
            }
            _ => return None,
        })
    }
}
