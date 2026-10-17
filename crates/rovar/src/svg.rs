use crate::asset::{AssetContent, Source};
use anyhow::{Result, ensure};
use base64::Engine;
use std::sync::{Arc, Mutex};
use std::{io::Read, path::Path};
mod raster;

pub(crate) struct SvgAsset {
    pub uri: String,
    pub size: [f32; 2],
    pub editable: Result<Vec<rovar_svg::Node>, String>,
    source: Arc<[u8]>,
    raster: Arc<Mutex<raster::State>>,
}

pub(crate) fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
}

pub(crate) fn decode(source: &Source) -> Result<(u32, u32, AssetContent)> {
    let mut bytes = Vec::new();
    source
        .open()?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "SVG exceeds the 16 MiB import limit"
    );
    #[cfg(not(target_family = "wasm"))]
    let svg = {
        let mut options = crate::render::svg::render_options(true)?;
        options.image_href_resolver.resolve_string = Box::new(|_, _| None);
        rovar_svg::parse(&bytes, &options).map_err(anyhow::Error::msg)?
    };
    #[cfg(target_family = "wasm")]
    let svg = crate::render::raster::import_svg(&bytes)?;
    Ok((
        svg.width,
        svg.height,
        AssetContent::Svg(SvgAsset {
            size: svg.size,
            uri: format!(
                "data:image/svg+xml;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&svg.source)
            ),
            editable: svg.editable,
            source: svg.source.into_bytes().into(),
            raster: Default::default(),
        }),
    ))
}
