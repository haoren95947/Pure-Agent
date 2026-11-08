//! Where the curated model catalog is published.
//!
//! Every crate that reads the catalog has to agree on its address: the
//! CLI that refreshes it, the provider crate that parses it, and the
//! GUI, which loads a provider's logo from the same origin.

/// The curated catalog, published by
/// [`pure-models`](https://github.com/QaidVoid/pure-models). It merges
/// models.dev with the model lists and plans the other providers
/// publish, so pure never fetches an upstream itself. Override with
/// [`CATALOG_URL_ENV`] to pin a revision or read a local file.
pub const CATALOG_URL: &str = "https://models.pure.qaidvoid.dev/";

/// The directory on the same origin holding one SVG per provider, which
/// pure-models publishes beside the catalog.
pub const LOGOS_PATH: &str = "logos";

/// Environment variable overriding [`CATALOG_URL`].
pub const CATALOG_URL_ENV: &str = "PURE_CATALOG_URL";

/// The curated catalog URL, honouring [`CATALOG_URL_ENV`].
///
/// An unset or empty variable falls back to the published default, so
/// an exported-but-blank override does not break the refresh.
#[must_use]
pub fn catalog_url() -> String {
    std::env::var(CATALOG_URL_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| CATALOG_URL.to_owned())
}

/// The origin of [`catalog_url`], when it is one a browser can fetch.
///
/// This is what a server's Content-Security-Policy has to allow, and
/// keeping the extraction here means the policy and the URL cannot
/// disagree about which origin that is. `None` for anything a page
/// cannot fetch: a `file://` catalog is read by the engine, a malformed
/// override has no origin, and `ftp://` is not a fetch scheme.
#[must_use]
pub fn catalog_origin() -> Option<String> {
    let url = catalog_url();
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let host = rest.split('/').next().unwrap_or_default();
    (!host.is_empty()).then(|| format!("{scheme}://{host}"))
}

/// The URL one provider's logo is read from, on the origin the catalog
/// itself comes from.
///
/// A `file://` catalog reads from the `logos/` directory beside the
/// file, and an HTTP catalog whose path ends in a file segment (no
/// trailing slash) likewise, so a single override pins the catalog and
/// its logos together.
#[must_use]
pub fn logo_url(id: &str) -> String {
    let base = catalog_url();
    if let Some(path) = base.strip_prefix("file://") {
        let dir = std::path::Path::new(path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        return format!("file://{}/{LOGOS_PATH}/{id}.svg", dir.display());
    }
    // A trailing slash names the logo directory itself; otherwise the
    // last segment is the catalog file and the logos sit beside it.
    let dir = if let Some(dir) = base.strip_suffix('/') {
        dir
    } else if let Some((dir, _)) = base.rsplit_once('/') {
        dir
    } else {
        base.as_str()
    };
    format!("{dir}/{LOGOS_PATH}/{id}.svg")
}

#[cfg(test)]
#[expect(
    clippy::result_large_err,
    reason = "figment::Jail closures must return figment::Error"
)]
mod tests {
    use super::*;

    use crate::test_support::process_globals;

    #[test]
    fn an_http_catalog_yields_the_origin_a_page_may_fetch() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            // The test runner may itself export an override; pin it.
            jail.set_env(CATALOG_URL_ENV, CATALOG_URL);
            assert_eq!(
                catalog_origin(),
                Some("https://models.pure.qaidvoid.dev".to_owned())
            );
            Ok(())
        });
    }

    #[test]
    fn only_the_origin_is_allowed_and_a_subpath_is_dropped() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            jail.set_env(CATALOG_URL_ENV, "https://example.com/feed/");
            assert_eq!(catalog_origin(), Some("https://example.com".to_owned()));
            jail.set_env(CATALOG_URL_ENV, "http://127.0.0.1:8787/catalog.json");
            assert_eq!(catalog_origin(), Some("http://127.0.0.1:8787".to_owned()));
            Ok(())
        });
    }

    #[test]
    fn a_local_catalog_allows_no_origin() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            jail.set_env(CATALOG_URL_ENV, "file:///tmp/catalog.json");
            assert_eq!(catalog_origin(), None);
            Ok(())
        });
    }

    #[test]
    fn a_malformed_override_allows_no_origin() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            for bad in [
                "not-a-url",
                "https://",
                "://example.com",
                "ftp://example.com/x",
            ] {
                jail.set_env(CATALOG_URL_ENV, bad);
                assert_eq!(catalog_origin(), None, "{bad:?} must allow nothing");
            }
            Ok(())
        });
    }

    #[test]
    fn a_logo_sits_beside_the_catalog_on_the_same_origin() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            jail.set_env(CATALOG_URL_ENV, CATALOG_URL);
            assert_eq!(
                logo_url("groq"),
                "https://models.pure.qaidvoid.dev/logos/groq.svg"
            );
            assert_eq!(catalog_url(), CATALOG_URL);
            Ok(())
        });
    }

    #[test]
    fn a_trailing_slash_on_the_override_does_not_double() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            jail.set_env(CATALOG_URL_ENV, "https://example.com/feed/");
            assert_eq!(logo_url("groq"), "https://example.com/feed/logos/groq.svg");
            Ok(())
        });
    }

    #[test]
    fn a_file_shaped_http_url_reads_the_logos_beside_the_catalog() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            jail.set_env(CATALOG_URL_ENV, "http://127.0.0.1:8787/catalog.json");
            assert_eq!(logo_url("groq"), "http://127.0.0.1:8787/logos/groq.svg");
            Ok(())
        });
    }

    #[test]
    fn a_file_catalog_reads_the_logos_beside_it() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            // One override pins the catalog and its logos together.
            let path = jail.directory().join("catalog.json");
            jail.set_env(CATALOG_URL_ENV, format!("file://{}", path.display()));
            assert_eq!(
                logo_url("groq"),
                format!("file://{}/logos/groq.svg", jail.directory().display())
            );
            Ok(())
        });
    }

    #[test]
    fn a_blank_override_falls_back_to_the_published_catalog() {
        let _globals = process_globals();
        figment::Jail::expect_with(|jail| {
            for blank in ["", "   "] {
                jail.set_env(CATALOG_URL_ENV, blank);
                assert_eq!(catalog_url(), CATALOG_URL, "blank: {blank:?}");
                assert_eq!(
                    logo_url("groq"),
                    "https://models.pure.qaidvoid.dev/logos/groq.svg"
                );
            }
            Ok(())
        });
    }
}
