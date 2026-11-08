//! What this build of pure calls itself.
//!
//! A build names the commit it was made from when it knows it, and a
//! nightly names itself instead of a release number: `0.3.1`,
//! `0.3.1 (abc1234)` or `nightly (abc1234)`. The binaries stamp the
//! string at compile time and hand it over with [`set_version`] first
//! thing, so the libraries report the same one over the protocols
//! without rebuilding on every commit.

use std::sync::OnceLock;

/// The version the running binary set.
static VERSION: OnceLock<&'static str> = OnceLock::new();

/// Records the version the running binary was stamped with. Only the
/// first call counts.
pub fn set_version(version: &'static str) {
    let _ = VERSION.set(version);
}

/// The version this build reports: what the binary set, else the bare
/// crate version.
#[must_use]
pub fn version() -> &'static str {
    VERSION.get().copied().unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// Whether a reported version names a nightly build.
#[must_use]
pub fn is_nightly(version: &str) -> bool {
    version.split_whitespace().next() == Some("nightly")
}

/// The release number a reported version carries: `0.3.1` of
/// `0.3.1 (abc1234)`. `None` for a nightly, which has none, or for an
/// empty report.
#[must_use]
pub fn release(version: &str) -> Option<&str> {
    version
        .split_whitespace()
        .next()
        .filter(|number| *number != "nightly")
}

#[cfg(test)]
mod tests {
    use super::{is_nightly, release};

    #[test]
    fn a_release_number_is_read_past_the_commit() {
        assert_eq!(release("0.3.1"), Some("0.3.1"));
        assert_eq!(release("0.3.1 (abc1234)"), Some("0.3.1"));
        assert_eq!(release("nightly (abc1234)"), None);
        assert_eq!(release(""), None);
    }

    #[test]
    fn a_nightly_is_known_by_its_name() {
        assert!(is_nightly("nightly (abc1234)"));
        assert!(is_nightly("nightly"));
        assert!(!is_nightly("0.3.1 (abc1234)"));
        assert!(!is_nightly("nightlyish"));
    }
}
