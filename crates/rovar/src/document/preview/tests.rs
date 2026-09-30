use super::*;
use crate::{
    artboard::Artboard,
    text::{StyleChange, VerticalAlign},
};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn green_bounds(bytes: &[u8]) -> (u32, u32, u32, u32) {
    let pixels = image::load_from_memory(bytes).unwrap().into_rgba8();
    assert_eq!(pixels.dimensions(), (560, 336));
    let mut bounds = None;
    for (x, y, pixel) in pixels.enumerate_pixels() {
        if pixel[1] > 120 && pixel[0] < 80 && pixel[2] < 80 {
            bounds = Some(
                bounds.map_or((x, y, x, y), |(l, t, r, b): (u32, u32, u32, u32)| {
                    (l.min(x), t.min(y), r.max(x), b.max(y))
                }),
            );
        }
    }
    bounds.expect("Rich-text run must be visible in the preview")
}

#[gpui::test]
fn thumbnail_preserves_rich_text_alignment_wrapping_and_clipped_overflow(
    cx: &mut gpui::TestAppContext,
) {
    let text_system = cx.update(|cx| cx.text_system().clone());
    let content = "AB CD EF GH".to_string();
    let mut styles = StyledText::default();
    styles.replace(0..0, content.len());
    styles.apply(
        0..content.len(),
        &StyleChange::Color(gpui::rgb(0xff0000)),
        true,
    );
    styles.apply(
        3..content.len(),
        &StyleChange::Color(gpui::rgb(0x00ff00)),
        false,
    );
    styles.apply(0..2, &StyleChange::Size(48.), false);
    styles.apply(
        0..content.len(),
        &StyleChange::VerticalAlign(VerticalAlign::Top),
        true,
    );
    let mut doc = Page {
        name: "Page 1".into(),
        id: uuid::Uuid::new_v4().to_string(),
        next_id: 3,
        boards: vec![Artboard {
            color_style: None,
            id: 1,
            layer: Default::default(),
            name: "Frame".into(),
            rect: rect(0., 0., 400., 240.),
            color: gpui::rgb(0xffffff),
            fill_mode: Default::default(),
            gradient: Default::default(),
            image_fill: Default::default(),
        }],
        shapes: vec![],
        texts: vec![Text {
            id: 2,
            board: Some(1),
            rect: rect(20., 20., 320., 180.),
            layer: Default::default(),
            content,
            styles,
        }],
        hierarchy: Default::default(),
        assets: vec![],
    };
    let initial = green_bounds(&render(&doc, &[], &text_system).unwrap());
    let len = doc.texts[0].content.len();
    doc.texts[0]
        .styles
        .apply(0..len, &StyleChange::Align(gpui::TextAlign::Right), true);
    let right = green_bounds(&render(&doc, &[], &text_system).unwrap());
    assert!(
        right.0 > initial.0 + 20,
        "Horizontal alignment should move text"
    );
    doc.texts[0].styles.apply(
        0..len,
        &StyleChange::VerticalAlign(VerticalAlign::Bottom),
        true,
    );
    let bottom = green_bounds(&render(&doc, &[], &text_system).unwrap());
    assert!(
        bottom.1 > right.1 + 8,
        "Smaller runs should move within the row's font band"
    );
    doc.texts[0].styles.apply(
        0..len,
        &StyleChange::VerticalAlign(VerticalAlign::Top),
        true,
    );
    doc.texts[0]
        .styles
        .apply(0..len, &StyleChange::Size(24.), true);
    doc.texts[0].rect.width = 65.;
    let wrapped = green_bounds(&render(&doc, &[], &text_system).unwrap());
    assert!(
        wrapped.3 - wrapped.1 > initial.3 - initial.1 + 20,
        "Narrow text should wrap"
    );
    doc.texts[0].rect.height = 55.;
    let clipped = green_bounds(&render(&doc, &[], &text_system).unwrap());
    assert!(
        clipped.3 < wrapped.3,
        "Fixed-height text should clip overflowing rows"
    );
}
