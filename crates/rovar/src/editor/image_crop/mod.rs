use super::*;
use crate::scene::image_fill::{ImageFill, ImageFit, Placement};
#[cfg(test)]
mod tests;
mod view;

pub(super) struct State {
    action: Option<(
        usize,
        usize,
        crate::scene::presentation::Trigger,
        crate::scene::presentation::property::Property,
    )>,
    id: usize,
    media: bool,
    layer: usize,
    frame: Rect,
    rotation: f32,
    pub(super) image: ImageFill,
}

impl State {
    fn source(&self) -> [u32; 2] {
        let asset = self.image.asset.as_ref().unwrap();
        [asset.width, asset.height]
    }
}

impl Workspace {
    fn crop_source(&self, id: usize) -> Option<(ImageFill, bool)> {
        if self.action_property_target() == Some(id) {
            return self
                .action_paint()
                .filter(|p| p.mode == FillMode::Image)
                .map(|p| (p.image.clone(), false));
        }
        if self.selected_shape.or(self.selected) == Some(id) && self.paint_layer_index(false) > 0 {
            return self.current_image_fill().map(|f| (f.clone(), false));
        }
        if let Some(shape) = self.shapes.iter().find(|s| s.id == id) {
            if shape.kind == ShapeKind::Image {
                return Some((
                    ImageFill {
                        asset: shape.asset.clone(),
                        fit: ImageFit::Contain,
                        placement: shape.media_placement,
                        ..Default::default()
                    },
                    true,
                ));
            }
            return (shape.fill_enabled && shape.can_fill() && shape.fill_mode == FillMode::Image)
                .then(|| (shape.image_fill.clone(), false));
        }
        self.boards
            .iter()
            .find(|b| b.id == id && b.fill_mode == FillMode::Image)
            .map(|b| (b.image_fill.clone(), false))
    }

    pub(super) fn start_image_crop(
        &mut self,
        id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let action = self
            .inspector_action()
            .filter(|_| self.action_paint().is_some())
            .map(|(source, trigger, property)| {
                (self.pages.generation, source, trigger, property.clone())
            });
        if self.preview_read_only()
            || !self.layer_editable(action.as_ref().map_or(id, |a| a.1))
            || self.gesture.is_some()
        {
            return false;
        }
        let Some((image, media)) = self.crop_source(id) else {
            return false;
        };
        if image
            .asset
            .as_ref()
            .is_none_or(|a| a.kind().is_playable() || a.width == 0 || a.height == 0)
        {
            return false;
        }
        let Some(frame) = self.world_rect(id) else {
            return false;
        };
        self.finish_bezier(cx);
        self.exit_vector_edit(cx);
        if action.is_some() {
            self.seal_text_edits(cx);
        } else if self.shapes.iter().any(|s| s.id == id) {
            self.select_shape(id, cx);
        } else {
            self.select(Some(id), cx);
        }
        self.draw_tool = None;
        self.toolbar.hand = false;
        self.corner_editor.clear_hover();
        self.measure_target = None;
        for popover in &self.inspector.paint_popovers {
            popover.update(cx, |p, cx| p.close(window, cx));
        }
        self.image_crop = Some(State {
            layer: self.paint_layer_index(false),
            action,
            id,
            media,
            frame,
            rotation: self.object_rotation(id),
            image,
        });
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn cropped_fill(&self, id: usize, fill: &ImageFill) -> ImageFill {
        self.cropped_fill_layer(id, 0, fill)
    }
    pub(super) fn cropped_fill_layer(
        &self,
        id: usize,
        layer: usize,
        fill: &ImageFill,
    ) -> ImageFill {
        self.image_crop
            .as_ref()
            .filter(|c| c.action.is_none() && c.id == id && !c.media && c.layer == layer)
            .map_or_else(|| fill.clone(), |c| c.image.clone())
    }

    pub(super) fn cropped_media(&self, shape: &Shape) -> ImageFill {
        self.image_crop
            .as_ref()
            .filter(|c| c.action.is_none() && c.id == shape.id && c.media)
            .map_or_else(
                || ImageFill {
                    asset: shape.asset.clone(),
                    fit: ImageFit::Contain,
                    placement: shape.media_placement,
                    ..Default::default()
                },
                |c| c.image.clone(),
            )
    }

    fn crop_pointer(&self, position: Point<Pixels>) -> Option<Point<f32>> {
        let crop = self.image_crop.as_ref()?;
        let world = self
            .view
            .world((position - self.bounds.get().origin).map(f32::from));
        Some(crate::scene::rotation::around(
            world,
            crate::scene::rotation::center(crop.frame),
            -crop.rotation,
        ))
    }

    fn begin_image_crop_drag(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = window.raw_mouse_position();
        let Some(p) = self.crop_pointer(position) else {
            return;
        };
        let crop = self.image_crop.as_ref().unwrap();
        if p.x >= crop.frame.x
            && p.x <= crop.frame.x + crop.frame.width
            && p.y >= crop.frame.y
            && p.y <= crop.frame.y + crop.frame.height
        {
            self.begin(
                GestureKind::ImageCrop {
                    original: crop.image.placement,
                },
                position,
                event.button,
                window,
                cx,
            );
        }
        cx.stop_propagation();
    }

    pub(super) fn move_image_crop(&mut self, original: Placement, delta: Point<f32>) {
        if let Some(crop) = &mut self.image_crop {
            let delta = crate::scene::rotation::vector(delta, -crop.rotation);
            let source = crop.source();
            crop.image.placement = original;
            crop.image
                .placement
                .pan(delta, crop.frame, source, crop.image.fit);
        }
    }

    pub(super) fn zoom_image_crop(
        &mut self,
        factor: f32,
        position: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some() {
            return;
        }
        let anchor = position.and_then(|p| self.crop_pointer(p));
        if let Some(crop) = &mut self.image_crop {
            let source = crop.source();
            let anchor = anchor.unwrap_or_else(|| crate::scene::rotation::center(crop.frame));
            crop.image.placement.zoom_at(
                crop.image.placement.zoom * factor,
                anchor,
                crop.frame,
                source,
                crop.image.fit,
            );
            cx.notify();
        }
    }

    pub(super) fn finish_image_crop(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.gesture,
            Some(Gesture {
                kind: GestureKind::ImageCrop { .. },
                ..
            })
        ) {
            if commit {
                self.finish_gesture(window, cx);
            } else {
                self.cancel_gesture(window, cx);
            }
        }
        let Some(crop) = self.image_crop.take() else {
            return;
        };
        if let Some((generation, id, trigger, property)) = crop.action {
            if commit
                && self.pages.generation == generation
                && self.action_property_target() == Some(crop.id)
                && self.inspector_action() == Some((id, trigger, &property))
            {
                self.history.borrow_mut().break_group();
                self.edit_action_paint(|b| b.image_fill.placement = crop.image.placement, cx);
                self.sync_fields(cx);
            }
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if !crop.media && crop.layer > 0 {
            if commit
                && self.selected_shape.or(self.selected) == Some(crop.id)
                && self.paint_layer_index(false) == crop.layer
                && self.layer_editable(crop.id)
                && self
                    .current_image_fill()
                    .is_some_and(|f| f.asset == crop.image.asset)
            {
                self.history.borrow_mut().break_group();
                if self.selected_shape.is_some() {
                    self.edit_shape_paint(|s| s.image_fill.placement = crop.image.placement);
                } else {
                    self.edit_board_paint(None, |b| b.image_fill.placement = crop.image.placement);
                }
            }
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        let unchanged_source = self
            .crop_source(crop.id)
            .is_some_and(|(fill, media)| media == crop.media && fill.asset == crop.image.asset);
        if commit && unchanged_source && self.layer_editable(crop.id) {
            let change = if let Some(index) = self.shapes.iter().position(|s| s.id == crop.id) {
                let before = self.shapes[index].clone();
                let placement = if crop.media {
                    &mut self.shapes[index].media_placement
                } else {
                    &mut self.shapes[index].image_fill.placement
                };
                let changed = *placement != crop.image.placement;
                *placement = crop.image.placement;
                changed.then_some(Change::Shape {
                    id: crop.id,
                    index,
                    value: Some(Box::new(before)),
                })
            } else if let Some(index) = self.boards.iter().position(|b| b.id == crop.id) {
                let before = self.boards[index].clone();
                self.boards[index].image_fill.placement = crop.image.placement;
                (before != self.boards[index]).then_some(Change::Board {
                    id: crop.id,
                    index,
                    value: Some(before),
                })
            } else {
                None
            };
            if let Some(change) = change {
                self.history.borrow_mut().record(vec![change], None);
            }
        }
        self.focus.focus(window, cx);
        cx.notify();
    }
}
