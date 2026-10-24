use super::*;
use crate::scene::presentation::MediaCommand;
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
    rate: f64,
    volume: f64,
    ready: bool,
    pending: Vec<MediaCommand>,
    error: Option<String>,
    _tasks: Vec<gpui::Task<()>>,
}
impl gpui::EventEmitter<PlaybackChanged> for VideoPreview {}
impl VideoPreview {
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
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
                            MediaBackendEvent::Ready => {
                                this.ready = true;
                                for command in std::mem::take(&mut this.pending) {
                                    this.apply(command, cx);
                                }
                            }
                            MediaBackendEvent::Ended => {
                                this.playing = false;
                                this.ended = true;
                            }
                            MediaBackendEvent::Error(error) => {
                                this.pending.clear();
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
            ready: playback.timeline().duration().is_some(),
            pending: Vec::new(),
            timeline: playback.timeline(),
            playback,
            surface: VideoSurface::new(),
            playing: false,
            ended: false,
            muted: false,
            rate: 1.,
            volume: 1.,
            error: None,
            _tasks: vec![frames, events, timeline],
        }
    }
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.pending.clear();
        if self.playing {
            if let Err(e) = self.playback.pause() {
                self.error = Some(e.to_string());
                cx.emit(PlaybackChanged);
                return;
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

    pub fn apply(&mut self, command: MediaCommand, cx: &mut Context<Self>) {
        if !command.valid() {
            return;
        }
        if !self.ready
            && (!self.pending.is_empty()
                || matches!(
                    command,
                    MediaCommand::SeekBy { .. }
                        | MediaCommand::SeekTo { .. }
                        | MediaCommand::Restart
                ))
        {
            self.pending.push(command);
            return;
        }
        let result = match command {
            MediaCommand::Play => {
                if !self.playing {
                    self.toggle(cx);
                }
                return;
            }
            MediaCommand::Pause => {
                self.pause(cx);
                return;
            }
            MediaCommand::Toggle => {
                self.toggle(cx);
                return;
            }
            MediaCommand::Restart => self.playback.restart().map(|()| {
                self.playing = true;
                self.ended = false;
            }),
            MediaCommand::SeekBy { seconds } => {
                let position = self.timeline.position().as_secs_f64();
                self.seek_position(
                    Duration::from_secs_f64((position + f64::from(seconds)).max(0.)),
                    cx,
                );
                return;
            }
            MediaCommand::SeekTo { seconds } => {
                self.seek_position(Duration::from_secs(u64::from(seconds)), cx);
                return;
            }
            MediaCommand::Rate { percent } => {
                let rate = f64::from(percent) / 100.;
                self.playback
                    .set_playback_rate(rate)
                    .map(|()| self.rate = rate)
            }
            MediaCommand::Volume { percent } => {
                self.volume = f64::from(percent) / 100.;
                self.playback.set_volume(self.volume);
                Ok(())
            }
            MediaCommand::Mute | MediaCommand::Unmute => {
                self.muted = command == MediaCommand::Mute;
                self.playback.set_muted(self.muted);
                Ok(())
            }
        };
        self.error = result.err().map(|e| e.to_string());
        self.timeline = self.playback.timeline();
        cx.emit(PlaybackChanged);
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            ended: self.ended,
            pending: self.pending.clone(),
            playing: self.playing,
            position: self.playback.timeline().position(),
            rate: self.rate,
            volume: self.volume,
            muted: self.muted,
        }
    }

    pub fn restore(&mut self, state: Snapshot, cx: &mut Context<Self>) {
        self.pause(cx);
        if self.ready {
            self.seek_position(state.position, cx);
        } else {
            self.apply(
                MediaCommand::SeekTo {
                    seconds: state.position.as_secs().min(u64::from(u32::MAX)) as u32,
                },
                cx,
            );
        }
        if self.rate != state.rate {
            self.apply(
                MediaCommand::Rate {
                    percent: (state.rate * 100.).round() as u16,
                },
                cx,
            );
        }
        self.apply(
            MediaCommand::Volume {
                percent: (state.volume * 100.).round() as u8,
            },
            cx,
        );
        self.apply(
            if state.muted {
                MediaCommand::Mute
            } else {
                MediaCommand::Unmute
            },
            cx,
        );
        if state.playing {
            self.apply(MediaCommand::Play, cx);
        }
        self.ended = state.ended;
        for command in state.pending {
            self.apply(command, cx);
        }
    }

    fn seek_position(&mut self, position: Duration, cx: &mut Context<Self>) {
        let timeline = self.playback.timeline();
        let position = timeline
            .duration()
            .map_or(position, |duration| position.min(duration));
        match self.playback.seek_to(position, SeekMode::Accurate) {
            Ok(()) => {
                self.ended = false;
                self.error = None;
                self.timeline =
                    PlaybackTimeline::new(position, timeline.duration(), timeline.is_seekable());
            }
            Err(error) => self.error = Some(error.to_string()),
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

#[derive(Clone)]
pub(in crate::editor) struct Snapshot {
    ended: bool,
    pending: Vec<MediaCommand>,
    playing: bool,
    position: Duration,
    rate: f64,
    volume: f64,
    muted: bool,
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
                                    GpuiIcons::LucidePause
                                } else {
                                    GpuiIcons::LucidePlay
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
                                    GpuiIcons::LucideVolumeX
                                } else {
                                    GpuiIcons::LucideVolume2
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
        rate: f64,
        volume: f64,
        muted: bool,
    }
    struct Session(Arc<Mutex<Calls>>);
    impl MediaPlaybackSession for Session {
        fn capabilities(&self) -> MediaCapabilities {
            MediaCapabilities::default()
        }
        fn timeline(&self) -> PlaybackTimeline {
            let position = self
                .0
                .lock()
                .unwrap()
                .seeks
                .last()
                .map_or(Duration::ZERO, |(position, _)| *position);
            PlaybackTimeline::new(position, Some(Duration::from_secs(20)), true)
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
        fn set_playback_rate(&mut self, rate: f64) -> MediaResult<()> {
            self.0.lock().unwrap().rate = rate;
            Ok(())
        }
        fn set_volume(&mut self, volume: f64) {
            self.0.lock().unwrap().volume = volume;
        }
        fn set_muted(&mut self, muted: bool) {
            self.0.lock().unwrap().muted = muted;
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
        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.play, 2);
        assert_eq!(recorded.seeks[1], (Duration::ZERO, SeekMode::KeyFrame));
        drop(recorded);
        view.update(&mut visual.cx, |view, cx| {
            // Prototype commands share this session and restore the pre-hover state.
            view.apply(MediaCommand::Play, cx);
            view.apply(MediaCommand::SeekTo { seconds: 12 }, cx);
            let before = view.snapshot();
            view.apply(MediaCommand::SeekBy { seconds: 30 }, cx);
            assert_eq!(view.timeline.position(), Duration::from_secs(20));
            view.apply(MediaCommand::SeekBy { seconds: -30 }, cx);
            assert_eq!(view.timeline.position(), Duration::ZERO);
            view.apply(MediaCommand::Rate { percent: 200 }, cx);
            view.apply(MediaCommand::Volume { percent: 35 }, cx);
            view.apply(MediaCommand::Mute, cx);
            view.apply(MediaCommand::Pause, cx);
            view.restore(before, cx);
            assert!(view.playing);
            assert_eq!(view.timeline.position(), Duration::from_secs(12));
            assert_eq!((view.rate, view.volume, view.muted), (1., 1., false));
        });
        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.play, 3);
        assert_eq!(
            (recorded.rate, recorded.volume, recorded.muted),
            (1., 1., false)
        );
        drop(recorded);
        view.update(&mut visual.cx, |view, cx| {
            view.pause(cx);
            view.ready = false;
            view.apply(MediaCommand::SeekTo { seconds: 4 }, cx);
            view.apply(MediaCommand::Play, cx);
            // Leaving the frame cancels a play queued behind initial readiness.
            view.pause(cx);
        });
        sink.emit(MediaBackendEvent::Ready);
        crate::editor::tests::draw(&mut visual);
        assert_eq!(calls.lock().unwrap().play, 3);
        view.update(&mut visual.cx, |view, cx| {
            view.ready = false;
            view.apply(MediaCommand::SeekTo { seconds: 4 }, cx);
            view.apply(MediaCommand::SeekBy { seconds: 3 }, cx);
            view.apply(MediaCommand::Play, cx);
        });
        sink.emit(MediaBackendEvent::Ready);
        crate::editor::tests::draw(&mut visual);
        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.play, 4);
        assert_eq!(
            recorded.seeks.last(),
            Some(&(Duration::from_secs(7), SeekMode::Accurate))
        );
    }
}
