//! Persistent inspector controls and the subscriptions that bind them to the editor.

use super::*;

pub(in crate::editor) struct State {
    pub gradient_menu: Entity<uic::components::dropdown::DropdownState>,
    pub stroke_editing: bool,
    pub paint_stops: [usize; 2],
    pub paint_popovers: [Entity<uic::components::popover::PopoverState>; 2],
    pub inspector_scroll: gpui::ScrollHandle,
    pub fields: Vec<Entity<TextInput>>,
    pub name_scroll: Rc<Cell<Pixels>>,
    pub pending: Vec<(usize, gpui::SharedString)>,
    pub invalid: [bool; PROPERTY_COUNT * PROPERTY_SURFACES],
    pub active_stop: usize,
    pub gradient_stop_scroll: gpui::ScrollHandle,
    pub picker: Entity<ColorPickerState>,
    pub alpha_picker: Entity<ColorPickerState>,
    pub font_picker: Entity<font_picker::FontPicker>,
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let fields: Vec<_> = (0..PROPERTY_COUNT * PROPERTY_SURFACES)
            .map(|_| cx.new(TextInput::new))
            .collect();
        fields[0].update(cx, |input, cx| {
            input.set_appearance(
                uic::components::input::InputAppearance {
                    caret: rgb(ACCENT).into(),
                    selection: gpui::rgba(0xb4a2ee44).into(),
                    caret_height: px(16.),
                    ..Default::default()
                },
                cx,
            )
        });
        let paint_popovers = std::array::from_fn(|_| {
            cx.new(|cx| uic::components::popover::PopoverState::new(window, cx))
        });
        let picker = cx.new(|cx| ColorPickerState::new(rgb(0xffffff), cx));
        let alpha_picker = cx.new(|cx| ColorPickerState::new(rgb(0xffffff), cx));
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.observe(&fields[0], |_, _, cx| cx.notify()));
        for (index, popover) in paint_popovers.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                popover,
                window,
                move |this, _, event: &uic::components::popover::PopoverEvent, window, cx| {
                    if *event == uic::components::popover::PopoverEvent::Opened {
                        this.inspector.paint_popovers[1 - index].update(cx, |state, cx| {
                            state.dismiss(
                                uic::components::popover::PopoverDismissReason::Programmatic,
                                false,
                                window,
                                cx,
                            )
                        });
                        if this.selected_shape.is_some() {
                            this.activate_paint(index == 1, cx);
                        }
                    }
                    cx.notify();
                },
            ));
        }
        let font_picker = cx.new(|cx| font_picker::FontPicker::new(window, cx));
        subscriptions.push(cx.subscribe(
            &font_picker,
            |this, _, event: &font_picker::FontChosen, cx| {
                this.history.borrow_mut().break_group();
                if let Some(text) = this.selected_text_mut() {
                    text.editor.update(cx, |editor, cx| {
                        editor.apply_style(StyleChange::Family(event.0.clone()), cx)
                    });
                    cx.notify();
                }
            },
        ));
        for (index, field) in fields.iter().enumerate() {
            subscriptions.push(cx.subscribe(field, move |this, _, event: &InputEvent, cx| {
                this.edit_field(index, event, cx);
            }));
            let focus = field.read(cx).focus_handle(cx);
            subscriptions.push(cx.on_blur(&focus, window, move |this, _, cx| {
                this.history.borrow_mut().break_group();
                this.sync_input_field(index, cx);
            }));
        }
        for (state, alpha_only) in [(&picker, false), (&alpha_picker, true)] {
            subscriptions.push(cx.subscribe(
                state,
                move |this, _, event: &ColorPickerEvent, cx| {
                    let (ColorPickerEvent::Preview(color) | ColorPickerEvent::Commit(color)) =
                        *event;
                    if !this.multi_selection.is_empty() {
                        let index = if alpha_only { 6 } else { 5 };
                        let value = if alpha_only {
                            inspector::number(color.a * 100.)
                        } else {
                            inspector::hex(color)
                        };
                        this.edit_multi_field(
                            if alpha_only {
                                Property::Opacity
                            } else {
                                Property::Color
                            },
                            &value,
                            cx,
                        );
                        if matches!(event, ColorPickerEvent::Commit(_)) {
                            this.history.borrow_mut().break_group();
                        }
                        this.sync_field(index, cx);
                        if let Some(color) = this.multi_picker_color(cx) {
                            this.sync_picker(color, cx);
                        }
                        return;
                    }
                    let stop = this.inspector.active_stop;
                    let target = this.selected_text.or(this.selected_shape).or(this.selected);
                    if let Some(id) = target {
                        this.history
                            .borrow_mut()
                            .set_scope(Group::Color(id, stop), true);
                    }
                    if this.selected_text.is_some() {
                        this.change_text_color(color, alpha_only, cx);
                    } else if this.selected_shape.is_some() {
                        let stroke = this.inspector.stroke_editing;
                        this.edit_shape(|shape| {
                            if let Some(target) = shape.paint_color_mut(stop, stroke) {
                                if alpha_only {
                                    target.a = color.a;
                                } else {
                                    target.r = color.r;
                                    target.g = color.g;
                                    target.b = color.b;
                                }
                            }
                        });
                    } else {
                        this.edit_board(None, |board| {
                            if let Some(target) = board.editable_color_mut(stop) {
                                if alpha_only {
                                    target.a = color.a;
                                } else {
                                    target.r = color.r;
                                    target.g = color.g;
                                    target.b = color.b;
                                }
                            }
                        });
                    }
                    this.history.borrow_mut().clear_scope();
                    if matches!(event, ColorPickerEvent::Commit(_)) {
                        this.history.borrow_mut().break_group();
                    }
                    this.sync_field(
                        if this.selected_shape.is_some() && this.inspector.stroke_editing {
                            if alpha_only { 17 } else { 16 }
                        } else {
                            if alpha_only { 6 } else { 5 }
                        },
                        cx,
                    );
                    this.sync_picker(color, cx);
                    cx.notify();
                },
            ));
        }
        Self {
            gradient_menu: cx.new(|cx| uic::components::dropdown::DropdownState::new(window, cx)),
            name_scroll: Rc::new(Cell::new(px(0.))),
            stroke_editing: false,
            paint_stops: [0; 2],
            paint_popovers,
            inspector_scroll: Default::default(),
            fields,
            pending: Vec::new(),
            invalid: [false; PROPERTY_COUNT * PROPERTY_SURFACES],
            active_stop: 0,
            gradient_stop_scroll: gpui::ScrollHandle::default(),
            picker,
            alpha_picker,
            font_picker,
            _subscriptions: subscriptions,
        }
    }
}
