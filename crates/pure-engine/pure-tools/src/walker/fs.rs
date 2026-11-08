//! Filesystem layer for the walker, over the crate's [`ExecutionEnv`] trait.
//!
//! This is the provider-free replacement for `pi-walker/src/fs`: walks, ignore
//! loading, and glob resolution read only through [`crate::env::FileSystem`],
//! so they work against `OsExecutionEnv` and `InMemoryExecutionEnv` alike.
//! Directories are plain paths (no directory handles), listings are owned
//! records, and files are read whole on open — `read_into`/`read` then serve
//! slices of that buffer.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio_util::sync::CancellationToken;

use crate::env::ExecutionEnv;

/// Kind of a listed entry. Mirrors pi-walker's `FileType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileType {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link (never followed by a listing).
    Symlink,
}

impl FileType {
    fn from_kind(kind: crate::env::FileKind) -> Self {
        match kind {
            crate::env::FileKind::File => Self::File,
            crate::env::FileKind::Directory => Self::Dir,
            crate::env::FileKind::Symlink => Self::Symlink,
        }
    }
}

/// How much metadata listings collect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Detail {
    /// Names and entry types only.
    #[default]
    Minimal,
    /// Also sizes and modification times.
    Full,
}

/// Metadata of a file opened by [`Session::open_file`] or found by
/// [`Session::stat`].
#[derive(Clone, Copy, Debug)]
pub struct FileInfo {
    /// `None` for special files.
    pub file_type: Option<FileType>,
    /// Byte size.
    pub size: u64,
    /// Modification time, Unix nanoseconds.
    pub mtime: i64,
}

/// One listing entry. Names are UTF-8 bytes from the environment's
/// [`crate::env::FileInfo::name`].
#[derive(Clone, Debug)]
pub struct Record {
    /// Entry name, one path component.
    pub name: Box<[u8]>,
    /// Entry kind as listed (symlinks unresolved).
    pub file_type: FileType,
    /// Regular-file size, when the listing collected it.
    pub size: Option<u64>,
    /// Modification time in Unix nanoseconds, when collected.
    pub mtime: Option<i64>,
}

/// One directory's entries, without `.` and `..`.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    records: Vec<Record>,
}

impl Listing {
    /// Entries in listing order.
    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// The name of `record`.
    pub fn name<'a>(&self, record: &'a Record) -> &'a [u8] {
        &record.name
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// The directory has no listed entries.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// An open directory, identified by path.
#[derive(Clone, Debug)]
pub struct Dir {
    path: PathBuf,
}

impl Dir {
    /// Path of this directory.
    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

/// An open regular file: the environment reads it whole on open.
#[derive(Debug)]
pub struct File {
    data: Vec<u8>,
    pos: usize,
    size: u64,
}

impl File {
    /// Size reported when the file was opened.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Replaces `buf`'s contents with up to `limit` leading bytes.
    pub async fn read_into(&mut self, buf: &mut Vec<u8>, limit: u64) -> io::Result<()> {
        buf.clear();
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let end = self.data.len().min(limit);
        buf.extend_from_slice(&self.data[..end]);
        self.pos = end;
        Ok(())
    }

    /// Reads the next bytes into `buf`, returning the count (0 at EOF).
    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = &self.data[self.pos..];
        let n = remaining.len().min(buf.len());
        buf[..n].copy_from_slice(&remaining[..n]);
        self.pos += n;
        Ok(n)
    }
}

/// [`Session::open_file`]'s error for directories and special files.
pub fn not_regular() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "not a regular file")
}

/// The filesystem every walk reads through. Cheap to clone.
#[derive(Clone)]
pub struct Fs {
    inner: Arc<dyn ExecutionEnv>,
    cancel: Option<CancellationToken>,
}

impl std::fmt::Debug for Fs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fs").finish_non_exhaustive()
    }
}

impl Fs {
    /// A facade over `env`.
    pub fn new(env: Arc<dyn ExecutionEnv>, cancel: Option<CancellationToken>) -> Self {
        Self { inner: env, cancel }
    }

    /// The environment behind this facade.
    pub fn env(&self) -> &dyn ExecutionEnv {
        &*self.inner
    }

    /// Cancellation checked by every directory and entry.
    pub fn cancel(&self) -> Option<&CancellationToken> {
        self.cancel.as_ref()
    }

    /// Whether `cancel` has fired.
    pub fn cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
    }

    /// A fresh session.
    pub fn session(&self) -> Session {
        Session { fs: self.clone() }
    }

    /// Directories listed at once.
    pub fn workers(&self) -> usize {
        std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .clamp(1, 8)
    }

    /// Absolute spelling of the walk-relative `rel` below `root`.
    pub fn join(&self, root: &Path, rel: &[u8]) -> PathBuf {
        let mut path = root.to_path_buf();
        if rel.is_empty() {
            return path;
        }
        for segment in rel.split(|&b| b == b'/').filter(|s| !s.is_empty()) {
            path.push(os_name(segment));
        }
        path
    }
}

/// Raw name bytes as an [`OsStr`].
pub fn os_name(bytes: &[u8]) -> std::ffi::OsString {
    std::ffi::OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

/// Path to the string spelling the environment's methods take.
pub fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn map_err(error: crate::error::FileError) -> io::Error {
    io::Error::other(error.to_string())
}

/// Per-call filesystem operations backed by [`Fs`].
pub struct Session {
    fs: Fs,
}

impl Session {
    /// The facade this session reads through.
    pub fn fs(&self) -> &Fs {
        &self.fs
    }

    /// Opens the directory at `path`.
    pub async fn open_dir(&mut self, path: &Path) -> io::Result<Dir> {
        Ok(Dir {
            path: path.to_path_buf(),
        })
    }

    /// Opens the subdirectory `name` of `parent`.
    pub async fn open_child(&mut self, parent: &Dir, name: &[u8]) -> io::Result<Dir> {
        Ok(Dir {
            path: parent.path.join(os_name(name)),
        })
    }

    /// Lists `dir`.
    pub async fn list(&mut self, dir: &Dir, detail: Detail) -> io::Result<Listing> {
        let cancel = self.fs.cancel.as_ref();
        let entries = self
            .fs
            .inner
            .list_dir(&path_str(&dir.path), cancel)
            .await
            .map_err(map_err)?;
        let mut records = Vec::with_capacity(entries.len());
        for entry in entries {
            let file_type = FileType::from_kind(entry.kind);
            let (size, mtime) = match detail {
                Detail::Minimal => (None, None),
                Detail::Full => (
                    (file_type == FileType::File).then_some(entry.size),
                    Some(entry.mtime_ms.saturating_mul(1_000_000)),
                ),
            };
            records.push(Record {
                name: entry.name.into_bytes().into_boxed_slice(),
                file_type,
                size,
                mtime,
            });
        }
        Ok(Listing { records })
    }

    /// Opens the regular file `name` in `dir`.
    pub async fn open_file_at(&mut self, dir: &Dir, name: &[u8]) -> io::Result<(File, FileInfo)> {
        self.open_file(&dir.path.join(os_name(name))).await
    }

    /// Opens the regular file at an absolute `path`.
    pub async fn open_file(&mut self, path: &Path) -> io::Result<(File, FileInfo)> {
        let info = self.stat(path, true).await?;
        if info.file_type != Some(FileType::File) {
            return Err(not_regular());
        }
        let data = self
            .fs
            .inner
            .read_binary_file(&path_str(path), self.fs.cancel.as_ref())
            .await
            .map_err(map_err)?;
        let size = data.len() as u64;
        Ok((File { data, pos: 0, size }, FileInfo { size, ..info }))
    }

    /// Describes the path, following a final symlink only when `follow`.
    pub async fn stat(&mut self, path: &Path, follow: bool) -> io::Result<FileInfo> {
        let cancel = self.fs.cancel.as_ref();
        let mut info = self
            .fs
            .inner
            .file_info(&path_str(path), cancel)
            .await
            .map_err(map_err)?;
        if follow && info.kind == crate::env::FileKind::Symlink {
            let canonical = self
                .fs
                .inner
                .canonical_path(&path_str(path), cancel)
                .await
                .map_err(map_err)?;
            info = self
                .fs
                .inner
                .file_info(&path_str(&canonical), cancel)
                .await
                .map_err(map_err)?;
        }
        Ok(FileInfo {
            file_type: Some(FileType::from_kind(info.kind)),
            size: info.size,
            mtime: info.mtime_ms.saturating_mul(1_000_000),
        })
    }
}
