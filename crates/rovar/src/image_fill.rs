use crate::media::{MediaAsset, MediaContent};
use gpui::{Bounds, Div, Pixels, Window, div, point, prelude::*, px, size};
use std::sync::Arc;

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
}
impl Default for ImageFill {
    fn default() -> Self {
        Self {
            asset: None,
            fit: ImageFit::Cover,
            opacity: 1.,
        }
    }
}
impl ImageFill {
    pub fn element(&self) -> Div {
        div()
            .overflow_hidden()
            .opacity(self.opacity)
            .when_some(self.asset.as_ref(), |el, asset| {
                if let Some(MediaContent::Image(image)) = asset.content() {
                    el.child(
                        gpui::img(image.clone())
                            .size_full()
                            .object_fit(match self.fit {
                                ImageFit::Cover => gpui::ObjectFit::Cover,
                                ImageFit::Contain => gpui::ObjectFit::Contain,
                            }),
                    )
                } else {
                    el
                }
            })
    }

    pub fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let Some(asset) = &self.asset else {
            return;
        };
        let Some(MediaContent::Image(image)) = asset.content() else {
            return;
        };
        let sx = f32::from(bounds.size.width) / asset.width as f32;
        let sy = f32::from(bounds.size.height) / asset.height as f32;
        let scale = match self.fit {
            ImageFit::Cover => sx.max(sy),
            ImageFit::Contain => sx.min(sy),
        };
        let image_size = size(
            px(asset.width as f32 * scale),
            px(asset.height as f32 * scale),
        );
        let origin = bounds.center() - point(image_size.width / 2., image_size.height / 2.);
        let _ = window.paint_image_with_clip(
            Bounds::new(origin, image_size),
            bounds,
            Default::default(),
            image.clone(),
            0,
            false,
        );
    }
}
