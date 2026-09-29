//! Workspace storage. Desktop uses the filesystem; Web uses in-memory files
//! restored from IndexedDB before opening the editor. Browser persistence is
//! asynchronous and must acknowledge a batch only after its transaction commits.
#[cfg(not(target_family = "wasm"))]
pub use std::fs;
#[cfg(not(target_family = "wasm"))]
pub use tempfile;

#[cfg(any(target_family = "wasm", test))]
#[cfg_attr(test, allow(dead_code))]
mod memory;
#[cfg(target_family = "wasm")]
pub use memory::{acknowledge, has_pending, pending, restore};
#[cfg(target_family = "wasm")]
pub use memory::{fs, tempfile};

pub fn exists(path: impl AsRef<std::path::Path>) -> bool {
    fs::metadata(path).is_ok()
}
pub fn is_file(path: impl AsRef<std::path::Path>) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file())
}
