//! Indexed, append-only Rovar containers. No editor or GUI dependencies.
//!
//! A writer publishes immutable blocks by committing an index. Readers retain a
//! snapshot and may stream individual blocks without loading the container.
pub mod delta;
mod layout;
mod reader;
mod version;
mod writer;

use anyhow::Result;
pub use reader::{Block, BlockHandle, BlockReader, Reader};
#[cfg(unix)]
use std::fs::File;
use std::path::Path;
pub use version::{VERSION, Version};
pub use writer::Writer;

/// Sync a published directory entry as well as the file's contents on Unix.
pub fn sync_parent(_path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(_path.parent().unwrap_or(Path::new(".")))?.sync_all()?;
    Ok(())
}

/// Rewrite only live blocks into a new file, then atomically replace the target.
/// Existing readers keep their original file snapshot. Source and target may match.
pub fn compact(source: &Path, target: &Path) -> Result<()> {
    let mut writer = Writer::open(source)?;
    let snapshot = writer.snapshot();
    let directory = target.parent().unwrap_or(Path::new("."));
    let temporary = rovar_storage::tempfile::NamedTempFile::new_in(directory)?.into_temp_path();
    let mut output = Writer::create(&temporary)?;
    for (key, block) in snapshot.entries() {
        output.put(
            key,
            &block.kind,
            snapshot.block(key)?.reader(),
            Some(block.hash),
        )?;
    }
    output.commit()?;
    drop(output);
    // Keep the source write lock through replacement.
    rovar_storage::fs::rename(&temporary, target)?;
    sync_parent(target)?;
    writer.finish();
    Ok(())
}

#[cfg(test)]
mod tests;
