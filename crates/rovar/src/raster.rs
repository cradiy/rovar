use anyhow::Result;

#[cfg(not(target_family = "wasm"))]
pub(crate) type Options = resvg::usvg::Options<'static>;
#[cfg(target_family = "wasm")]
#[derive(Default)]
pub(crate) struct Options;

/// Load the optional renderer before entering synchronous document/export code.
pub(crate) async fn prepare() -> Result<()> {
    #[cfg(target_family = "wasm")]
    {
        let (sender, receiver) = futures_channel::oneshot::channel();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = sender.send(
                browser::ensure_renderer()
                    .await
                    .map_err(|error| anyhow::anyhow!("Renderer: {error:?}")),
            );
        });
        receiver.await??;
    }
    Ok(())
}

/// A thumbnail failure must not prevent saving the document itself.
pub(crate) async fn prepare_preview() {
    if let Err(error) = prepare().await {
        eprintln!("Could not load preview renderer: {error:#}");
    }
}

#[cfg(target_family = "wasm")]
mod browser {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(module = "/src/web/renderer.js")]
    extern "C" {
        #[wasm_bindgen(catch, js_name = ensureRenderer)]
        pub async fn ensure_renderer() -> Result<(), JsValue>;
        #[wasm_bindgen(catch)]
        pub fn render(
            svg: &str,
            width: u32,
            height: u32,
            scale: f32,
            preview: bool,
            vector: bool,
        ) -> Result<Vec<u8>, JsValue>;
    }
}

pub(crate) fn render(
    svg: &str,
    size: [u32; 2],
    scale: f32,
    preview: bool,
    vector: bool,
    options: &Options,
) -> Result<Vec<u8>> {
    #[cfg(target_family = "wasm")]
    {
        let _ = options;
        browser::render(svg, size[0], size[1], scale, preview, vector)
            .map_err(|error| anyhow::anyhow!("Renderer: {error:?}"))
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let tree = resvg::usvg::Tree::from_str(svg, options)?;
        if vector {
            return Ok(tree
                .to_string(&resvg::usvg::WriteOptions::default())
                .into_bytes());
        }
        let mut pixels = resvg::tiny_skia::Pixmap::new(size[0], size[1])
            .ok_or_else(|| anyhow::anyhow!("Invalid render dimensions"))?;
        if preview {
            pixels.fill(resvg::tiny_skia::Color::from_rgba8(18, 20, 25, 255));
        }
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixels.as_mut(),
        );
        Ok(pixels.encode_png()?)
    }
}
