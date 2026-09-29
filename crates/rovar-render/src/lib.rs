use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! {
    static OPTIONS: RefCell<resvg::usvg::Options<'static>> = RefCell::new({
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().set_serif_family("IBM Plex Sans");
        options.fontdb_mut().set_sans_serif_family("IBM Plex Sans");
        options
    });
}

#[wasm_bindgen]
pub fn add_font(bytes: Vec<u8>) {
    OPTIONS.with(|options| options.borrow_mut().fontdb_mut().load_font_data(bytes));
}

#[wasm_bindgen]
pub fn render(
    svg: &str,
    width: u32,
    height: u32,
    scale: f32,
    preview: bool,
    vector: bool,
) -> Result<Vec<u8>, JsValue> {
    if !vector
        && (width == 0
            || height == 0
            || width > 16384
            || height > 16384
            || u64::from(width) * u64::from(height) > 64_000_000
            || !scale.is_finite()
            || scale <= 0.)
    {
        return Err(JsValue::from_str("Invalid render dimensions"));
    }
    OPTIONS.with(|options| {
        let options = options.borrow();
        let tree = resvg::usvg::Tree::from_str(svg, &options)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        if vector {
            return Ok(tree
                .to_string(&resvg::usvg::WriteOptions::default())
                .into_bytes());
        }
        let mut pixels = resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| JsValue::from_str("Invalid render dimensions"))?;
        if preview {
            pixels.fill(resvg::tiny_skia::Color::from_rgba8(18, 20, 25, 255));
        }
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixels.as_mut(),
        );
        pixels
            .encode_png()
            .map_err(|error| JsValue::from_str(&error.to_string()))
    })
}
