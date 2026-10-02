use super::*;
use crate::i18n::t;
use crate::media::{MediaAsset, MediaContent};
use gpui::AnimationExt as _;
use gpui_media::{MediaBackend, MediaOutputSink, MediaPlaybackRequest};
use std::{sync::Arc, time::Duration};
mod state;
#[cfg(test)]
mod tests;
mod video;
pub(super) use state::State;
use video::{VideoControls, VideoPreview};

#[derive(Clone)]
pub(super) struct VideoRuntime {
    view: Entity<VideoPreview>,
    controls: Entity<VideoControls>,
}

impl Workspace {
    pub(super) fn load_visible_media(&mut self, window: &Window, cx: &mut Context<Self>) {
        let bounds = self.bounds.get();
        let size = if f32::from(bounds.size.width) > 0. {
            bounds.size
        } else {
            window.viewport_size()
        };
        let (left, right) = self.canvas_insets();
        let origin = self.view.world(point(left, 0.));
        let viewport = Rect {
            x: origin.x,
            y: origin.y,
            width: (f32::from(size.width) - left - right).max(1.) / self.view.zoom,
            height: f32::from(size.height) / self.view.zoom,
        };
        let visible = |id| {
            self.layer_info(id)
                .is_some_and(|(own, parent)| !self.effective_layer(own, parent).hidden)
                && self.world_rect(id).is_some_and(|rect| {
                    crate::scene::rotation::intersects(rect, self.object_rotation(id), viewport)
                })
        };
        let candidates: Vec<_> = self
            .boards
            .iter()
            .filter(|b| visible(b.id))
            .filter_map(|b| b.image_fill.asset.clone())
            .chain(self.shapes.iter().filter(|s| visible(s.id)).flat_map(|s| {
                [s.media.clone(), s.image_fill.asset.clone()]
                    .into_iter()
                    .flatten()
            }))
            .filter(|asset| asset.is_pending())
            .collect();
        for asset in candidates {
            if self.media.decoding.len() >= 2 {
                break;
            }
            if !self.media.decoding.insert(asset.hash.clone()) {
                continue;
            }
            let hash = asset.hash.clone();
            let page = self.pages.active.clone();
            cx.spawn(async move |this, cx| {
                let result = asset.ensure_decoded_async(cx.background_executor()).await;
                let _ = this.update(cx, |this, cx| {
                    this.media.decoding.remove(&hash);
                    if let Err(error) = result
                        && this.pages.active == page
                    {
                        this.media.error = Some(crate::i18n::message(
                            "media-import-error",
                            &[("error", format!("{error:#}"))],
                        ));
                    }
                    this.scene.update(cx, |_, cx| cx.notify());
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub(crate) fn pause_videos(&mut self, cx: &mut Context<Self>) {
        self.media.playback_generation += 1;
        for runtime in self.media.videos.values() {
            runtime.view.update(cx, |video, cx| video.pause(cx));
        }
    }
    pub(super) fn import_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_bezier(cx);
        self.exit_vector_edit(cx);
        self.draw_tool = None;
        self.toolbar.hand = false;
        self.seal_text_edits(cx);
        let request = self.media.import_request;
        let page = self.pages.active.clone();
        let view = self.view;
        let size = self.bounds.get().size.map(f32::from);
        let insets = self.canvas_insets();
        let dialog = crate::platform::prompt_for_paths(
            cx,
            gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(t("media-choose").into()),
            },
        );
        cx.spawn_in(window, async move |this, cx| {
            let path = match dialog.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                error => {
                    let _ = this.update(cx, |this, cx| {
                        if this.media.import_request == request {
                            this.media.error = Some(crate::i18n::message(
                                "file-dialog-error",
                                &[("error", format!("{error:?}"))],
                            ));
                            cx.notify();
                        }
                    });
                    None
                }
            };
            let Some(path) = path else {
                return;
            };
            let Ok(current) = this.update(cx, |this, cx| {
                if this.media.import_request != request {
                    return false;
                }
                this.media.importing = true;
                this.media.error = None;
                cx.notify();
                true
            }) else {
                return;
            };
            if !current {
                return;
            }
            let loaded = MediaAsset::load_async(path, cx.background_executor()).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.media.import_request != request {
                    return;
                }
                this.media.importing = false;
                match loaded {
                    Ok(asset) => {
                        if this.pages.active == page {
                            this.add_media(asset, window, cx);
                        } else if let Some(state) =
                            this.pages.entries.iter_mut().find(|p| p.page.id == page)
                        {
                            let (left, right) = insets;
                            let scale = (0.8 * (size.width - left - right).max(1.)
                                / view.zoom
                                / asset.width as f32)
                                .min(
                                    0.8 * (size.height - 160.).max(1.)
                                        / view.zoom
                                        / asset.height as f32,
                                )
                                .min(320. / view.zoom / asset.width.max(asset.height) as f32)
                                .min(1.);
                            let width = asset.width as f32 * scale;
                            let height = asset.height as f32 * scale;
                            let center = view
                                .world(point((left + size.width - right) / 2., size.height / 2.));
                            let id = state.page.next_id;
                            state.page.next_id += 1;
                            let mut shape = Shape::new(
                                id,
                                None,
                                asset.kind(),
                                Rect {
                                    x: center.x - width / 2.,
                                    y: center.y - height / 2.,
                                    width,
                                    height,
                                },
                            );
                            shape.name = asset.name();
                            shape.fill_enabled = false;
                            shape.layer.aspect_locked = true;
                            state.page.assets.push(crate::document::AssetUse {
                                object: id,
                                fill: false,
                                hash: asset.hash.clone(),
                            });
                            shape.media = Some(asset);
                            let index = state.page.shapes.len();
                            state.page.shapes.push(shape);
                            this.history.borrow_mut().record_for(
                                page.clone(),
                                vec![Change::Shape {
                                    id,
                                    index,
                                    value: None,
                                }],
                            );
                        }
                    }
                    Err(error) => {
                        this.media.error = Some(crate::i18n::message(
                            "media-import-error",
                            &[("error", format!("{error:#}"))],
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn add_media(&mut self, asset: Arc<MediaAsset>, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_gesture(window, cx);
        self.draw_tool = None;
        let size = self.bounds.get().size.map(f32::from);
        let (left, right) = self.canvas_insets();
        // Start at a compact component size, regardless of the window size or zoom.
        // A smaller viewport can reduce it further; small sources are never enlarged.
        let scale =
            (0.8 * (size.width - left - right).max(1.) / self.view.zoom / asset.width as f32)
                .min(0.8 * (size.height - 160.).max(1.) / self.view.zoom / asset.height as f32)
                .min(320. / self.view.zoom / asset.width.max(asset.height) as f32)
                .min(1.);
        let width = asset.width as f32 * scale;
        let height = asset.height as f32 * scale;
        let center = self
            .view
            .world(point((left + size.width - right) / 2., size.height / 2.));
        let rect = Rect {
            x: center.x - width / 2.,
            y: center.y - height / 2.,
            width,
            height,
        };
        let (board, rect) = self.parent_for_rect(None, rect);
        let id = self.next_id;
        self.next_id += 1;
        let mut shape = Shape::new(id, board, asset.kind(), rect);
        shape.name = asset.name();
        shape.fill_enabled = false;
        shape.layer.aspect_locked = true;
        shape.media = Some(asset);
        self.shapes.push(shape);
        self.history.borrow_mut().record(
            vec![Change::Shape {
                id,
                index: self.shapes.len() - 1,
                value: None,
            }],
            None,
        );
        self.select_shape(id, cx);
        self.focus.focus(window, cx);
    }

    pub(super) fn media_surface(&self, shape: &Shape, outset: f32) -> Div {
        let mut content = div().absolute().inset(px(outset)).overflow_hidden();
        let [tl, tr, br, bl] = shape.displayed_radii().map(|r| px(r * self.view.zoom));
        if let Some(asset) = &shape.media {
            content = match asset.content() {
                Some(MediaContent::Image(_)) => content.child(
                    self.cropped_media(shape)
                        .element_with_radii(gpui::Corners {
                            top_left: tl,
                            top_right: tr,
                            bottom_right: br,
                            bottom_left: bl,
                        })
                        .size_full(),
                ),
                Some(MediaContent::Video(frame)) => content
                    .bg(rgb(0x101216))
                    .child(gpui::surface(frame.clone()).absolute().size_full()),
                None => content.bg(gpui::rgba(0xb4a2ee12)),
            };
            if let Some(runtime) = self.media.videos.get(&shape.id) {
                content = content.child(runtime.view.clone());
            }
        }
        content
    }

    pub(super) fn media_properties(&self, cx: &mut Context<Self>) -> Div {
        let shape = self.selected_shape().unwrap();
        let id = shape.id;
        let Some(asset) = &shape.media else {
            return div();
        };
        let asset = asset.clone();
        let video = shape.kind == ShapeKind::Video;
        let loading = self.media.video_loading.contains(&id);
        let name = asset.name();
        inspector::inspector_section(t(if video {
            "video-source"
        } else {
            "image-source"
        }))
        .child(
            div()
                .debug_selector(move || {
                    if video {
                        "video-source-row"
                    } else {
                        "image-source-row"
                    }
                    .into()
                })
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .size(px(40.))
                        .flex_shrink_0()
                        .rounded(px(6.))
                        .overflow_hidden()
                        .bg(gpui::checkerboard(rgb(0x30343d), 4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(match asset.content() {
                            Some(MediaContent::Image(image)) => gpui::img(image.clone())
                                .size_full()
                                .object_fit(gpui::ObjectFit::Contain)
                                .into_any_element(),
                            Some(MediaContent::Video(frame)) => gpui::surface(frame.clone())
                                .size_full()
                                .object_fit(gpui::ObjectFit::Contain)
                                .into_any_element(),
                            _ => icon(
                                if video {
                                    LucideIcons::Film
                                } else {
                                    LucideIcons::Image
                                },
                                18.,
                            )
                            .text_color(rgb(MUTED))
                            .into_any_element(),
                        }),
                )
                .child(
                    div()
                        .id("media-source-info")
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(div().truncate().text_size(px(12.)).child(name.clone()))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(format!("{} × {} px", asset.width, asset.height)),
                        )
                        .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(name.clone())).into()),
                )
                .when(!video, |el| el.child(self.image_crop_button(id, cx)))
                .when(video && !self.media.videos.contains_key(&id), |el| {
                    el.child(
                        div()
                            .id("video-start")
                            .debug_selector(|| "video-start".into())
                            .size(px(28.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                icon(
                                    if loading {
                                        LucideIcons::Loader
                                    } else {
                                        LucideIcons::Play
                                    },
                                    16.,
                                )
                                .text_color(rgb(if loading {
                                    ACCENT
                                } else {
                                    MUTED
                                })),
                            )
                            .tooltip(move |_, cx| {
                                cx.new(|_| {
                                    toolbar::ToolTip(
                                        t(if loading { "video-loading" } else { "play" }).into(),
                                    )
                                })
                                .into()
                            })
                            .when(!loading, |el| {
                                el.cursor_pointer().hover(|el| el.bg(rgb(BORDER))).on_click(
                                    cx.listener(move |this, _, window, cx| {
                                        this.play_video(id, window, cx)
                                    }),
                                )
                            }),
                    )
                }),
        )
        .when(video, |el| {
            el.when_some(self.media.videos.get(&id), |el, runtime| {
                el.child(runtime.controls.clone())
            })
        })
    }

    pub(super) fn media_status(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let failed = self.media.error.is_some();
        let indicator = if failed {
            icon(LucideIcons::CircleAlert, 15.)
                .text_color(rgb(0xe4aa91))
                .into_any_element()
        } else {
            icon(LucideIcons::Loader, 15.)
                .text_color(rgb(ACCENT))
                .with_animation(
                    ("media-loading-spinner", self.media.import_request as usize),
                    gpui::Animation::new(Duration::from_millis(900)).repeat(),
                    |icon, phase| {
                        icon.with_transformation(gpui::Transformation::rotate(gpui::radians(
                            phase * std::f32::consts::TAU,
                        )))
                    },
                )
                .into_any_element()
        };
        let status = layers::glass_surface()
            .id("media-status")
            .occlude()
            .max_w(px(360.))
            .px(px(12.))
            .py(px(8.))
            .rounded(px(8.))
            .border_1()
            .border_color(gpui::rgba(0xb4a2ee28))
            .shadow_md()
            .text_size(px(12.))
            .text_color(rgb(TEXT))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .size(px(16.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(indicator),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            self.media
                                .error
                                .clone()
                                .unwrap_or_else(|| t("media-loading").into()),
                        ),
                    )
                    .child(
                        div()
                            .id("dismiss-media-status")
                            .size(px(24.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(gpui::rgba(0xffffff0c)))
                            .child(icon(LucideIcons::X, 13.).text_color(rgb(MUTED)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.media.error = None;
                                if this.media.importing {
                                    this.media.cancel_import();
                                }
                                cx.notify();
                            })),
                    ),
            );
        let (left, right) = self.canvas_insets();
        div()
            .absolute()
            .top(px(64.))
            .left(px(left))
            .right(px(right))
            .flex()
            .justify_center()
            .child(status)
    }

    pub(super) fn play_video(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.layer_editable(id) {
            return;
        }
        for (&other, runtime) in &self.media.videos {
            if other != id {
                runtime.view.update(cx, |view, cx| view.pause(cx));
            }
        }
        if let Some(runtime) = self.media.videos.get(&id) {
            runtime.view.update(cx, |view, cx| view.toggle(cx));
            return;
        }
        if !self.media.video_loading.insert(id) {
            return;
        }
        let Some(asset) = self
            .shapes
            .iter()
            .find(|s| s.id == id && s.kind == ShapeKind::Video)
            .and_then(|s| s.media.clone())
        else {
            self.media.video_loading.remove(&id);
            return;
        };
        let expected = asset.clone();
        let playback_generation = self.media.playback_generation;
        let page = self.pages.active.clone();
        let page_generation = self.pages.generation;
        cx.spawn_in(window, async move |this, cx| {
            let opened = cx
                .background_executor()
                .spawn(async move {
                    let (sink, output) = MediaOutputSink::channel();
                    let playback = gpui_media_backend::SystemBackend.open_playback(
                        MediaPlaybackRequest {
                            source: asset.source.media_source()?,
                            output_capabilities: None,
                        },
                        sink,
                    )?;
                    // Portable frames also work inside the editor's rotated offscreen surfaces.
                    #[cfg(not(target_family = "wasm"))]
                    let playback = {
                        let mut playback = playback;
                        playback.set_frame_transport_preference(
                            gpui_media::FrameTransportPreference::CpuOnly,
                        )?;
                        playback
                    };
                    Ok::<_, anyhow::Error>((playback, output))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.pages.active != page || this.pages.generation != page_generation {
                    return;
                }
                this.media.video_loading.remove(&id);
                if !this.layer_editable(id)
                    || !this.shapes.iter().any(|s| {
                        s.id == id && s.media.as_ref().is_some_and(|a| Arc::ptr_eq(a, &expected))
                    })
                {
                    return;
                }
                match opened {
                    Ok((playback, output)) => {
                        for runtime in this.media.videos.values() {
                            runtime.view.update(cx, |v, cx| v.pause(cx));
                        }
                        let view = cx.new(|cx| VideoPreview::new(playback, output, cx));
                        let controls = cx.new(|cx| VideoControls::new(view.clone(), cx));
                        if this.media.playback_generation == playback_generation {
                            view.update(cx, |v, cx| v.toggle(cx));
                        }
                        this.media
                            .videos
                            .insert(id, VideoRuntime { view, controls });
                    }
                    Err(error) => {
                        this.media.error = Some(crate::i18n::message(
                            "media-play-error",
                            &[("error", error.to_string())],
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn sync_video_visibility(&mut self, cx: &mut Context<Self>) {
        self.media.videos.retain(|id, _| {
            self.shapes
                .iter()
                .any(|s| s.id == *id && s.kind == ShapeKind::Video)
        });
        for (id, runtime) in &self.media.videos {
            if self
                .layer_info(*id)
                .is_none_or(|(own, parent)| self.effective_layer(own, parent).hidden)
            {
                runtime.view.update(cx, |view, cx| view.pause(cx));
            }
        }
    }
}
