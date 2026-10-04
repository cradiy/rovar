use super::*;
use crate::ui::theme::Color;
use gpui::{AvailableSpace, ContentMask, EntityInputHandler, TextRun, size};

impl Workspace {
    pub(super) fn name_input(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let input = self.inspector.fields[0].clone();
        let focus = input.focus_handle(cx);
        let scroll = self.inspector.name_scroll.clone();
        div()
            .id("name-input")
            .w_full()
            .min_w_0()
            .h(px(30.))
            .px(px(5.))
            .rounded(px(4.))
            .border_1()
            .border_color(Color::Input.color())
            .track_focus(&focus)
            .focus(|el| el.border_color(ACCENT.color()))
            .text_size(px(12.))
            .line_height(px(24.))
            .text_color(TEXT.color())
            .flex()
            .items_center()
            .overflow_hidden()
            .cursor_text()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .child(
                gpui::canvas(
                    move |bounds, window, cx| {
                        let value = input.read(cx).value().replace(['\r', '\n'], " ");
                        let style = window.text_style();
                        let line = window.text_system().shape_line(
                            value.clone().into(),
                            px(12.),
                            &[TextRun {
                                len: value.len(),
                                font: style.font(),
                                color: style.color,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        );
                        let focused = input.focus_handle(cx).is_focused(window);
                        let selection = input
                            .update(cx, |input, cx| input.selected_text_range(false, window, cx));
                        let utf16 = selection.map_or(0, |s| {
                            if s.reversed {
                                s.range.start
                            } else {
                                s.range.end
                            }
                        });
                        let mut units = 0;
                        let cursor = value
                            .char_indices()
                            .find_map(|(i, c)| {
                                if units >= utf16 {
                                    Some(i)
                                } else {
                                    units += c.len_utf16();
                                    None
                                }
                            })
                            .unwrap_or(value.len());
                        let caret = line.x_for_index(cursor);
                        let available = bounds.size.width.max(px(1.));
                        let width = (line.width + px(2.)).max(available);
                        let mut offset = scroll.get().min((width - available).max(px(0.)));
                        if !focused {
                            offset = px(0.);
                        } else if caret < offset {
                            offset = caret;
                        } else if caret + px(2.) > offset + available {
                            offset = caret + px(2.) - available;
                        }
                        scroll.set(offset);
                        let mut child = div()
                            .w(width)
                            .h_full()
                            .child(input.clone())
                            .into_any_element();
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            child.prepaint_as_root(
                                bounds.origin - point(offset, px(0.)),
                                size(
                                    AvailableSpace::Definite(width),
                                    AvailableSpace::Definite(bounds.size.height),
                                ),
                                window,
                                cx,
                            );
                        });
                        child
                    },
                    |bounds, mut child, window, cx| {
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            child.paint(window, cx)
                        })
                    },
                )
                .w_full()
                .h(px(24.)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tests::{click, create, draw, open};
    use gpui::{TestAppContext, VisualTestContext};

    #[gpui::test]
    fn long_unicode_name_stays_bounded_and_scrolls_with_caret(cx: &mut TestAppContext) {
        let window = open(cx);
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        create(&mut visual, "add-rectangle");
        let name = "设计草稿📷-长名称-".repeat(30);
        window
            .update(&mut visual.cx, |this, _, cx| {
                this.shapes[0].name = name.clone();
                this.panels.set(panels::Side::Right, 256.);
                this.sync_fields(cx);
                cx.notify();
            })
            .unwrap();
        draw(&mut visual);
        let field = visual.debug_bounds("property-0").unwrap();
        let panel = visual.debug_bounds("properties-panel").unwrap();
        assert!(field.right() < panel.right());
        click(&mut visual, "property-0");
        visual.simulate_keystrokes("end");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert!(this.inspector.name_scroll.get() > px(100.))
            })
            .unwrap();
        visual.simulate_input("末尾");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[0].name, format!("{name}末尾"))
            })
            .unwrap();
        visual.simulate_keystrokes("home");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.inspector.name_scroll.get(), px(0.))
            })
            .unwrap();
        visual.simulate_input("开头");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                assert_eq!(this.shapes[0].name, format!("开头{name}末尾"))
            })
            .unwrap();
        visual.simulate_keystrokes("end");
        draw(&mut visual);
        click(&mut visual, "property-1");
        window
            .update(&mut visual.cx, |this, _, cx| {
                assert_eq!(this.inspector.name_scroll.get(), px(0.));
                assert_eq!(
                    this.inspector.fields[0].read(cx).value().as_ref(),
                    format!("开头{name}末尾")
                );
            })
            .unwrap();
    }
}
