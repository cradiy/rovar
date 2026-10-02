use gpui::{App, ImageSource, PathPromptOptions, Task};
use std::path::{Path, PathBuf};

pub(crate) fn now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

type Dialog<T> = Task<anyhow::Result<anyhow::Result<Option<T>>>>;

pub fn render_font_family(family: &gpui::SharedString) -> gpui::SharedString {
    #[cfg(target_family = "wasm")]
    if !matches!(
        family.as_ref(),
        "IBM Plex Sans" | "Lilex" | "Noto Sans CJK SC"
    ) {
        return "IBM Plex Sans".into();
    }
    family.clone()
}

pub fn export_directory() -> PathBuf {
    #[cfg(target_family = "wasm")]
    {
        "/tmp".into()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        dirs::document_dir().unwrap_or_else(std::env::temp_dir)
    }
}

pub fn workspace_directory() -> PathBuf {
    #[cfg(target_family = "wasm")]
    return "/workspace".into();
    #[cfg(not(target_family = "wasm"))]
    std::env::var_os("ROVAR_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("rovar")
        })
}

pub fn prompt_for_paths(cx: &App, options: PathPromptOptions) -> Dialog<Vec<PathBuf>> {
    #[cfg(not(target_family = "wasm"))]
    {
        let receiver = cx.prompt_for_paths(options);
        cx.foreground_executor()
            .spawn(async move { receiver.await.map_err(Into::into) })
    }
    #[cfg(target_family = "wasm")]
    {
        if options.directories && !options.files {
            let directory = PathBuf::from(format!("/tmp/export-{}", uuid::Uuid::new_v4()));
            return Task::ready(Ok(rovar_storage::fs::create_dir_all(&directory)
                .map(|_| Some(vec![directory]))
                .map_err(Into::into)));
        }
        let receiver = cx.prompt_for_files(gpui::FilePromptOptions {
            multiple: options.multiple,
        });
        cx.foreground_executor().spawn(async move {
            let result = async {
                let Some(files) = receiver.await?? else {
                    return Ok(None);
                };
                let directory = PathBuf::from(format!("/tmp/import-{}", uuid::Uuid::new_v4()));
                rovar_storage::fs::create_dir_all(&directory)?;
                let mut paths = Vec::new();
                for file in files {
                    let name = Path::new(file.name())
                        .file_name()
                        .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?;
                    let path = directory.join(name);
                    rovar_storage::fs::write(&path, file.read().await?)?;
                    paths.push(path);
                }
                Ok(Some(paths))
            }
            .await;
            Ok(result)
        })
    }
}

pub fn prompt_for_new_path(cx: &App, directory: &Path, name: Option<&str>) -> Dialog<PathBuf> {
    #[cfg(not(target_family = "wasm"))]
    {
        let receiver = cx.prompt_for_new_path(directory, name);
        cx.foreground_executor()
            .spawn(async move { receiver.await.map_err(Into::into) })
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = (directory, cx);
        let directory = PathBuf::from(format!("/tmp/export-{}", uuid::Uuid::new_v4()));
        Task::ready(Ok(rovar_storage::fs::create_dir_all(&directory)
            .map(|_| Some(directory.join(name.unwrap_or("Export.rovar"))))
            .map_err(Into::into)))
    }
}

pub fn preview_image(path: PathBuf) -> ImageSource {
    #[cfg(not(target_family = "wasm"))]
    {
        path.into()
    }
    #[cfg(target_family = "wasm")]
    {
        std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            rovar_storage::fs::read(path).unwrap_or_default(),
        ))
        .into()
    }
}

pub fn download(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_family = "wasm")]
    crate::web::download(path)?;
    #[cfg(not(target_family = "wasm"))]
    let _ = path;
    Ok(())
}
