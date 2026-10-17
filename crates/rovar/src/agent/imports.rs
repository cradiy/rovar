use super::protocol::{
    Error, Result,
    import::{ImageSource, Source, SvgSource},
};
use crate::asset::{AssetContent, FileAsset};
use base64::Engine as _;
use std::{collections::BTreeMap, io::Write, path::Path, sync::Arc};

pub(crate) const MAX_DATA_BYTES: usize = 16 * 1024 * 1024;
pub(crate) type Prepared = BTreeMap<usize, Arc<FileAsset>>;

pub(crate) fn prepare(sources: Vec<(usize, Source)>) -> Result<Prepared> {
    sources
        .into_iter()
        .map(|(index, source)| {
            load(source)
                .map(|asset| (index, asset))
                .map_err(|mut error| {
                    error.operation = Some(index);
                    error
                })
        })
        .collect()
}

fn load(source: Source) -> Result<Arc<FileAsset>> {
    let result = match &source {
        Source::Image {
            source: ImageSource::Path { path },
        }
        | Source::Svg {
            source: SvgSource::Path { path },
            ..
        }
        | Source::Video { path }
        | Source::Audio { path } => {
            let path = Path::new(path);
            if !path.is_absolute() || !path.metadata().is_ok_and(|metadata| metadata.is_file()) {
                return Err(Error::invalid(
                    "Import path must name an existing absolute local file",
                ));
            }
            FileAsset::load(path.to_owned())
        }
        Source::Image {
            source: ImageSource::Data { base64 },
        } => {
            if base64.len() > MAX_DATA_BYTES.div_ceil(3) * 4 {
                return Err(Error::new("LIMIT_REACHED", "Image data exceeds 16 MiB"));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(base64)
                .map_err(|_| Error::invalid("Image data must be standard base64"))?;
            let format = image::guess_format(&bytes)
                .map_err(|error| Error::new("IMPORT_FAILED", error.to_string()))?;
            from_bytes(&bytes, &format!("image.{}", format.extensions_str()[0]))
        }
        Source::Svg {
            source: SvgSource::Data { svg },
            ..
        } => from_bytes(svg.as_bytes(), "image.svg"),
    };
    let asset = result.map_err(|error| Error::new("IMPORT_FAILED", error.to_string()))?;
    let matches = matches!(
        (&source, asset.content()),
        (Source::Image { .. }, Some(AssetContent::Image(_)))
            | (Source::Svg { .. }, Some(AssetContent::Svg(_)))
            | (Source::Video { .. }, Some(AssetContent::Video(_)))
            | (Source::Audio { .. }, Some(AssetContent::Audio(_)))
    );
    if !matches {
        return Err(Error::invalid(
            "File content does not match the requested import type",
        ));
    }
    if let Source::Svg { editable: true, .. } = source
        && let Some(AssetContent::Svg(svg)) = asset.content()
        && let Err(reason) = &svg.editable
    {
        return Err(Error::new("SVG_NOT_EDITABLE", reason.clone()));
    }
    Ok(asset)
}

fn from_bytes(bytes: &[u8], name: &str) -> anyhow::Result<Arc<FileAsset>> {
    use sha2::{Digest, Sha256};
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_DATA_BYTES,
        "Import data must contain 1 to 16 MiB of bytes"
    );
    let mut file = rovar_storage::tempfile::Builder::new()
        .suffix(&format!(
            ".{}",
            Path::new(name).extension().unwrap().to_string_lossy()
        ))
        .tempfile()?;
    file.write_all(bytes)?;
    file.flush()?;
    FileAsset::from_cached(
        name.into(),
        crate::asset::Source::file(file.into_temp_path()),
        hex::encode(Sha256::digest(bytes)),
    )
}
