use crate::media::{MediaAsset, MediaContent};
use gpui::{Bounds, Div, Pixels, Window, div, point, prelude::*, px, size};
use std::sync::Arc;
mod placement;
pub use placement::Placement;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageFill {
    #[serde(skip)]
    pub asset: Option<Arc<MediaAsset>>,
    pub fit: ImageFit,
    pub opacity: f32,
    #[serde(default)]
    pub placement: Placement,
}
impl Default for ImageFill {
    fn default() -> Self {
        Self {
            asset: None,
            fit: ImageFit::Cover,
            opacity: 1.,
            placement: Placement::default(),
        }
    }
}
impl ImageFill {
    pub fn element(&self) -> Div {
        self.element_with_radii(Default::default())
    }

    pub fn element_with_radii(&self, radii: gpui::Corners<Pixels>) -> Div {
        let fill = self.clone();
        div().overflow_hidden().opacity(self.opacity).child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, window, _| fill.paint_with_radii(bounds, radii, window),
            )
            .size_full(),
        )
    }

    pub fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        self.paint_with_radii(bounds, Default::default(), window);
    }

    fn paint_with_radii(
        &self,
        bounds: Bounds<Pixels>,
        radii: gpui::Corners<Pixels>,
        window: &mut Window,
    ) {
        let Some(asset) = &self.asset else {
            return;
        };
        let Some(MediaContent::Image(image)) = asset.content() else {
            return;
        };
        let rect = self.placement.rect(
            crate::scene::artboard::Rect {
                x: f32::from(bounds.origin.x),
                y: f32::from(bounds.origin.y),
                width: f32::from(bounds.size.width),
                height: f32::from(bounds.size.height),
            },
            [asset.width, asset.height],
            self.fit,
        );
        let _ = window.paint_image_with_clip(
            Bounds::new(
                point(px(rect.x), px(rect.y)),
                size(px(rect.width), px(rect.height)),
            ),
            bounds,
            radii,
            image.clone(),
            0,
            false,
        );
    }
}
