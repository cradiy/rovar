use super::*;
use gpui_media::{AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, PlaybackState, SeekMode};

#[derive(Clone)]
pub(in crate::editor) struct Snapshot {
    playing: bool,
    position: Duration,
    volume: f64,
    muted: bool,
    pending: Vec<MediaCommand>,
}

pub(in crate::editor) struct Playback {
    player: Entity<AudioPlayer>,
    playing: bool,
    pending: Vec<MediaCommand>,
    _subscription: Subscription,
}

impl Playback {
    pub fn new(player: Entity<AudioPlayer>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&player, |this, _, event: &AudioPlayerEvent, cx| {
            match event {
                AudioPlayerEvent::InfoChanged(_) => {
                    for command in std::mem::take(&mut this.pending) {
                        this.apply(command, cx);
                    }
                }
                AudioPlayerEvent::StateChanged(PlaybackState::Ended | PlaybackState::Error(_)) => {
                    this.playing = false;
                    this.pending.clear();
                }
                _ => {}
            }
            if let AudioPlayerEvent::StateChanged(PlaybackState::Error(error)) = event {
                cx.emit(Failed(error.to_string()));
            }
            cx.notify();
        });
        Self {
            player,
            playing: false,
            pending: Vec::new(),
            _subscription: subscription,
        }
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if !self.playing && self.pending.is_empty() {
            return;
        }
        self.pending.clear();
        self.playing = false;
        self.player.update(cx, |player, cx| player.pause(cx));
        cx.notify();
    }

    pub fn apply(&mut self, command: MediaCommand, cx: &mut Context<Self>) {
        if !command.supports(ShapeKind::Audio) {
            return;
        }
        if self.player.read(cx).info().is_none()
            && (!self.pending.is_empty()
                || matches!(
                    command,
                    MediaCommand::Restart
                        | MediaCommand::SeekBy { .. }
                        | MediaCommand::SeekTo { .. }
                ))
        {
            self.pending.push(command);
            return;
        }
        let playing = self.playing;
        let result = self.player.update(cx, |player, cx| {
            match command {
                MediaCommand::Play => {
                    if !playing {
                        player.play(cx)?;
                    }
                    self.playing = true;
                }
                MediaCommand::Pause => {
                    player.pause(cx);
                    self.playing = false;
                }
                MediaCommand::Toggle => {
                    if playing {
                        player.pause(cx);
                    } else {
                        player.play(cx)?;
                    }
                    self.playing = !playing;
                }
                MediaCommand::Restart => {
                    player.seek_to(Duration::ZERO, SeekMode::Accurate, cx)?;
                    player.play(cx)?;
                    self.playing = true;
                }
                MediaCommand::SeekBy { seconds } => {
                    let position = (player.position().as_secs_f64() + f64::from(seconds)).max(0.);
                    player.seek_to(Duration::from_secs_f64(position), SeekMode::Accurate, cx)?;
                }
                MediaCommand::SeekTo { seconds } => {
                    player.seek_to(
                        Duration::from_secs(u64::from(seconds)),
                        SeekMode::Accurate,
                        cx,
                    )?;
                }
                MediaCommand::Volume { percent } => {
                    player.set_volume(f64::from(percent) / 100., cx)
                }
                MediaCommand::Mute => player.set_muted(true, cx),
                MediaCommand::Unmute => player.set_muted(false, cx),
                MediaCommand::Rate { .. } => {}
            }
            Ok::<_, gpui_media::MediaError>(())
        });
        if let Err(error) = result {
            self.pause(cx);
            cx.emit(Failed(error.to_string()));
        }
        cx.notify();
    }

    pub fn snapshot(&self, cx: &gpui::App) -> Snapshot {
        let player = self.player.read(cx);
        Snapshot {
            playing: self.playing,
            position: player.position(),
            volume: player.volume(),
            muted: player.is_muted(),
            pending: self.pending.clone(),
        }
    }

    pub fn restore(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.pause(cx);
        if self.player.read(cx).info().is_some() {
            if let Err(error) = self.player.update(cx, |player, cx| {
                player.seek_to(snapshot.position, SeekMode::Accurate, cx)
            }) {
                cx.emit(Failed(error.to_string()));
            }
        } else {
            self.apply(
                MediaCommand::SeekTo {
                    seconds: snapshot.position.as_secs().min(u64::from(u32::MAX)) as u32,
                },
                cx,
            );
        }
        self.apply(
            MediaCommand::Volume {
                percent: (snapshot.volume * 100.).round() as u8,
            },
            cx,
        );
        self.apply(
            if snapshot.muted {
                MediaCommand::Mute
            } else {
                MediaCommand::Unmute
            },
            cx,
        );
        if snapshot.playing {
            self.apply(MediaCommand::Play, cx);
        }
        for command in snapshot.pending {
            self.apply(command, cx);
        }
    }
}

pub(super) struct Failed(pub String);
impl gpui::EventEmitter<Failed> for Playback {}

impl Render for Playback {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let player = self.player.read(cx);
        let muted = player.is_muted();
        let position = player.position();
        let duration = player.duration();
        let stamp = |t: Duration| format!("{}:{:02}", t.as_secs() / 60, t.as_secs() % 60);
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .children(
                [
                    (
                        "audio-toggle",
                        if self.playing {
                            LucideIcons::Pause
                        } else {
                            LucideIcons::Play
                        },
                        MediaCommand::Toggle,
                    ),
                    (
                        "audio-restart",
                        LucideIcons::RotateCcw,
                        MediaCommand::Restart,
                    ),
                ]
                .map(|(id, glyph, command)| {
                    div()
                        .id(id)
                        .size(px(30.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .bg(Color::Input.color())
                        .cursor_pointer()
                        .child(icon(glyph, 16.))
                        .on_click(cx.listener(move |this, _, _, cx| this.apply(command, cx)))
                }),
            )
            .child(div().flex_1().child(format!(
                "{} / {}",
                stamp(position),
                duration.map(stamp).unwrap_or_else(|| "--:--".into())
            )))
            .child(
                div()
                    .id("audio-mute")
                    .size(px(30.))
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
                        16.,
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply(
                            if muted {
                                MediaCommand::Unmute
                            } else {
                                MediaCommand::Mute
                            },
                            cx,
                        )
                    })),
            )
    }
}

impl Workspace {
    pub(in crate::editor) fn control_audio(
        &mut self,
        id: usize,
        command: MediaCommand,
        cx: &mut Context<Self>,
    ) {
        if !command.supports(ShapeKind::Audio) {
            return;
        }
        if !self.media.audios.contains_key(&id) {
            let Some(asset) = self
                .shapes
                .iter()
                .find(|s| s.id == id && s.kind == ShapeKind::Audio)
                .and_then(|s| s.asset.clone())
            else {
                return;
            };
            let source = match asset.source.media_source() {
                Ok(source) => source,
                Err(error) => {
                    self.media.error = Some(error.to_string());
                    cx.notify();
                    return;
                }
            };
            let player = cx.new(|cx| {
                AudioPlayer::new(
                    source,
                    AudioPlayerOptions {
                        autoplay: false,
                        ..Default::default()
                    },
                    cx,
                )
                .expect("Could not create audio worker")
            });
            let playback = cx.new(|cx| Playback::new(player, cx));
            let subscription = cx.subscribe(&playback, move |this, runtime, error: &Failed, cx| {
                if this.media.audios.get(&id) != Some(&runtime) {
                    return;
                }
                this.media.audios.remove(&id);
                this.media.audio_errors.remove(&id);
                this.media.error = Some(crate::i18n::message(
                    "media-play-error",
                    &[("error", error.0.clone())],
                ));
                cx.notify();
            });
            self.media.audio_errors.insert(id, subscription);
            self.media.audios.insert(id, playback);
        }
        self.media.error = None;
        self.media.audios[&id].update(cx, |player, cx| player.apply(command, cx));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tests::{draw, open};
    use gpui::{TestAppContext, VisualTestContext};

    #[gpui::test]
    fn failed_audio_is_removed_and_play_creates_a_fresh_session(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("invalid.mp3");
        std::fs::write(&path, b"invalid audio").unwrap();
        let asset = FileAsset::load(path).unwrap();
        let window = open(cx);
        let (id, failed) = window
            .update(cx, |w, window, cx| {
                w.add_media(asset, None, window, cx);
                let id = w.selected_shape.unwrap();
                w.control_audio(id, MediaCommand::Play, cx);
                let runtime = w.media.audios[&id].clone();
                runtime.update(cx, |runtime, cx| {
                    runtime.player.update(cx, |_, cx| {
                        cx.emit(AudioPlayerEvent::StateChanged(PlaybackState::Error(
                            Arc::new(gpui_media::MediaError::unsupported(
                                "simulated output failure",
                            )),
                        )));
                    });
                });
                (id, runtime)
            })
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        draw(&mut visual);
        window
            .update(&mut visual.cx, |w, _, cx| {
                assert!(!failed.read(cx).playing);
                assert!(!w.media.audios.contains_key(&id));
                assert!(!w.media.audio_errors.contains_key(&id));
                assert!(w.media.error.is_some());
                w.control_audio(id, MediaCommand::Play, cx);
                assert_ne!(w.media.audios[&id].entity_id(), failed.entity_id());
                assert!(w.media.error.is_none());
            })
            .unwrap();
    }
}
