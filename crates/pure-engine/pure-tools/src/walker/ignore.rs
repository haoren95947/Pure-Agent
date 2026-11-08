//! Gitignore-format rules and the ignore-file chain walks honour.
//!
//! Ported from `pi-walker/src/ignore.rs`, minus the listing-cache stamps and
//! the `pi-vfs` URL spellings: reads go through [`Session`] over this crate's
//! [`Fs`].
//!
//! - Each directory may contribute `.ignore`, `.gitignore`, and (when it has a
//!   `.git` entry) `.git/info/exclude`. For each kind the nearest directory
//!   with a matching rule decides; `.ignore` beats `.gitignore` beats the
//!   exclude file beats git's global excludes file.
//! - Exclude files above the nearest repository root (`.git` or `.jj`) are not
//!   consulted; exclude and global rules apply only inside a repository.
//!   `.gitignore` applies everywhere.
//! - Ignore files in ancestors of the walk root, up to the nearest repository
//!   root, apply too, minus any rule that would ignore the walk root itself.
//! - Within one file the last matching rule wins; `dir/` rules match only
//!   directories; `!` rules re-include.

use std::{
    borrow::Cow,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{
    fs::{Dir, File, FileInfo, FileType, Fs, Session},
    pattern::{Dialect, Glob},
};

/// Outcome of matching a path against rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    /// No rule matched.
    None,
    /// The last matching rule ignores the path.
    Ignore,
    /// The last matching rule is a `!` re-include.
    Whitelist,
}

/// Cap on how much of one ignore file is read.
const MAX_IGNORE_BYTES: u64 = 16 << 20;

/// Glob dialect of gitignore rules: an unclosed `[` is a literal.
const RULE_DIALECT: Dialect = Dialect {
    allow_unclosed_class: true,
};

#[derive(Clone, Debug)]
struct Rule {
    glob: Glob,
    dir_only: bool,
    whitelist: bool,
}

/// The compiled rules of one gitignore-format file.
#[derive(Clone, Debug, Default)]
pub struct Rules {
    rules: Vec<Rule>,
    /// Some rule needs the full path, not just the basename.
    anchored: bool,
}

impl Rules {
    /// Compiles file contents.
    pub fn parse(data: &[u8]) -> Self {
        Self::parse_filtered(data, |_| true)
    }

    fn parse_filtered(mut data: &[u8], mut keep: impl FnMut(&str) -> bool) -> Self {
        let mut out = Self::default();
        let mut first = true;
        while !data.is_empty() {
            let raw = match memchr::memchr(b'\n', data) {
                Some(i) => {
                    let line = &data[..i];
                    data = &data[i + 1..];
                    line.strip_suffix(b"\r").unwrap_or(line)
                }
                None => std::mem::take(&mut data),
            };
            let Ok(mut line) = std::str::from_utf8(raw) else {
                break;
            };
            if first {
                line = line.trim_start_matches('\u{feff}');
                first = false;
            }
            if !keep(line) {
                continue;
            }
            if let Some(rule) = parse_rule(line) {
                out.anchored |= !rule.glob.basename_only();
                out.rules.push(rule);
            }
        }
        out
    }

    /// Reads (at most 16 MiB of) and compiles the file at `path`.
    pub async fn read(fs: &Fs, path: &Path) -> io::Result<Self> {
        let mut session = fs.session();
        let opened = session.open_file(path).await;
        let mut data = Vec::new();
        read_opened(opened, &mut data).await?;
        Ok(Self::parse(&data))
    }

    /// No rule survived parsing.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Decision of the last rule matching `path`, which is relative to the
    /// rule file's directory, `/`-separated.
    pub fn matched(&self, path: &[u8], is_dir: bool) -> Match {
        let base = memchr::memrchr(b'/', path).map_or(path, |i| &path[i + 1..]);
        for rule in self.rules.iter().rev() {
            if rule.dir_only && !is_dir {
                continue;
            }
            if rule.glob.is_match_base(path, base) {
                return if rule.whitelist {
                    Match::Whitelist
                } else {
                    Match::Ignore
                };
            }
        }
        Match::None
    }

    /// [`Self::matched`] for a filesystem `path` below `base`, the directory
    /// the rules are relative to; a path outside `base` is matched whole.
    pub fn matched_below(&self, base: &Path, path: &Path, is_dir: bool) -> Match {
        let rel = path.strip_prefix(base).unwrap_or(path);
        let bytes = path_bytes(rel);
        if cfg!(windows) && bytes.contains(&b'\\') {
            let slashed: Vec<u8> = bytes
                .iter()
                .map(|&byte| if byte == b'\\' { b'/' } else { byte })
                .collect();
            return self.matched(&slashed, is_dir);
        }
        self.matched(&bytes, is_dir)
    }

    /// Like [`Self::matched`], then each ancestor of `path` as a directory,
    /// nearest first; the first decision wins.
    pub fn matched_path_or_parents(&self, mut path: &[u8], is_dir: bool) -> Match {
        let mut is_dir = is_dir;
        loop {
            let m = self.matched(path, is_dir);
            if m != Match::None || path.is_empty() {
                return m;
            }
            path = &path[..memchr::memrchr(b'/', path).unwrap_or(0)];
            is_dir = true;
        }
    }
}

/// `bytes` of a path as gitignore matching sees it (`/`-separated).
fn path_bytes(path: &Path) -> Cow<'_, [u8]> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if cfg!(windows) && bytes.contains(&b'\\') {
        return Cow::Owned(
            bytes
                .iter()
                .map(|&b| if b == b'\\' { b'/' } else { b })
                .collect(),
        );
    }
    Cow::Borrowed(bytes)
}

/// Ports `GitignoreBuilder::add_line`; `None` for comments, blank lines, and
/// invalid globs.
fn parse_rule(line: &str) -> Option<Rule> {
    if line.starts_with('#') {
        return None;
    }
    let mut line = if line.ends_with("\\ ") {
        line
    } else {
        line.trim_end()
    };
    if line.is_empty() {
        return None;
    }
    let mut whitelist = false;
    let mut absolute = false;
    if line.starts_with("\\!") || line.starts_with("\\#") {
        line = &line[1..];
    } else {
        if let Some(rest) = line.strip_prefix('!') {
            whitelist = true;
            line = rest;
        }
        if let Some(rest) = line.strip_prefix('/') {
            line = rest;
            absolute = true;
        }
    }
    let mut dir_only = false;
    if let Some(rest) = line.strip_suffix('/') {
        dir_only = true;
        line = rest.strip_suffix('\\').unwrap_or(rest);
    }
    let mut actual = if !absolute && !line.contains('/') && !line.starts_with("**/") && line != "**"
    {
        Cow::Owned(format!("**/{line}"))
    } else {
        Cow::Borrowed(line)
    };
    if actual.ends_with("/**") {
        actual.to_mut().push_str("/*");
    }
    let glob = Glob::with_dialect(&actual, RULE_DIALECT).ok()?;
    Some(Rule {
        glob,
        dir_only,
        whitelist,
    })
}

/// Which ignore-relevant names a directory listing contains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Names(u8);

const HAS_DOT_IGNORE: u8 = 1;
const HAS_GITIGNORE: u8 = 2;
const HAS_GIT_DIR: u8 = 4;
const HAS_REPO_MARKER: u8 = 8;

/// Per-directory ignore sources, in precedence order.
const SOURCES: [&str; 3] = [".ignore", ".gitignore", ".git/info/exclude"];

/// Listing names that make each source worth reading: the file itself, or a
/// `.git` entry for the exclude file.
const SOURCE_FLAGS: [u8; 3] = [HAS_DOT_IGNORE, HAS_GITIGNORE, HAS_GIT_DIR];

impl Names {
    /// Notes `name` (a listed entry of type `file_type`) if it is `.ignore`,
    /// `.gitignore`, `.git`, or `.jj`.
    pub(crate) fn record(&mut self, name: &[u8], file_type: FileType) {
        if name.len() < 3 || name[0] != b'.' {
            return;
        }
        match name {
            b".ignore" if file_type != FileType::Dir => self.0 |= HAS_DOT_IGNORE,
            b".gitignore" if file_type != FileType::Dir => self.0 |= HAS_GITIGNORE,
            b".git" => self.0 |= HAS_GIT_DIR | HAS_REPO_MARKER,
            b".jj" => self.0 |= HAS_REPO_MARKER,
            _ => {}
        }
    }

    /// The listing has none of those names.
    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    const fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

/// One directory's compiled ignore sources.
#[derive(Clone, Debug, Default)]
pub(crate) struct DirIgnores {
    files: [Option<Arc<Rules>>; 3],
}

/// Reads up to [`MAX_IGNORE_BYTES`] of an opened file into `buf`.
async fn read_opened(opened: io::Result<(File, FileInfo)>, buf: &mut Vec<u8>) -> io::Result<()> {
    let (mut file, _) = opened?;
    file.read_into(buf, MAX_IGNORE_BYTES).await
}

/// Compiled rules, or `None` when no rule survives.
fn non_empty(rules: Rules) -> Option<Arc<Rules>> {
    (!rules.is_empty()).then(|| Arc::new(rules))
}

/// Opens `.git/info/exclude` below `dir`.
async fn open_exclude(session: &mut Session, dir: &Dir) -> io::Result<(File, FileInfo)> {
    let git = session.open_child(dir, b".git").await?;
    let info = session.open_child(&git, b"info").await?;
    session.open_file_at(&info, b"exclude").await
}

impl DirIgnores {
    /// Reads the sources `names` calls for, relative to the open `dir`.
    pub(crate) async fn load(session: &mut Session, dir: &Dir, names: Names) -> Self {
        let mut out = Self::default();
        let mut buf = Vec::new();
        for (k, _) in Self::source_names(names) {
            let opened = if k < 2 {
                session.open_file_at(dir, SOURCES[k].as_bytes()).await
            } else {
                open_exclude(session, dir).await
            };
            if read_opened(opened, &mut buf).await.is_err() {
                continue;
            }
            out.files[k] = non_empty(Rules::parse(&buf));
        }
        out
    }

    /// Paths (relative to the directory) of the sources `names` calls for.
    pub(crate) fn source_names(names: Names) -> impl Iterator<Item = (usize, &'static str)> {
        (0..SOURCES.len())
            .filter(move |&k| names.has(SOURCE_FLAGS[k]))
            .map(|k| (k, SOURCES[k]))
    }

    /// The frame for a directory at walk-relative `base` whose listing
    /// contained `names`; `parent` unchanged when `names` is empty.
    pub(crate) fn frame(
        &self,
        parent: Option<&Arc<Frame>>,
        names: Names,
        base: &[u8],
    ) -> Option<Arc<Frame>> {
        if names.is_empty() {
            return parent.cloned();
        }
        Some(Arc::new(Frame::new(
            parent.cloned(),
            self.files.clone(),
            names.has(HAS_REPO_MARKER),
            base.into(),
            Box::default(),
        )))
    }
}

/// One directory's link in the chain of applicable ignore files.
#[derive(Debug)]
pub(crate) struct Frame {
    parent: Option<Arc<Self>>,
    files: [Option<Arc<Rules>>; 3],
    base: Box<[u8]>,
    up: Box<[u8]>,
    has_git: bool,
    chain_has_matchers: bool,
    any_git: bool,
}

impl Drop for Frame {
    fn drop(&mut self) {
        let mut next = self.parent.take();
        while let Some(parent) = next {
            next = match Arc::try_unwrap(parent) {
                Ok(mut frame) => frame.parent.take(),
                Err(_) => None,
            };
        }
    }
}

impl Frame {
    fn new(
        parent: Option<Arc<Self>>,
        files: [Option<Arc<Rules>>; 3],
        has_git: bool,
        base: Box<[u8]>,
        up: Box<[u8]>,
    ) -> Self {
        let own = files.iter().any(Option::is_some);
        let (parent_matchers, parent_git) = parent
            .as_ref()
            .map_or((false, false), |p| (p.chain_has_matchers, p.any_git));
        Self {
            parent,
            files,
            base,
            up,
            has_git,
            chain_has_matchers: own || parent_matchers,
            any_git: has_git || parent_git,
        }
    }

    fn rel_path<'a>(&'a self, rules: &Rules, rel: &'a [u8], scratch: &'a mut Vec<u8>) -> &'a [u8] {
        if !rules.anchored {
            return rel;
        }
        if !self.up.is_empty() {
            scratch.clear();
            scratch.extend_from_slice(&self.up);
            scratch.push(b'/');
            scratch.extend_from_slice(rel);
            return scratch;
        }
        if self.base.is_empty() {
            rel
        } else {
            &rel[self.base.len() + 1..]
        }
    }
}

/// Evaluates ignore chains for one walk.
#[derive(Debug)]
pub(crate) struct Ignorer {
    global: Option<Rules>,
    cwd: Vec<u8>,
    root: Vec<u8>,
}

impl Ignorer {
    /// An ignorer that matches nothing (walks without `gitignore`).
    pub(crate) fn empty() -> Self {
        Self {
            global: None,
            cwd: Vec::new(),
            root: Vec::new(),
        }
    }

    /// Loads git's global excludes file and the frames of `root`'s ancestors
    /// up to the nearest repository root.
    pub(crate) async fn new(
        session: &mut Session,
        _fs: &Fs,
        root: &Path,
    ) -> (Self, Option<Arc<Frame>>) {
        let mut buf = Vec::new();
        let GitConfigs {
            configs,
            home,
            fallback,
        } = git_configs();
        let mut excludes = None;
        for config in configs.into_iter().flatten() {
            let opened = session.open_file(&config).await;
            if read_opened(opened, &mut buf).await.is_ok() {
                if let Some(path) = parse_excludes_file(&buf, home.as_deref()) {
                    excludes = Some(path);
                    break;
                }
            }
        }
        let global = match excludes.or(fallback) {
            Some(p) => {
                let opened = session.open_file(&p).await;
                read_opened(opened, &mut buf).await.ok()
            }
            None => None,
        }
        .map(|_| Rules::parse(&buf))
        .filter(|rules| !rules.is_empty());
        let cwd = std::env::current_dir()
            .map_or_else(|_| Vec::new(), |cwd| path_bytes(&cwd).into_owned());
        let ignorer = Self {
            global,
            cwd,
            root: path_bytes(root).into_owned(),
        };
        (ignorer, ancestor_frames(session, root, &mut buf).await)
    }

    /// Whether the entry at walk-relative `rel` is ignored by the chain ending
    /// at `frame`.
    pub(crate) fn ignored(
        &self,
        frame: Option<&Frame>,
        rel: &[u8],
        is_dir: bool,
        scratch: &mut Vec<u8>,
    ) -> bool {
        let Some(frame) = frame else { return false };
        let global = self.global.as_ref().filter(|_| frame.any_git);
        if !frame.chain_has_matchers && global.is_none() {
            return false;
        }
        let mut found = [Match::None; 3];
        let mut saw_git = false;
        let mut f = Some(frame);
        while let Some(cur) = f.filter(|f| f.chain_has_matchers) {
            for (k, slot) in found.iter_mut().enumerate() {
                if *slot != Match::None || (k == 2 && (!frame.any_git || saw_git)) {
                    continue;
                }
                if let Some(rules) = &cur.files[k] {
                    *slot = rules.matched(cur.rel_path(rules, rel, scratch), is_dir);
                }
            }
            saw_git |= cur.has_git;
            f = cur.parent.as_deref();
        }
        if let Some(m) = found.into_iter().find(|m| *m != Match::None) {
            return m == Match::Ignore;
        }
        global.is_some_and(|rules| {
            rules.matched(self.global_path(rules, rel, scratch), is_dir) == Match::Ignore
        })
    }

    fn global_path<'a>(&self, rules: &Rules, rel: &'a [u8], scratch: &'a mut Vec<u8>) -> &'a [u8] {
        if !rules.anchored {
            return rel;
        }
        scratch.clear();
        scratch.extend_from_slice(&self.root);
        if !scratch.ends_with(b"/") {
            scratch.push(b'/');
        }
        scratch.extend_from_slice(rel);
        let cwd = self.cwd.strip_suffix(b"/").unwrap_or(&self.cwd);
        let skip = match scratch.strip_prefix(cwd) {
            Some(rest) if !cwd.is_empty() && rest.first().is_none_or(|&b| b == b'/') => {
                cwd.len() + usize::from(!rest.is_empty())
            }
            _ => 0,
        };
        &scratch[skip..]
    }
}

/// Frames for the directories from the nearest repository root at or above
/// `root`'s parent down to that parent, minus rules that would ignore `root`.
async fn ancestor_frames(
    session: &mut Session,
    root: &Path,
    buf: &mut Vec<u8>,
) -> Option<Arc<Frame>> {
    async fn exists(session: &mut Session, dir: &Path, name: &str) -> bool {
        session
            .stat(&dir.join(OsStr::new(name)), false)
            .await
            .is_ok()
    }
    // (ancestor, root relative to it).
    let mut ancestors: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut up: Vec<u8> = Vec::new();
    let mut repo = None;
    let mut cur = root;
    while let Some(parent) = cur.parent() {
        let Some(name) = cur.file_name() else {
            break;
        };
        let mut next = name.as_encoded_bytes().to_vec();
        if !up.is_empty() {
            next.push(b'/');
            next.extend_from_slice(&up);
        }
        up = next;
        ancestors.push((parent.to_path_buf(), up.clone()));
        if exists(session, parent, ".git").await || exists(session, parent, ".jj").await {
            repo = Some(ancestors.len() - 1);
            break;
        }
        cur = parent;
    }
    let mut frame: Option<Arc<Frame>> = None;
    for (i, (dir, up)) in ancestors[..=repo?].iter().enumerate().rev() {
        let has_git = i == repo?;
        let files = [
            load_ancestor(session, dir, SOURCES[0], up, buf).await,
            load_ancestor(session, dir, SOURCES[1], up, buf).await,
            if has_git {
                load_ancestor(session, dir, SOURCES[2], up, buf).await
            } else {
                None
            },
        ];
        frame = Some(Arc::new(Frame::new(
            frame,
            files,
            has_git,
            Box::default(),
            up.as_slice().into(),
        )));
    }
    frame
}

/// Reads and compiles the rule file `rel` of ancestor `dir`.
async fn load_ancestor(
    session: &mut Session,
    dir: &Path,
    rel: &str,
    up: &[u8],
    buf: &mut Vec<u8>,
) -> Option<Arc<Rules>> {
    let path = dir.join(rel);
    let opened = session.open_file(&path).await;
    read_opened(opened, buf).await.ok()?;
    ancestor_rules(buf, up)
}

/// Compiles an ancestor's rule file, dropping rules that would ignore the
/// walk root at `up` (relative to the ancestor).
fn ancestor_rules(data: &[u8], up: &[u8]) -> Option<Arc<Rules>> {
    let rules = Rules::parse(data);
    if rules.matched_path_or_parents(up, true) != Match::Ignore {
        return non_empty(rules);
    }
    non_empty(Rules::parse_filtered(data, |line| {
        parse_rule(line).is_none_or(|rule| {
            let single = Rules {
                anchored: !rule.glob.basename_only(),
                rules: vec![rule],
            };
            single.matched_path_or_parents(up, true) != Match::Ignore
        })
    }))
}

/// Where git looks for `core.excludesFile`.
struct GitConfigs {
    configs: [Option<PathBuf>; 4],
    home: Option<PathBuf>,
    fallback: Option<PathBuf>,
}

/// Git's config files and the `$XDG_CONFIG_HOME/git/ignore` fallback.
fn git_configs() -> GitConfigs {
    let env_path = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    #[allow(deprecated)]
    let home = std::env::home_dir();
    let config_home =
        env_path("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config")));
    let configs = [
        env_path("GIT_CONFIG_GLOBAL"),
        home.as_ref().map(|h| h.join(".gitconfig")),
        config_home.as_ref().map(|d| d.join("git/config")),
        Some(env_path("GIT_CONFIG_SYSTEM").unwrap_or_else(|| PathBuf::from("/etc/gitconfig"))),
    ];
    GitConfigs {
        configs,
        home,
        fallback: config_home.map(|d| d.join("git/ignore")),
    }
}

/// Extracts `core.excludesFile` from raw git config contents.
fn parse_excludes_file(data: &[u8], home: Option<&Path>) -> Option<PathBuf> {
    const KEY: &[u8] = b"excludesfile";
    let is_space = |b: u8| matches!(b, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ');
    let skip_space = |mut i: usize| {
        while data.get(i).is_some_and(|&b| is_space(b)) {
            i += 1;
        }
        i
    };
    let ends_line = |i: usize| {
        let end = skip_space(i);
        end == data.len() || data[i..end].contains(&b'\n')
    };
    let tail_matches = |i: usize| {
        let quote = skip_space(i);
        ends_line(i) || (data.get(quote) == Some(&b'"') && ends_line(quote + 1))
    };
    let capture_at = |start: usize| {
        let mut end = start;
        while data.get(end).is_some_and(|&b| !is_space(b)) {
            end += 1;
            if tail_matches(end) {
                return Some(&data[start..end]);
            }
        }
        None
    };
    let mut line_starts = std::iter::once(0).chain(memchr::memchr_iter(b'\n', data).map(|i| i + 1));
    let value = line_starts.find_map(|start| {
        let key_start = skip_space(start);
        let key = data.get(key_start..key_start + KEY.len())?;
        if !key.eq_ignore_ascii_case(KEY) {
            return None;
        }
        let equals = skip_space(key_start + KEY.len());
        if data.get(equals) != Some(&b'=') {
            return None;
        }
        let value_start = skip_space(equals + 1);
        (data.get(value_start) == Some(&b'"'))
            .then(|| capture_at(skip_space(value_start + 1)))
            .flatten()
            .or_else(|| capture_at(value_start))
    })?;
    let value = std::str::from_utf8(value).ok()?;
    Some(PathBuf::from(match home {
        Some(home) => value.replace('~', &home.to_string_lossy()),
        None => value.to_owned(),
    }))
}
