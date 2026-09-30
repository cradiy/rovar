use super::*;
use crate::color_styles::{ColorStyle, Palette};

pub(crate) fn read(directory: &Path) -> Result<Palette> {
    match rovar_storage::fs::read(directory.join("colors.json")) {
        Ok(bytes) => {
            let palette: Palette = serde_json::from_slice(&bytes)?;
            for (id, style) in &palette {
                ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid color style ID");
                style.validate()?;
            }
            Ok(palette)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Palette::new()),
        Err(error) => Err(error.into()),
    }
}

impl Library {
    pub fn set_color(
        &mut self,
        id: String,
        value: Option<ColorStyle>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        ensure!(self.ready && !self.busy, "Library is busy");
        ensure!(uuid::Uuid::parse_str(&id).is_ok(), "Invalid color style ID");
        if let Some(value) = &value {
            value.validate()?;
        }
        let mut palette = self.colors.clone();
        if let Some(value) = value {
            palette.insert(id, value);
        } else {
            palette.remove(&id);
        }
        crate::remote::write_atomic(
            &self.directory.join("colors.json"),
            &serde_json::to_vec(&palette)?,
        )?;
        self.colors = palette;
        if let Some(remote) = crate::remote::Remote::existing(cx) {
            remote.update(cx, |remote, cx| {
                remote.colors_changed(&self.directory, &self.colors, cx)
            })?;
        }
        cx.notify();
        Ok(())
    }
}
