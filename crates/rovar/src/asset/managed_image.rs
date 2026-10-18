use futures_channel::oneshot;
use gpui::{App, RenderImage};
use std::sync::{Arc, Mutex};

/// Owns both decoded pixels and the atlas tiles uploaded in this application.
pub(crate) struct ManagedImage {
    image: Arc<RenderImage>,
    release: Mutex<Option<oneshot::Sender<()>>>,
}

impl ManagedImage {
    pub fn new(image: Arc<RenderImage>) -> Arc<Self> {
        Arc::new(Self {
            image,
            release: Mutex::new(None),
        })
    }

    pub fn render(&self, cx: &App) -> Arc<RenderImage> {
        let mut release = self.release.lock().unwrap();
        if release.is_none() {
            let (sender, receiver) = oneshot::channel();
            *release = Some(sender);
            let image = self.image.clone();
            cx.spawn(async move |cx| {
                // The last owner drops the sender, even on a background thread.
                let _ = receiver.await;
                cx.update(|cx| cx.drop_image(image, None));
            })
            .detach();
        }
        self.image.clone()
    }

    pub fn element(self: &Arc<Self>) -> gpui::Img {
        let image = self.clone();
        gpui::img(move |_: &mut gpui::Window, cx: &mut App| Some(Ok(image.render(cx))))
    }
}

impl std::ops::Deref for ManagedImage {
    type Target = RenderImage;

    fn deref(&self) -> &Self::Target {
        &self.image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn shared_images_release_after_the_last_owner_drops_in_a_background_task(
        cx: &mut gpui::TestAppContext,
    ) {
        let pixels = Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::new(2, 2),
        )]));
        let weak = Arc::downgrade(&pixels);
        let owner = ManagedImage::new(pixels);
        let other_document = owner.clone();
        cx.update(|cx| {
            owner.render(cx);
            other_document.render(cx);
        });
        drop(owner);
        cx.run_until_parked();
        assert!(weak.upgrade().is_some());
        cx.background_executor
            .spawn(async move { drop(other_document) })
            .detach();
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
    }
}
