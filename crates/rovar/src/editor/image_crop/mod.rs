use super::*;
use crate::scene::image_fill::{ImageFill, ImageFit, Placement};
#[cfg(test)]
mod tests;
mod view;

pub(super) struct State {
    id: usize,
    media: bool,
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
        if let Some(shape) = self.shapes.iter().find(|s| s.id == id) {
            if shape.kind == ShapeKind::Image {
                return Some((
                    ImageFill {
                        asset: shape.media.clone(),
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
        if self.preview_read_only() || !self.layer_editable(id) || self.gesture.is_some() {
            return false;
        }
        let Some((image, media)) = self.crop_source(id) else {
            return false;
        };
        if image
            .asset
            .as_ref()
            .is_none_or(|a| a.video || a.width == 0 || a.height == 0)
        {
            return false;
        }
        let Some(frame) = self.world_rect(id) else {
            return false;
        };
        self.finish_bezier(cx);
        self.exit_vector_edit(cx);
        if self.shapes.iter().any(|s| s.id == id) {
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
        self.image_crop
            .as_ref()
            .filter(|c| c.id == id && !c.media)
            .map_or_else(|| fill.clone(), |c| c.image.clone())
    }

    pub(super) fn cropped_media(&self, shape: &Shape) -> ImageFill {
        self.image_crop
            .as_ref()
            .filter(|c| c.id == shape.id && c.media)
            .map_or_else(
                || ImageFill {
                    asset: shape.media.clone(),
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
