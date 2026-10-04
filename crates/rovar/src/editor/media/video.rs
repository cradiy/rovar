use super::*;
use crate::ui::theme::Color;
use gpui_media::{
    MediaBackendEvent, MediaOutput, MediaPlaybackSession, PlaybackTimeline, SeekMode, VideoSurface,
};

pub(super) struct PlaybackChanged;
pub(super) struct VideoPreview {
    playback: Box<dyn MediaPlaybackSession>,
    surface: VideoSurface,
    timeline: PlaybackTimeline,
    playing: bool,
    ended: bool,
    muted: bool,
    error: Option<String>,
    _tasks: Vec<gpui::Task<()>>,
}
impl gpui::EventEmitter<PlaybackChanged> for VideoPreview {}
impl VideoPreview {
    pub fn new(
        playback: Box<dyn MediaPlaybackSession>,
        output: MediaOutput,
        cx: &mut Context<Self>,
    ) -> Self {
        let frames = cx.spawn(async move |this, cx| {
            while let Ok(frame) = output.video_frames.recv().await {
                if this
                    .update(cx, |this, cx| {
                        if let Err(error) = this.surface.set_frame(&frame) {
                            this.error = Some(error.to_string());
                            cx.emit(PlaybackChanged);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let events = cx.spawn(async move |this, cx| {
            while let Ok(event) = output.events.recv().await {
                if this
                    .update(cx, |this, cx| {
                        match event {
                            MediaBackendEvent::Ended => {
                                this.playing = false;
                                this.ended = true;
                            }
                            MediaBackendEvent::Error(error) => {
                                this.error = Some(error.to_string());
                                this.playing = false;
                            }
                            _ => {}
                        }
                        this.timeline = this.playback.timeline();
                        cx.emit(PlaybackChanged);
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let timeline = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if this.playing {
                            this.timeline = this.playback.timeline();
                            cx.emit(PlaybackChanged);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            timeline: playback.timeline(),
            playback,
            surface: VideoSurface::new(),
            playing: false,
            ended: false,
            muted: false,
            error: None,
            _tasks: vec![frames, events, timeline],
        }
    }
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if self.playing {
            if let Err(e) = self.playback.pause() {
                self.error = Some(e.to_string());
            }
            self.playing = false;
            self.timeline = self.playback.timeline();
            cx.emit(PlaybackChanged);
        }
    }
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.playing {
            self.pause(cx);
            return;
        }
        let result = if self.ended {
            self.playback.restart()
        } else {
            self.playback.play()
        };
        match result {
            Ok(()) => {
                self.playing = true;
                self.ended = false;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        cx.emit(PlaybackChanged);
    }
    fn seek(&mut self, ratio: f64, cx: &mut Context<Self>) {
        let Some(duration) = self
            .timeline
            .duration()
            .filter(|_| self.timeline.is_seekable())
        else {
            return;
        };
        match self
            .playback
            .seek_to(duration.mul_f64(ratio.clamp(0., 1.)), SeekMode::Accurate)
        {
            Ok(()) => {
                self.ended = false;
                self.timeline = PlaybackTimeline::new(
                    duration.mul_f64(ratio.clamp(0., 1.)),
                    Some(duration),
                    true,
                );
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        cx.emit(PlaybackChanged);
    }
}
impl Render for VideoPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .size_full()
            .when_some(self.surface.surface().cloned(), |el, frame| {
                el.child(gpui::surface(frame).size_full())
            })
    }
}

pub(super) struct VideoControls {
    view: Entity<VideoPreview>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    _subscription: Subscription,
}
impl VideoControls {
    pub fn new(view: Entity<VideoPreview>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&view, |_, _, _: &PlaybackChanged, cx| cx.notify());
        Self {
            view,
            bounds: Rc::new(Cell::new(Bounds::default())),
            _subscription: subscription,
        }
    }
}
impl Render for VideoControls {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.view.read(cx);
        let timeline = state.timeline;
        let playing = state.playing;
        let muted = state.muted;
        let error = state.error.clone();
        let bounds = self.bounds.clone();
        let stamp = |t: Duration| format!("{}:{:02}", t.as_secs() / 60, t.as_secs() % 60);
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("video-toggle")
                            .debug_selector(|| "video-toggle".into())
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(Color::Selected.color())
                            .cursor_pointer()
                            .child(icon(
                                if playing {
                                    LucideIcons::Pause
                                } else {
                                    LucideIcons::Play
                                },
                                17.,
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.view.update(cx, |v, cx| v.toggle(cx))
                            })),
                    )
                    .child(div().flex_1().child(format!(
                            "{} / {}",
                            stamp(timeline.position()),
                            timeline
                                .duration()
                                .map(stamp)
                                .unwrap_or_else(|| "--:--".into())
                        )))
                    .child(
                        div()
                            .id("video-mute")
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child(icon(
                                if muted {
                                    LucideIcons::VolumeX
                                } else {
                                    LucideIcons::Volume2
                                },
                                17.,
                            ))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.view.update(cx, |v, cx| {
                                    v.muted = !v.muted;
                                    v.playback.set_muted(v.muted);
                                    cx.emit(PlaybackChanged);
                                })
                            })),
                    ),
            )
            .child(
                div()
                    .id("video-progress")
                    .debug_selector(|| "video-progress".into())
                    .h(px(16.))
                    .relative()
                    .cursor_pointer()
                    .child(
                        gpui::canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(5.))
                            .h(px(6.))
                            .w_full()
                            .rounded(px(3.))
                            .bg(BORDER.color()),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(5.))
                            .h(px(6.))
                            .w(gpui::relative(timeline.progress().unwrap_or(0.) as f32))
                            .rounded(px(3.))
                            .bg(ACCENT.color()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            let b = this.bounds.get();
                            let ratio = f32::from(event.position.x - b.origin.x)
                                / f32::from(b.size.width).max(1.);
                            this.view.update(cx, |v, cx| v.seek(ratio as f64, cx));
                            cx.stop_propagation();
                        }),
                    ),
            )
            .when_some(error, |el, error| {
                el.child(div().text_color(Color::Danger.color()).child(error))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use gpui_media::{MediaCapabilities, MediaResult};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Calls {
        play: usize,
        pause: usize,
        seeks: Vec<(Duration, SeekMode)>,
    }
    struct Session(Arc<Mutex<Calls>>);
    impl MediaPlaybackSession for Session {
        fn capabilities(&self) -> MediaCapabilities {
            MediaCapabilities::default()
        }
        fn timeline(&self) -> PlaybackTimeline {
            PlaybackTimeline::new(Duration::ZERO, Some(Duration::from_secs(20)), true)
        }
        fn play(&mut self) -> MediaResult<()> {
            self.0.lock().unwrap().play += 1;
            Ok(())
        }
        fn pause(&mut self) -> MediaResult<()> {
            self.0.lock().unwrap().pause += 1;
            Ok(())
        }
        fn seek_to(&mut self, t: Duration, mode: SeekMode) -> MediaResult<()> {
            self.0.lock().unwrap().seeks.push((t, mode));
            Ok(())
        }
    }
    #[gpui::test]
    fn controls_seek_accurately_pause_and_restart_after_end(cx: &mut TestAppContext) {
        cx.update(uic::init);
        let calls = Arc::new(Mutex::new(Calls::default()));
        let (sink, output) = MediaOutputSink::channel();
        let view = cx.new(|cx| VideoPreview::new(Box::new(Session(calls.clone())), output, cx));
        let window = cx.open_window(size(px(320.), px(160.)), |_, cx| {
            VideoControls::new(view.clone(), cx)
        });
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        crate::editor::tests::draw(&mut visual);
        let button = visual.debug_bounds("video-toggle").unwrap().center();
        visual.simulate_click(button, Default::default());
        crate::editor::tests::draw(&mut visual);
        assert_eq!(calls.lock().unwrap().play, 1);
        let progress = visual.debug_bounds("video-progress").unwrap();
        visual.simulate_click(
            progress.origin + point(progress.size.width * 0.75, px(8.)),
            Default::default(),
        );
        crate::editor::tests::draw(&mut visual);
        assert_eq!(
            calls.lock().unwrap().seeks[0],
            (Duration::from_secs(15), SeekMode::Accurate)
        );
        visual.simulate_click(button, Default::default());
        crate::editor::tests::draw(&mut visual);
        assert_eq!(calls.lock().unwrap().pause, 1);
        sink.emit(MediaBackendEvent::Ended);
        crate::editor::tests::draw(&mut visual);
        visual.simulate_click(button, Default::default());
        crate::editor::tests::draw(&mut visual);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.play, 2);
        assert_eq!(calls.seeks[1], (Duration::ZERO, SeekMode::KeyFrame));
    }
}
