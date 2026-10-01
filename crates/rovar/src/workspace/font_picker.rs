use super::{ACCENT, BORDER, MUTED, PANEL, TEXT, icon};
use crate::i18n::t;
use gpui::{
    Context, Entity, EventEmitter, Focusable, IntoElement, Render, ScrollStrategy, SharedString,
    Subscription, Task, UniformListScrollHandle, Window, div, prelude::*, px, rgb, uniform_list,
};
use uic::{
    assets::LucideIcons,
    components::{
        input::{Input, InputAppearance, InputEvent, TextInput},
        popover::{Popover, PopoverEvent, PopoverPlacement, PopoverState},
    },
};

pub(crate) struct FontChosen(pub SharedString);

pub(crate) struct FontPicker {
    fonts: Vec<SharedString>,
    filtered: Vec<usize>,
    selected: SharedString,
    show_label: bool,
    loading: bool,
    failed: bool,
    search: Entity<TextInput>,
    popover: Entity<PopoverState>,
    scroll: UniformListScrollHandle,
    _scan: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FontChosen> for FontPicker {}

impl FontPicker {
    pub(crate) fn refresh_language(&mut self, old_mixed: &str, cx: &mut Context<Self>) {
        if self.selected.as_ref() == old_mixed {
            self.selected = t("font-mixed").into();
        }
        self.search.update(cx, |input, cx| {
            input.set_placeholder(t("font-search"));
            cx.notify();
        });
        cx.notify();
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new(cx).placeholder(t("font-search")));
        let popover = cx.new(|cx| PopoverState::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(
                &search,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change(_) => this.filter(cx),
                    InputEvent::Submit(_) => {
                        if let Some(&index) = this.filtered.first() {
                            this.choose(index, window, cx);
                        }
                    }
                },
            ),
            cx.subscribe_in(
                &popover,
                window,
                |this, _, event: &PopoverEvent, window, cx| {
                    if *event == PopoverEvent::Opened {
                        this.search.update(cx, |input, cx| input.clear(cx));
                        // Focus after the popover has attached the search field to its focus tree.
                        let search = this.search.clone();
                        let popover = this.popover.clone();
                        window.on_next_frame(move |window, cx| {
                            if popover.read(cx).is_open() {
                                search.focus_handle(cx).focus(window, cx);
                            }
                        });
                    }
                    cx.notify();
                },
            ),
        ];
        let mut picker = Self {
            fonts: Vec::new(),
            filtered: Vec::new(),
            selected: "".into(),
            show_label: true,
            loading: false,
            failed: false,
            search,
            popover,
            scroll: UniformListScrollHandle::new(),
            _scan: Task::ready(()),
            _subscriptions: subscriptions,
        };
        // UI tests provide their own catalog and never inspect the host's fonts.
        if !cfg!(test) {
            picker.scan(cx);
        }
        picker
    }

    fn scan(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.failed = false;
        let scan = cx.background_executor().spawn(async {
            #[cfg(not(target_family = "wasm"))]
            {
                font_kit::source::SystemSource::new()
                    .all_families()
                    .map_err(|e| e.to_string())
            }
            #[cfg(target_family = "wasm")]
            {
                Ok::<Vec<String>, String>(vec![
                    "IBM Plex Sans".into(),
                    "Lilex".into(),
                    "Noto Sans CJK SC".into(),
                ])
            }
        });
        self._scan = cx.spawn(async move |this, cx| {
            let result = scan.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(mut names) => {
                        names.retain(|name| !name.trim().is_empty());
                        names.sort_by_cached_key(|name| name.to_lowercase());
                        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                        this.fonts = names.into_iter().map(Into::into).collect();
                    }
                    Err(error) => {
                        this.failed = true;
                        eprintln!("Could not read system fonts: {error}");
                    }
                }
                this.filter(cx);
            });
        });
        cx.notify();
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().trim().to_lowercase();
        self.filtered = self
            .fonts
            .iter()
            .enumerate()
            .filter_map(|(index, name)| name.to_lowercase().contains(&query).then_some(index))
            .collect();
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub fn set_selected(&mut self, family: SharedString, cx: &mut Context<Self>) {
        if self.selected != family {
            self.selected = family;
            cx.notify();
        }
    }

    pub(crate) fn hide_label(&mut self) {
        self.show_label = false;
    }

    pub(crate) fn is_open(&self, cx: &gpui::App) -> bool {
        self.popover.read(cx).is_open()
    }

    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.popover.update(cx, |state, cx| state.close(window, cx));
    }

    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.failed || !self.popover.read(cx).is_open() {
            return;
        }
        let Some(family) = self.fonts.get(index).cloned() else {
            return;
        };
        self.selected = family.clone();
        cx.emit(FontChosen(family));
        self.popover.update(cx, |state, cx| state.close(window, cx));
        cx.notify();
    }

    fn menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let message = if self.loading {
            t("font-scanning")
        } else if self.failed {
            t("font-scan-failed")
        } else if self.fonts.is_empty() {
            t("font-none")
        } else {
            t("font-no-match")
        };
        div()
            .w(px(280.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .id("font-search")
                    .debug_selector(|| "font-search".into())
                    .child(
                        Input::new(&self.search)
                            .h(px(32.))
                            .px(px(9.))
                            .text_size(px(12.))
                            .rounded(px(5.))
                            .bg(rgb(0x252830))
                            .border_color(rgb(BORDER))
                            .text_color(rgb(TEXT))
                            .appearance(InputAppearance {
                                focus_border: rgb(ACCENT).into(),
                                caret: rgb(ACCENT).into(),
                                selection: gpui::rgba(0xb4a2ee44).into(),
                                ..Default::default()
                            }),
                    ),
            )
            .when(
                !self.filtered.is_empty() && !self.loading && !self.failed,
                |el| {
                    el.child(
                        uniform_list(
                            "system-fonts",
                            self.filtered.len(),
                            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|row| {
                                        let index = this.filtered[row];
                                        let family = this.fonts[index].clone();
                                        let selected = family == this.selected;
                                        div()
                                            .id(("font-option", index))
                                            .debug_selector(move || format!("font-option-{row}"))
                                            .w_full()
                                            .h(px(32.))
                                            .px(px(8.))
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .text_size(px(12.))
                                            .text_color(rgb(TEXT))
                                            .when(selected, |el| {
                                                el.bg(rgb(0x353044)).text_color(rgb(ACCENT))
                                            })
                                            .hover(|s| s.bg(rgb(BORDER)))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .text_ellipsis()
                                                    .font_family(family.clone())
                                                    .child(family),
                                            )
                                            .when(selected, |el| {
                                                el.child(icon(LucideIcons::Check, 14.))
                                            })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.choose(index, window, cx)
                                            }))
                                    })
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.scroll)
                        .h(px((self.filtered.len().min(8) * 32) as f32))
                        .w_full(),
                    )
                },
            )
            .when(
                self.filtered.is_empty() || self.loading || self.failed,
                |el| {
                    el.child(
                        div()
                            .py(px(18.))
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(message),
                    )
                },
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .pt(px(8.))
                    .flex()
                    .justify_between()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(crate::i18n::count("font-count", self.fonts.len()))
                    .child(
                        div()
                            .id("refresh-fonts")
                            .cursor_pointer()
                            .text_color(rgb(ACCENT))
                            .child(t("font-rescan"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if !this.loading {
                                    this.scan(cx);
                                }
                            })),
                    ),
            )
    }
}

impl Render for FontPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(7.))
            .when(self.show_label, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(t("font")),
                )
            })
            .child(
                Popover::new(&self.popover)
                    .label(t("font-choose"))
                    .placement(PopoverPlacement::BottomEnd)
                    .bg(rgb(PANEL))
                    .border_color(rgb(BORDER))
                    .p(px(8.))
                    .trigger(
                        div()
                            .id("font-picker")
                            .debug_selector(|| "font-picker".into())
                            .h(px(32.))
                            .px(px(9.))
                            .rounded(px(5.))
                            .bg(rgb(0x252830))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .text_color(rgb(TEXT))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .when(self.selected.as_ref() != t("font-mixed"), |el| {
                                        el.font_family(self.selected.clone())
                                    })
                                    .child(if self.selected.as_ref() == crate::ui_font::SYSTEM {
                                        t("settings-system-font").into()
                                    } else {
                                        self.selected.clone()
                                    }),
                            )
                            .child(icon(LucideIcons::ChevronDown, 14.).text_color(rgb(MUTED))),
                    )
                    .content(move |_, cx| {
                        div().children(weak.update(cx, |this, cx| this.menu(cx)).ok())
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{click, create, draw, open};
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};

    #[gpui::test]
    fn search_only_filters_and_choosing_changes_only_the_selected_text(cx: &mut TestAppContext) {
        let window = open(cx);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        create(&mut visual, "add-artboard");
        create(&mut visual, "add-text");
        visual.simulate_input("first");
        create(&mut visual, "add-text");
        visual.simulate_input("second");
        window
            .update(&mut visual.cx, |this, _, cx| {
                this.font_picker.update(cx, |picker, cx| {
                    picker._scan = Task::ready(());
                    picker.loading = false;
                    picker.fonts = vec!["Fixture Sans".into(), "Fixture Serif".into()];
                    picker.filter(cx);
                })
            })
            .unwrap();
        click(&mut visual, "font-picker");
        click(&mut visual, "font-search");
        visual.simulate_input("serif");
        draw(&mut visual);
        assert!(visual.debug_bounds("property-0").is_none());
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert_eq!(
                    this.texts[1].editor.read(cx).effective_style().family,
                    crate::text::TextStyle::default().family
                );
                assert_eq!(this.font_picker.read(cx).filtered, vec![1]);
            })
            .unwrap();
        click(&mut visual, "font-option-0");
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert_eq!(
                    this.texts[0].editor.read(cx).effective_style().family,
                    crate::text::TextStyle::default().family
                );
                assert_eq!(
                    this.texts[1]
                        .editor
                        .read(cx)
                        .effective_style()
                        .family
                        .as_ref(),
                    "Fixture Serif"
                );
                assert_eq!(this.texts[1].editor.read(cx).content, "second");
                assert!(!this.font_picker.read(cx).popover.read(cx).is_open());
            })
            .unwrap();
        click(&mut visual, "font-picker");
        click(&mut visual, "font-search");
        visual.simulate_input("not-installed");
        visual.simulate_keystrokes("enter");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert_eq!(
                    this.texts[1]
                        .editor
                        .read(cx)
                        .effective_style()
                        .family
                        .as_ref(),
                    "Fixture Serif"
                )
            })
            .unwrap();
        visual.simulate_keystrokes("escape");
        draw(&mut visual);
        assert!(visual.debug_bounds("font-search").is_none());
    }
}
