//! Provider-free port of oh-my-pi's `pi-walker` core: `walk`, `ignore`,
//! `pattern`, and `glob`, reading through the crate's [`crate::env::FileSystem`]
//! and using the `regex` crate (no PCRE2).

pub mod fs;
pub mod glob;
pub mod ignore;
pub mod pattern;
pub mod walk;

pub use fs::{Detail, Dir, FileType, Fs, Listing, Record};
pub use glob::{glob, GlobError, GlobMatch, GlobOptions, Selector};
pub use ignore::{Match, Rules};
pub use pattern::{Dialect, Glob, PatternError};
pub use walk::{
    collect, walk, Action, Batch, Collection, DirError, Entry, Options, Stop, Walk, WalkError,
    Worker,
};
