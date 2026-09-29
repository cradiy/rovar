use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use web_time::SystemTime;

#[derive(Default)]
struct Store {
    files: BTreeMap<PathBuf, Arc<Mutex<Node>>>,
    directories: BTreeSet<PathBuf>,
    changes: BTreeMap<PathBuf, u64>,
    revision: u64,
}
struct Node {
    bytes: Vec<u8>,
    path: Option<PathBuf>,
    modified: SystemTime,
    locked: bool,
}
static STORE: LazyLock<Mutex<Store>> = LazyLock::new(Default::default);
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
fn path(path: &Path) -> PathBuf {
    let mut result = PathBuf::from("/");
    for part in path.components() {
        match part {
            Component::Normal(part) => result.push(part),
            Component::ParentDir => {
                result.pop();
            }
            _ => {}
        }
    }
    result
}
fn changed(store: &mut Store, path: &Path) {
    if path.starts_with("/workspace") {
        store.revision += 1;
        store.changes.insert(path.to_owned(), store.revision);
    }
}
fn directories(store: &mut Store, path: &Path) {
    for ancestor in path.ancestors() {
        store.directories.insert(ancestor.to_owned());
    }
}

/// Restore once before starting GPUI. Temporary files are never persisted.
pub fn restore(entries: Vec<(String, Vec<u8>)>) {
    let mut store = STORE.lock().unwrap();
    *store = Store::default();
    directories(&mut store, Path::new("/workspace"));
    directories(&mut store, Path::new("/tmp"));
    for (name, bytes) in entries {
        let name = path(Path::new(&name));
        if !name.starts_with("/workspace") {
            continue;
        }
        directories(&mut store, name.parent().unwrap());
        store.files.insert(
            name.clone(),
            Arc::new(Mutex::new(Node {
                bytes,
                path: Some(name),
                modified: SystemTime::now(),
                locked: false,
            })),
        );
    }
}
pub type Change = (String, Option<Vec<u8>>);
pub fn has_pending() -> bool {
    !STORE.lock().unwrap().changes.is_empty()
}
/// Snapshot only changed durable files, including tombstones.
pub fn pending() -> (u64, Vec<Change>) {
    let store = STORE.lock().unwrap();
    (
        store.revision,
        store
            .changes
            .keys()
            .map(|name| {
                (
                    name.to_string_lossy().into_owned(),
                    store
                        .files
                        .get(name)
                        .map(|n| n.lock().unwrap().bytes.clone()),
                )
            })
            .collect(),
    )
}
pub fn acknowledge(revision: u64) {
    STORE
        .lock()
        .unwrap()
        .changes
        .retain(|_, changed| *changed > revision);
}

pub mod fs {
    use super::*;
    struct Lease {
        node: Arc<Mutex<Node>>,
        held: AtomicBool,
    }
    impl Drop for Lease {
        fn drop(&mut self) {
            if self.held.load(Ordering::Acquire) {
                self.node.lock().unwrap().locked = false;
            }
        }
    }
    pub struct File {
        node: Arc<Mutex<Node>>,
        position: u64,
        writable: bool,
        lease: Arc<Lease>,
    }
    impl File {
        pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
            OpenOptions::new().read(true).open(path)
        }
        pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
        }
        pub fn try_clone(&self) -> io::Result<Self> {
            Ok(Self {
                node: self.node.clone(),
                position: self.position,
                writable: self.writable,
                lease: self.lease.clone(),
            })
        }
        pub fn metadata(&self) -> io::Result<Metadata> {
            let node = self.node.lock().unwrap();
            Ok(Metadata {
                len: node.bytes.len() as u64,
                modified: node.modified,
                directory: false,
            })
        }
        pub fn try_lock(&self) -> io::Result<()> {
            let mut node = self.node.lock().unwrap();
            if node.locked {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            node.locked = true;
            self.lease.held.store(true, Ordering::Release);
            Ok(())
        }
        pub fn unlock(&self) -> io::Result<()> {
            if self.lease.held.swap(false, Ordering::AcqRel) {
                self.node.lock().unwrap().locked = false;
            }
            Ok(())
        }
        fn mutate(&self, edit: impl FnOnce(&mut Vec<u8>) -> io::Result<()>) -> io::Result<()> {
            if !self.writable {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            // Namespace then node is the lock order everywhere, including snapshots.
            let mut store = STORE.lock().unwrap();
            let mut node = self.node.lock().unwrap();
            edit(&mut node.bytes)?;
            node.modified = SystemTime::now();
            if let Some(path) = &node.path {
                changed(&mut store, path);
            }
            Ok(())
        }
        pub fn set_len(&self, len: u64) -> io::Result<()> {
            let len = usize::try_from(len).map_err(|_| io::ErrorKind::InvalidInput)?;
            self.mutate(|bytes| {
                bytes.resize(len, 0);
                Ok(())
            })
        }
        pub fn read_at(&self, output: &mut [u8], offset: u64) -> io::Result<usize> {
            let node = self.node.lock().unwrap();
            let start = usize::try_from(offset)
                .map_err(|_| io::ErrorKind::InvalidInput)?
                .min(node.bytes.len());
            let count = output.len().min(node.bytes.len() - start);
            output[..count].copy_from_slice(&node.bytes[start..start + count]);
            Ok(count)
        }
        pub fn write_all_at(&self, input: &[u8], offset: u64) -> io::Result<()> {
            let start = usize::try_from(offset).map_err(|_| io::ErrorKind::InvalidInput)?;
            let end = start
                .checked_add(input.len())
                .ok_or(io::ErrorKind::InvalidInput)?;
            self.mutate(|bytes| {
                if end > bytes.len() {
                    bytes.resize(end, 0);
                }
                bytes[start..end].copy_from_slice(input);
                Ok(())
            })
        }
        /// Publishes to the in-memory workspace; IndexedDB durability is reported
        /// separately by the browser save indicator after transaction completion.
        pub fn sync_all(&self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Read for File {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let count = self.read_at(output, self.position)?;
            self.position += count as u64;
            Ok(count)
        }
    }
    impl Write for File {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            self.write_all_at(input, self.position)?;
            self.position += input.len() as u64;
            Ok(input.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Seek for File {
        fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
            let position = match from {
                SeekFrom::Start(n) => i128::from(n),
                SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
                SeekFrom::End(n) => i128::from(self.metadata()?.len()) + i128::from(n),
            };
            self.position = u64::try_from(position).map_err(|_| io::ErrorKind::InvalidInput)?;
            Ok(self.position)
        }
    }
    #[derive(Default)]
    pub struct OpenOptions {
        write: bool,
        create: bool,
        truncate: bool,
        create_new: bool,
    }
    impl OpenOptions {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn read(&mut self, _: bool) -> &mut Self {
            self
        }
        pub fn write(&mut self, v: bool) -> &mut Self {
            self.write = v;
            self
        }
        pub fn create(&mut self, v: bool) -> &mut Self {
            self.create = v;
            self
        }
        pub fn truncate(&mut self, v: bool) -> &mut Self {
            self.truncate = v;
            self
        }
        pub fn create_new(&mut self, v: bool) -> &mut Self {
            self.create_new = v;
            self
        }
        pub fn open(&self, name: impl AsRef<Path>) -> io::Result<File> {
            let name = path(name.as_ref());
            let mut store = STORE.lock().unwrap();
            if store.directories.contains(&name) {
                return Err(io::ErrorKind::IsADirectory.into());
            }
            if self.create_new && store.files.contains_key(&name) {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            if !store.files.contains_key(&name) {
                if !self.create && !self.create_new {
                    return Err(io::ErrorKind::NotFound.into());
                }
                if !store.directories.contains(name.parent().unwrap()) {
                    return Err(io::ErrorKind::NotFound.into());
                }
                store.files.insert(
                    name.clone(),
                    Arc::new(Mutex::new(Node {
                        bytes: Vec::new(),
                        path: Some(name.clone()),
                        modified: SystemTime::now(),
                        locked: false,
                    })),
                );
                changed(&mut store, &name);
            }
            let node = store.files[&name].clone();
            if self.truncate {
                if !self.write {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                node.lock().unwrap().bytes.clear();
                changed(&mut store, &name);
            }
            Ok(File {
                node: node.clone(),
                position: 0,
                writable: self.write,
                lease: Arc::new(Lease {
                    node,
                    held: AtomicBool::new(false),
                }),
            })
        }
    }
    pub struct Metadata {
        len: u64,
        modified: SystemTime,
        directory: bool,
    }
    #[allow(clippy::len_without_is_empty)] // Match std::fs::Metadata on both targets.
    impl Metadata {
        pub fn len(&self) -> u64 {
            self.len
        }
        pub fn is_file(&self) -> bool {
            !self.directory
        }
        pub fn is_dir(&self) -> bool {
            self.directory
        }
        pub fn modified(&self) -> io::Result<SystemTime> {
            Ok(self.modified)
        }
        pub fn created(&self) -> io::Result<SystemTime> {
            Ok(self.modified)
        }
    }
    pub fn metadata(name: impl AsRef<Path>) -> io::Result<Metadata> {
        let name = path(name.as_ref());
        let store = STORE.lock().unwrap();
        if store.directories.contains(&name) {
            return Ok(Metadata {
                len: 0,
                modified: SystemTime::now(),
                directory: true,
            });
        }
        let node = store
            .files
            .get(&name)
            .ok_or(io::ErrorKind::NotFound)?
            .lock()
            .unwrap();
        Ok(Metadata {
            len: node.bytes.len() as u64,
            modified: node.modified,
            directory: false,
        })
    }
    pub fn read(name: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let mut b = Vec::new();
        File::open(name)?.read_to_end(&mut b)?;
        Ok(b)
    }
    pub fn read_to_string(name: impl AsRef<Path>) -> io::Result<String> {
        String::from_utf8(read(name)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    pub fn write(name: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
        File::create(name)?.write_all(bytes.as_ref())
    }
    pub fn create_dir_all(name: impl AsRef<Path>) -> io::Result<()> {
        let name = path(name.as_ref());
        let mut store = STORE.lock().unwrap();
        if name.ancestors().any(|p| store.files.contains_key(p)) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        directories(&mut store, &name);
        Ok(())
    }
    pub fn canonicalize(name: impl AsRef<Path>) -> io::Result<PathBuf> {
        metadata(&name)?;
        Ok(path(name.as_ref()))
    }
    pub fn remove_file(name: impl AsRef<Path>) -> io::Result<()> {
        let name = path(name.as_ref());
        let mut store = STORE.lock().unwrap();
        let node = store.files.remove(&name).ok_or(io::ErrorKind::NotFound)?;
        node.lock().unwrap().path = None;
        changed(&mut store, &name);
        Ok(())
    }
    pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
        let from = path(from.as_ref());
        let to = path(to.as_ref());
        let mut store = STORE.lock().unwrap();
        if !store.directories.contains(to.parent().unwrap()) {
            return Err(io::ErrorKind::NotFound.into());
        }
        if store.directories.contains(&to) {
            return Err(io::ErrorKind::IsADirectory.into());
        }
        let node = store.files.remove(&from).ok_or(io::ErrorKind::NotFound)?;
        node.lock().unwrap().path = Some(to.clone());
        if let Some(old) = store.files.insert(to.clone(), node) {
            old.lock().unwrap().path = None;
        }
        changed(&mut store, &from);
        changed(&mut store, &to);
        Ok(())
    }
    pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<u64> {
        let b = read(from)?;
        write(to, &b)?;
        Ok(b.len() as u64)
    }
    pub struct DirEntry(PathBuf);
    impl DirEntry {
        pub fn path(&self) -> PathBuf {
            self.0.clone()
        }
        pub fn file_name(&self) -> std::ffi::OsString {
            self.0.file_name().unwrap().to_owned()
        }
        pub fn metadata(&self) -> io::Result<Metadata> {
            metadata(&self.0)
        }
        pub fn file_type(&self) -> io::Result<Metadata> {
            self.metadata()
        }
    }
    pub fn read_dir(
        name: impl AsRef<Path>,
    ) -> io::Result<std::vec::IntoIter<io::Result<DirEntry>>> {
        let name = path(name.as_ref());
        let store = STORE.lock().unwrap();
        if !store.directories.contains(&name) {
            return Err(io::ErrorKind::NotFound.into());
        }
        let paths: BTreeSet<_> = store
            .files
            .keys()
            .chain(store.directories.iter())
            .filter(|p| p.parent() == Some(name.as_path()))
            .cloned()
            .collect();
        Ok(paths
            .into_iter()
            .map(|p| Ok(DirEntry(p)))
            .collect::<Vec<_>>()
            .into_iter())
    }
}

pub mod tempfile {
    use super::*;
    pub struct TempPath(Option<PathBuf>);
    impl TempPath {
        pub fn persist_noclobber(
            mut self,
            target: impl AsRef<Path>,
        ) -> Result<(), PathPersistError> {
            if fs::metadata(&target).is_ok() {
                return Err(PathPersistError {
                    error: io::ErrorKind::AlreadyExists.into(),
                });
            }
            fs::rename(&self, target).map_err(|error| PathPersistError { error })?;
            self.0 = None;
            Ok(())
        }
    }
    impl std::ops::Deref for TempPath {
        type Target = Path;
        fn deref(&self) -> &Path {
            self.0.as_deref().unwrap()
        }
    }
    impl AsRef<Path> for TempPath {
        fn as_ref(&self) -> &Path {
            self
        }
    }
    impl Drop for TempPath {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = fs::remove_file(path);
            }
        }
    }
    pub struct NamedTempFile {
        file: fs::File,
        path: TempPath,
    }
    impl NamedTempFile {
        pub fn new() -> io::Result<Self> {
            Builder::new().tempfile()
        }
        pub fn new_in(dir: impl AsRef<Path>) -> io::Result<Self> {
            Builder::new().tempfile_in(dir)
        }
        pub fn as_file(&self) -> &fs::File {
            &self.file
        }
        pub fn path(&self) -> &Path {
            &self.path
        }
        pub fn into_temp_path(self) -> TempPath {
            self.path
        }
        pub fn persist(mut self, target: impl AsRef<Path>) -> Result<fs::File, PersistError> {
            if let Err(error) = fs::rename(self.path(), target) {
                return Err(PersistError { error, file: self });
            }
            self.path.0 = None;
            Ok(self.file)
        }
        pub fn persist_noclobber(self, target: impl AsRef<Path>) -> Result<fs::File, PersistError> {
            if fs::metadata(&target).is_ok() {
                return Err(PersistError {
                    error: io::ErrorKind::AlreadyExists.into(),
                    file: self,
                });
            }
            self.persist(target)
        }
    }
    impl Write for NamedTempFile {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.file.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }
    pub struct PersistError {
        pub error: io::Error,
        pub file: NamedTempFile,
    }
    impl std::fmt::Debug for PersistError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.error.fmt(f)
        }
    }
    impl std::fmt::Display for PersistError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.error.fmt(f)
        }
    }
    impl std::error::Error for PersistError {}
    #[derive(Debug)]
    pub struct PathPersistError {
        pub error: io::Error,
    }
    impl std::fmt::Display for PathPersistError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.error.fmt(f)
        }
    }
    impl std::error::Error for PathPersistError {}
    #[derive(Default)]
    pub struct Builder {
        suffix: String,
    }
    impl Builder {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn suffix(&mut self, suffix: &str) -> &mut Self {
            self.suffix = suffix.into();
            self
        }
        pub fn prefix(&mut self, _: &str) -> &mut Self {
            self
        }
        pub fn tempfile(&self) -> io::Result<NamedTempFile> {
            fs::create_dir_all("/tmp")?;
            self.tempfile_in("/tmp")
        }
        pub fn tempfile_in(&self, dir: impl AsRef<Path>) -> io::Result<NamedTempFile> {
            let name = dir.as_ref().join(format!(
                ".rovar-{}{}",
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed),
                self.suffix
            ));
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&name)?;
            Ok(NamedTempFile {
                file,
                path: TempPath(Some(name)),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_preserves_reader_snapshot_and_ack_does_not_drop_later_edits() {
        restore(vec![]);
        fs::write("/workspace/a", b"original").unwrap();
        let old = fs::File::open("/workspace/a").unwrap();
        let (revision, _) = pending();
        fs::write("/tmp/b", b"replacement").unwrap();
        fs::rename("/tmp/b", "/workspace/a").unwrap();
        acknowledge(revision);
        let (_, changes) = pending();
        assert_eq!(
            changes,
            vec![("/workspace/a".into(), Some(b"replacement".to_vec()))]
        );
        let mut bytes = [0; 8];
        old.read_at(&mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"original");
        let writer = fs::File::open("/workspace/a").unwrap();
        writer.try_lock().unwrap();
        assert!(fs::File::open("/workspace/a").unwrap().try_lock().is_err());
        drop(writer);
        assert!(fs::File::open("/workspace/a").unwrap().try_lock().is_ok());
        fs::remove_file("/workspace/a").unwrap();
        assert_eq!(pending().1, vec![("/workspace/a".into(), None)]);
    }
}
