//! Parallel directory walking over [`Fs`]: gitignore-aware, depth-bounded, and
//! cancellation-friendly.
//!
//! This is the provider-free core of pi-walker's `walk`/`collect`:
//!
//! - [`walk`] lists directories on tokio tasks (a permit pool caps concurrency),
//!   visits entries in no particular order, and streams one [`Batch`] per
//!   non-empty directory. Dropping the [`Walk`] stops the walk.
//! - [`collect`] keeps the entries a filter accepts and returns them, sorted by
//!   path when asked.
//!
//! Traversal honours hidden / `.git` / `node_modules` pruning, [`ignore`] rules,
//! the glob selector's pruning queries, `follow_links` with a cycle guard, and
//! the caller's cancellation token.

use std::{
    borrow::Cow,
    collections::HashSet,
    fmt, io,
    marker::PhantomData,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex,
    },
    task::{Context, Poll},
};

use futures::Stream;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::task::TaskTracker;

use super::{
    fs::{path_str, Detail, FileType, Fs, Listing},
    ignore::{DirIgnores, Frame, Ignorer, Names},
};

/// A caller's handle on one walk's stop flag.
#[derive(Clone, Debug, Default)]
pub struct Stop(Arc<AtomicBool>);

impl PartialEq for Stop {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Stop {}

impl Stop {
    /// A flag that is not set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ends the walk holding this flag at its next directory or record.
    pub fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether the walk has ended.
    pub fn stopped(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Traversal options shared by [`walk`] and [`collect`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Include dot-prefixed entries.
    pub hidden: bool,
    /// Honour `.ignore`, `.gitignore`, `.git/info/exclude`, and git's global
    /// excludes.
    pub gitignore: bool,
    /// Prune entries named `.git`.
    pub skip_git: bool,
    /// Prune entries named `node_modules`.
    pub skip_node_modules: bool,
    /// Report symlinks as their targets and descend into linked directories.
    pub follow_links: bool,
    /// Listing metadata; [`Detail::Full`] is needed for sizes and mtimes.
    pub detail: Detail,
    /// Deepest entry depth reported (root's children are depth 1).
    pub max_depth: Option<usize>,
    /// Kept for API parity with pi-walker; the listing cache is not ported.
    pub same_file_system: bool,
    /// Directories listed at once; `None` picks `PI_WALK_WORKERS` or the
    /// environment's parallelism.
    pub workers: Option<usize>,
    /// Kept for API parity; the listing cache is not ported.
    pub cache: bool,
    /// The flag this walk ends on.
    pub stop: Option<Stop>,
}

impl Options {
    /// Directories a walk lists at once.
    pub fn parallelism(&self, fs: &Fs) -> usize {
        static ENV: LazyLock<Option<usize>> = LazyLock::new(|| {
            std::env::var("PI_WALK_WORKERS")
                .ok()
                .and_then(|value| value.trim().parse::<usize>().ok())
                .filter(|&workers| workers > 0)
        });
        static CPUS: LazyLock<usize> = LazyLock::new(|| {
            std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
        });
        let workers = self
            .workers
            .filter(|&workers| workers > 0)
            .or(*ENV)
            .unwrap_or_else(|| fs.workers());
        workers.clamp(1, (*CPUS).max(1))
    }
}

/// One walked entry. Paths are relative to the walk root, `/`-separated bytes.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    /// Walk-relative path.
    pub path: &'a [u8],
    /// Entry kind (a followed symlink's target kind).
    pub file_type: FileType,
    /// 1 for the root's children.
    pub depth: usize,
    /// Regular-file size, when known.
    pub size: Option<u64>,
    /// Modification time in Unix nanoseconds, when known.
    pub mtime: Option<i64>,
}

impl<'a> Entry<'a> {
    /// Final path component.
    pub fn name(&self) -> &'a [u8] {
        match memchr::memrchr(b'/', self.path) {
            Some(slash) => &self.path[slash + 1..],
            None => self.path,
        }
    }

    /// The path as text, with invalid UTF-8 replaced.
    pub fn path_lossy(&self) -> Cow<'a, str> {
        String::from_utf8_lossy(self.path)
    }

    /// The walk-relative path as a [`Path`] (`/`-separated).
    pub fn rel_path(&self) -> &'a Path {
        rel_path(self.path)
    }
}

/// How a visitor or filter steers the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Keep the entry; directories are descended.
    Continue,
    /// Leave the entry out of a [`collect`] but still descend into it.
    Skip,
    /// Leave the entry out and do not descend into it.
    SkipDir,
    /// End the walk as soon as in-flight directories finish.
    Stop,
}

/// A directory below the root that could not be opened or listed.
#[derive(Debug)]
pub struct DirError {
    /// Absolute path in the walked filesystem's spelling.
    pub path: PathBuf,
    /// What failed.
    pub error: io::Error,
}

/// Why a walk ended without a result.
#[derive(Debug)]
pub enum WalkError {
    /// The root could not be opened as a directory.
    Root {
        /// The root as given.
        path: PathBuf,
        /// What failed.
        error: io::Error,
    },
}

impl fmt::Display for WalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for WalkError {}

/// The walking task, as seen by visitors and filters: path math only.
pub struct Worker<'w> {
    fs: &'w Fs,
    root: &'w Path,
    abs: PathBuf,
}

impl Worker<'_> {
    /// Absolute path of `entry`; valid until the next call.
    pub fn abs(&mut self, entry: &Entry<'_>) -> &Path {
        self.abs_rel(entry.path)
    }

    /// Absolute path of the walk-relative `rel`; valid until the next call.
    pub fn abs_rel(&mut self, rel: &[u8]) -> &Path {
        self.abs = self.fs.join(self.root, rel);
        self.abs.as_path()
    }
}

/// One directory's outputs.
#[derive(Debug)]
pub struct Batch<T> {
    /// What the visitor pushed for the directory; never empty.
    pub items: Vec<T>,
}

/// Output batches fitting the channel of a [`walk`] per worker.
const BATCHES_PER_WORKER: usize = 4;

struct Ctx<T, V> {
    fs: Fs,
    opts: Options,
    root: PathBuf,
    visit: V,
    tx: mpsc::Sender<Batch<T>>,
    halt: Stop,
    errors: Arc<Mutex<Vec<DirError>>>,
    tracker: TaskTracker,
    permits: Arc<Semaphore>,
    ignorer: Arc<Ignorer>,
    seen: Mutex<HashSet<PathBuf>>,
    _marker: PhantomData<fn() -> T>,
}

/// Starts a walk of `root`, visiting every entry below it that `opts` admits.
///
/// # Errors
/// [`WalkError::Root`] when `root` cannot be opened as a directory.
pub async fn walk<T, V>(
    fs: &Fs,
    root: &Path,
    opts: &Options,
    visit: V,
) -> Result<Walk<T>, WalkError>
where
    T: Send + 'static,
    V: Fn(&mut Worker<'_>, &Entry<'_>, &mut Vec<T>) -> Action + Send + Sync + 'static,
{
    // Resolve through the environment, not `std::path::absolute`: in-memory
    // environments use their own root scheme (e.g. `/tmp/work` on Windows).
    let root_abs = fs
        .env()
        .absolute_path(&path_str(root), fs.cancel())
        .await
        .map_err(|error| WalkError::Root {
            path: root.to_path_buf(),
            error: io::Error::other(error.to_string()),
        })?;
    let mut session = fs.session();
    let dir = session
        .open_dir(&root_abs)
        .await
        .map_err(|error| WalkError::Root {
            path: root.to_path_buf(),
            error,
        })?;
    let listing = session
        .list(&dir, opts.detail)
        .await
        .map_err(|error| WalkError::Root {
            path: root.to_path_buf(),
            error,
        })?;
    let (ignorer, root_frame) = if opts.gitignore {
        Ignorer::new(&mut session, fs, &root_abs).await
    } else {
        (Ignorer::empty(), None)
    };

    let workers = opts.parallelism(fs);
    let (tx, rx) = mpsc::channel(workers * BATCHES_PER_WORKER);
    let halt = opts.stop.clone().unwrap_or_default();
    let tracker = TaskTracker::new();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let ctx = Arc::new(Ctx {
        fs: fs.clone(),
        opts: opts.clone(),
        root: root_abs.clone(),
        visit,
        tx,
        halt: halt.clone(),
        errors: Arc::clone(&errors),
        tracker: tracker.clone(),
        permits: Arc::new(Semaphore::new(workers)),
        ignorer: Arc::new(ignorer),
        seen: Mutex::new(HashSet::from([root_abs.clone()])),
        _marker: PhantomData,
    });
    spawn_dir(
        Arc::clone(&ctx),
        Vec::new(),
        root_abs,
        0,
        root_frame,
        Some(listing),
    );
    tracker.close();
    drop(ctx);
    Ok(Walk {
        rx,
        errors,
        _halt: halt,
    })
}

fn spawn_dir<T, V>(
    ctx: Arc<Ctx<T, V>>,
    rel: Vec<u8>,
    abs: PathBuf,
    depth: usize,
    frame: Option<Arc<Frame>>,
    listing: Option<Listing>,
) where
    T: Send + 'static,
    V: Fn(&mut Worker<'_>, &Entry<'_>, &mut Vec<T>) -> Action + Send + Sync + 'static,
{
    let tracker = ctx.tracker.clone();
    tracker.spawn(run_dir(ctx, rel, abs, depth, frame, listing));
}

async fn run_dir<T, V>(
    ctx: Arc<Ctx<T, V>>,
    rel: Vec<u8>,
    abs: PathBuf,
    depth: usize,
    frame: Option<Arc<Frame>>,
    listing: Option<Listing>,
) where
    T: Send + 'static,
    V: Fn(&mut Worker<'_>, &Entry<'_>, &mut Vec<T>) -> Action + Send + Sync + 'static,
{
    if ctx.halt.stopped() || ctx.fs.cancelled() {
        return;
    }
    let Ok(permit) = Arc::clone(&ctx.permits).acquire_owned().await else {
        return;
    };
    let listing = match listing {
        Some(listing) => listing,
        None => match list_dir(&ctx.fs, &abs, ctx.opts.detail).await {
            Ok(listing) => listing,
            Err(error) => {
                ctx.errors
                    .lock()
                    .unwrap()
                    .push(DirError { path: abs, error });
                return;
            }
        },
    };

    let mut names = Names::default();
    for record in listing.records() {
        names.record(&record.name, record.file_type);
    }
    let mut session = ctx.fs.session();
    let dir = session
        .open_dir(&abs)
        .await
        .unwrap_or_else(|_| unreachable!("open_dir is infallible"));
    let dir_ignores = if ctx.opts.gitignore && !names.is_empty() {
        DirIgnores::load(&mut session, &dir, names).await
    } else {
        DirIgnores::default()
    };
    let frame = dir_ignores.frame(frame.as_ref(), names, &rel);

    let mut items = Vec::new();
    let mut children: Vec<(Vec<u8>, PathBuf, Option<Arc<Frame>>)> = Vec::new();
    let mut scratch = Vec::new();
    let mut worker = Worker {
        fs: &ctx.fs,
        root: &ctx.root,
        abs: PathBuf::new(),
    };

    for record in listing.records() {
        if ctx.halt.stopped() || ctx.fs.cancelled() {
            break;
        }
        let name = &record.name[..];
        if pruned(
            name,
            ctx.opts.hidden,
            ctx.opts.skip_git,
            ctx.opts.skip_node_modules,
        ) {
            continue;
        }
        let child_depth = depth + 1;
        if let Some(max) = ctx.opts.max_depth {
            if child_depth > max {
                continue;
            }
        }
        let mut child_rel = Vec::with_capacity(rel.len() + 1 + name.len());
        if !rel.is_empty() {
            child_rel.extend_from_slice(&rel);
            child_rel.push(b'/');
        }
        child_rel.extend_from_slice(name);

        let mut file_type = record.file_type;
        let mut size = record.size;
        let mut mtime = record.mtime;
        let mut child_abs = abs.join(super::fs::os_name(name));
        if file_type == FileType::Symlink && ctx.opts.follow_links {
            let Ok(info) = session.stat(&child_abs, true).await else {
                continue;
            };
            file_type = info.file_type.unwrap_or(FileType::Symlink);
            size = (file_type == FileType::File).then_some(info.size);
            mtime = Some(info.mtime);
            if file_type == FileType::Dir {
                let Ok(canonical) = ctx
                    .fs
                    .env()
                    .canonical_path(&path_str(&child_abs), ctx.fs.cancel())
                    .await
                else {
                    continue;
                };
                let mut seen = ctx.seen.lock().unwrap();
                if !seen.insert(canonical.clone()) {
                    continue;
                }
                drop(seen);
                child_abs = canonical;
            }
        }

        if ctx.opts.gitignore
            && ctx.ignorer.ignored(
                frame.as_deref(),
                &child_rel,
                file_type == FileType::Dir,
                &mut scratch,
            )
        {
            continue;
        }

        let entry = Entry {
            path: &child_rel,
            file_type,
            depth: child_depth,
            size,
            mtime,
        };
        let action = (ctx.visit)(&mut worker, &entry, &mut items);
        if action == Action::Stop {
            ctx.halt.stop();
            break;
        }
        if file_type == FileType::Dir && action != Action::SkipDir && !ctx.halt.stopped() {
            children.push((child_rel, child_abs, frame.clone()));
        }
    }

    drop(permit);
    if !items.is_empty() && ctx.tx.send(Batch { items }).await.is_err() {
        ctx.halt.stop();
        return;
    }
    for (child_rel, child_abs, child_frame) in children {
        spawn_dir(
            Arc::clone(&ctx),
            child_rel,
            child_abs,
            depth + 1,
            child_frame,
            None,
        );
    }
}

async fn list_dir(fs: &Fs, abs: &Path, detail: Detail) -> io::Result<Listing> {
    let mut session = fs.session();
    let dir = session.open_dir(abs).await?;
    session.list(&dir, detail).await
}

/// A running [`walk`]: per-directory output batches in no particular order.
pub struct Walk<T> {
    rx: mpsc::Receiver<Batch<T>>,
    errors: Arc<Mutex<Vec<DirError>>>,
    _halt: Stop,
}

impl<T> Stream for Walk<T> {
    type Item = Batch<T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Batch<T>>> {
        self.rx.poll_recv(cx)
    }
}

impl<T> Walk<T> {
    /// Directories below the root that could not be read; complete once the
    /// stream has ended.
    pub fn into_errors(self) -> Vec<DirError> {
        std::mem::take(&mut *self.errors.lock().unwrap())
    }
}

/// One kept entry, owning its path.
#[derive(Debug)]
struct Stored {
    path: Box<[u8]>,
    file_type: FileType,
    depth: usize,
    size: Option<u64>,
    mtime: Option<i64>,
}

/// Entries kept by a [`collect`].
#[derive(Debug, Default)]
pub struct Collection {
    entries: Vec<Stored>,
    errors: Vec<DirError>,
}

impl Collection {
    /// Number of kept entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Nothing was kept.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The `index`th entry.
    pub fn get(&self, index: usize) -> Entry<'_> {
        let stored = &self.entries[index];
        Entry {
            path: &stored.path,
            file_type: stored.file_type,
            depth: stored.depth,
            size: stored.size,
            mtime: stored.mtime,
        }
    }

    /// Kept entries in collection order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Entry<'_>> + '_ {
        (0..self.len()).map(|index| self.get(index))
    }

    /// Directories below the root that could not be read.
    pub fn errors(&self) -> &[DirError] {
        &self.errors
    }
}

/// Collects the entries below `root` that `filter` keeps.
///
/// `filter` returns [`Action::Continue`] to keep an entry; `Skip` drops an
/// entry, `SkipDir` drops a directory with its subtree, `Stop` ends the walk
/// keeping what was collected. With `sorted`, entries come back in byte order
/// of their paths.
pub async fn collect<F>(
    fs: &Fs,
    root: &Path,
    opts: &Options,
    sorted: bool,
    filter: F,
) -> Result<Collection, WalkError>
where
    F: Fn(&mut Worker<'_>, &mut Entry<'_>) -> Action + Send + Sync + 'static,
{
    let filter = Arc::new(filter);
    let mut walk = walk(
        fs,
        root,
        opts,
        move |worker, entry, out: &mut Vec<Stored>| {
            let mut entry = *entry;
            let action = filter(worker, &mut entry);
            if action == Action::Continue {
                out.push(Stored {
                    path: entry.path.to_vec().into_boxed_slice(),
                    file_type: entry.file_type,
                    depth: entry.depth,
                    size: entry.size,
                    mtime: entry.mtime,
                });
            }
            action
        },
    )
    .await?;
    let mut entries = Vec::new();
    while let Some(batch) = walk.next().await {
        entries.extend(batch.items);
    }
    let errors = walk.into_errors();
    if sorted {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
    }
    Ok(Collection { entries, errors })
}

use futures::StreamExt;

/// A walk-relative path as a [`Path`].
pub(crate) fn rel_path(bytes: &[u8]) -> &Path {
    Path::new(std::str::from_utf8(bytes).unwrap_or_default())
}

/// Whether `name` is pruned by the hidden, `.git`, and `node_modules` rules.
pub(crate) fn pruned(name: &[u8], hidden: bool, skip_git: bool, skip_node_modules: bool) -> bool {
    (!hidden && name.first() == Some(&b'.'))
        || (skip_git && name == b".git")
        || (skip_node_modules && name == b"node_modules")
}
