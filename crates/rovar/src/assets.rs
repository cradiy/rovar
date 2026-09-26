use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

const EMBEDDED: [(&str, &[u8]); 4] = [
    (
        "rovar-mark.svg",
        include_bytes!("../../../assets/rovar-mark.svg"),
    ),
    (
        "loading/bottom.svg",
        include_bytes!("../../../assets/loading/bottom.svg"),
    ),
    (
        "loading/upper.svg",
        include_bytes!("../../../assets/loading/upper.svg"),
    ),
    (
        "loading/left.svg",
        include_bytes!("../../../assets/loading/left.svg"),
    ),
];

pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = EMBEDDED.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        uic::assets::LucideAssets::new().load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = uic::assets::LucideAssets::new().list(path)?;
        paths.extend(
            EMBEDDED
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| (*name).into()),
        );
        Ok(paths)
    }
}
