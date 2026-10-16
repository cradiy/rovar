use super::*;
use std::cell::RefCell;
use uic::components::input::{Input, InputAppearance};

pub(super) const COMMANDS: [MediaCommand; 11] = [
    MediaCommand::Play,
    MediaCommand::Pause,
    MediaCommand::Toggle,
    MediaCommand::Restart,
    MediaCommand::SeekBy { seconds: 10 },
    MediaCommand::SeekBy { seconds: -10 },
    MediaCommand::SeekTo { seconds: 0 },
    MediaCommand::Rate { percent: 150 },
    MediaCommand::Volume { percent: 50 },
    MediaCommand::Mute,
    MediaCommand::Unmute,
];

pub(super) fn label(command: MediaCommand) -> &'static str {
    match command {
        MediaCommand::Play => "prototype-media-play",
        MediaCommand::Pause => "prototype-media-pause",
        MediaCommand::Toggle => "prototype-media-toggle",
        MediaCommand::Restart => "prototype-media-restart",
        MediaCommand::SeekBy { seconds } if seconds < 0 => "prototype-media-rewind",
        MediaCommand::SeekBy { .. } => "prototype-media-forward",
        MediaCommand::SeekTo { .. } => "prototype-media-seek",
        MediaCommand::Rate { .. } => "prototype-media-rate",
        MediaCommand::Volume { .. } => "prototype-media-volume",
        MediaCommand::Mute => "prototype-media-mute",
        MediaCommand::Unmute => "prototype-media-unmute",
    }
}

fn parameter(command: MediaCommand) -> Option<(&'static str, f64)> {
    Some(match command {
        MediaCommand::SeekBy { seconds } => ("prototype-media-seconds", f64::from(seconds).abs()),
        MediaCommand::SeekTo { seconds } => ("prototype-media-time", f64::from(seconds)),
        MediaCommand::Rate { percent } => ("prototype-media-speed", f64::from(percent) / 100.),
        MediaCommand::Volume { percent } => ("prototype-media-level", f64::from(percent)),
        _ => return None,
    })
}

pub(super) struct State {
    input: Entity<TextInput>,
    target: RefCell<Option<(usize, usize, Trigger, Action)>>,
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let input = cx.new(TextInput::new);
        let submit = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Submit(_)) {
                this.apply_media_parameter(window, cx);
            }
        });
        let blur = cx.on_blur(&input.focus_handle(cx), window, |this, window, cx| {
            this.apply_media_parameter(window, cx);
        });
        Self {
            input,
            target: RefCell::new(None),
            _subscriptions: vec![submit, blur],
        }
    }
}

impl Workspace {
    pub(super) fn prototype_media(&self, cx: &gpui::App) -> Vec<(usize, String)> {
        self.shapes
            .iter()
            .filter(|s| s.kind.is_playable() && !self.effective_layer(s.layer, s.board).hidden)
            .map(|s| (s.id, self.layer_name(s.id, cx)))
            .collect()
    }

    fn apply_media_parameter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((generation, id, trigger, action @ Action::Media { target, command })) =
            self.presentation.media.target.borrow().clone()
        else {
            return;
        };
        if generation != self.pages.generation
            || self.selection_ids() != BTreeSet::from([id])
            || self.presentation.trigger != trigger
            || self
                .hierarchy
                .interactions
                .get(&id)
                .and_then(|i| i.get(trigger))
                != Some(action)
        {
            return;
        }
        let Some((_, previous)) = parameter(command) else {
            return;
        };
        let value = self
            .presentation
            .media
            .input
            .read(cx)
            .value()
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite());
        let updated = value.and_then(|value| {
            Some(match command {
                MediaCommand::SeekBy { seconds } if (1. ..=86400.).contains(&value) => {
                    MediaCommand::SeekBy {
                        seconds: value.round() as i32 * seconds.signum(),
                    }
                }
                MediaCommand::SeekTo { .. } if (0. ..=86400.).contains(&value) => {
                    MediaCommand::SeekTo {
                        seconds: value.round() as u32,
                    }
                }
                MediaCommand::Rate { .. } if (0.25..=4.).contains(&value) => MediaCommand::Rate {
                    percent: (value * 100.).round() as u16,
                },
                MediaCommand::Volume { .. } if (0. ..=100.).contains(&value) => {
                    MediaCommand::Volume {
                        percent: value.round() as u8,
                    }
                }
                _ => return None,
            })
        });
        if let Some(command) = updated {
            self.set_prototype_action(
                id,
                trigger,
                Some(Action::Media { target, command }),
                window,
                cx,
            );
        }
        let value = updated
            .and_then(parameter)
            .map_or(previous, |(_, value)| value);
        self.presentation
            .media
            .input
            .update(cx, |input, cx| input.set_value(value.to_string(), cx));
    }

    pub(super) fn media_action_controls(
        &self,
        id: usize,
        trigger: Trigger,
        action: Option<Action>,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let mut el = div().flex().flex_col().gap(px(6.));
        let Some(action @ Action::Media { command, .. }) = action else {
            self.presentation.media.target.replace(None);
            return None;
        };
        el = el
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t("prototype-media-operation")),
            )
            .child(self.presentation_menu(Some(id), controls::Menu::MediaControl, cx));
        let Some((label, value)) = parameter(command) else {
            self.presentation.media.target.replace(None);
            return Some(el);
        };
        let key = Some((self.pages.generation, id, trigger, action));
        if self.presentation.media.target.replace(key.clone()) != key {
            self.presentation
                .media
                .input
                .update(cx, |input, cx| input.set_value(value.to_string(), cx));
        }
        el = el
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t(label)),
            )
            .child(
                Input::new(&self.presentation.media.input)
                    .w_full()
                    .h(px(32.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .bg(Color::Input.color())
                    .border_color(Color::Hover.color())
                    .text_size(px(12.))
                    .text_color(TEXT.color())
                    .appearance(InputAppearance {
                        caret_height: px(16.),
                        ..crate::ui::theme::input_appearance()
                    }),
            );
        Some(el)
    }
}
