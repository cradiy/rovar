use std::sync::atomic::{AtomicBool, Ordering};
use std::{borrow::Cow, path::Path, sync::OnceLock};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/web/storage.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn initialize() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    async fn persist(changes: js_sys::Array) -> Result<(), JsValue>;
    fn status(message: &str, failed: bool);
    fn watch(callback: &js_sys::Function);
    fn downloadBytes(name: &str, bytes: &[u8]);
}
static FONTS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
static UNSAVED: AtomicBool = AtomicBool::new(false);
static SAVE_FAILED: AtomicBool = AtomicBool::new(false);
pub fn set_unsaved(unsaved: bool) {
    UNSAVED.store(unsaved, Ordering::Relaxed);
}
pub fn fonts() -> &'static [Vec<u8>] {
    FONTS.get().map(Vec::as_slice).unwrap_or_default()
}
pub fn load_fonts(cx: &mut gpui::App) {
    let _ = cx
        .text_system()
        .add_fonts(fonts().iter().cloned().map(Cow::Owned).collect());
}
fn error(value: JsValue) -> anyhow::Error {
    anyhow::anyhow!("Browser storage: {value:?}")
}
pub fn start() {
    console_error_panic_hook::set_once();
    gpui_web::init_logging();
    wasm_bindgen_futures::spawn_local(async {
        let result = async {
            let data = initialize().await.map_err(error)?;
            let entries =
                js_sys::Array::from(&js_sys::Reflect::get(&data, &"files".into()).map_err(error)?);
            let files = entries
                .iter()
                .map(|entry| {
                    let entry = js_sys::Array::from(&entry);
                    (
                        entry.get(0).as_string().unwrap(),
                        js_sys::Uint8Array::new(&entry.get(1)).to_vec(),
                    )
                })
                .collect();
            rovar_storage::restore(files);
            let fonts =
                js_sys::Array::from(&js_sys::Reflect::get(&data, &"fonts".into()).map_err(error)?);
            let _ = FONTS.set(
                fonts
                    .iter()
                    .map(|f| js_sys::Uint8Array::new(&f).to_vec())
                    .collect(),
            );
            crate::run_app();
            let callback = Closure::<dyn FnMut() -> bool>::new(|| {
                rovar_storage::has_pending() || UNSAVED.load(Ordering::Relaxed)
            });
            watch(callback.as_ref().unchecked_ref());
            callback.forget();
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = result {
            status(&error.to_string(), true);
        }
    });
}

#[wasm_bindgen]
pub async fn flush_workspace() {
    let (revision, changes) = rovar_storage::pending();
    if changes.is_empty() {
        return;
    }
    // Session writes remember pan, zoom and active pages without changing a
    // document. Keep those background writes out of the document save feedback.
    let notify = changes.iter().any(|(name, _)| {
        Path::new(name)
            .extension()
            .is_some_and(|ext| ext == "rovar")
    });
    if notify {
        status("Saving locally…", false);
    }
    let entries = js_sys::Array::new();
    for (name, bytes) in changes {
        let entry = js_sys::Array::new();
        entry.push(&name.into());
        entry.push(
            &bytes
                .map(|b| JsValue::from(js_sys::Uint8Array::from(b.as_slice())))
                .unwrap_or(JsValue::NULL),
        );
        entries.push(&entry);
    }
    match persist(entries).await {
        Ok(()) => {
            rovar_storage::acknowledge(revision);
            let recovered = SAVE_FAILED.swap(false, Ordering::Relaxed);
            if notify || recovered {
                status("Saved locally", false);
            }
        }
        Err(error) => {
            SAVE_FAILED.store(true, Ordering::Relaxed);
            status(
                &format!("Could not save locally: {error:?}. Export your document to keep a copy."),
                true,
            );
        }
    }
}
pub fn download(path: &Path) -> anyhow::Result<()> {
    let bytes = rovar_storage::fs::read(path)?;
    downloadBytes(
        &path.file_name().unwrap_or_default().to_string_lossy(),
        &bytes,
    );
    rovar_storage::fs::remove_file(path)?;
    Ok(())
}
