//! Stamps the version the client reports, and on Windows embeds the
//! executable icon for Explorer, the taskbar and Alt-Tab.

fn main() {
    stamp_version();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=assets/pure.ico");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/pure.ico");
    resource.compile().expect("the Windows icon embeds");
}

/// Stamps `PURE_VERSION`, the version this binary reports, and
/// `PURE_COMMIT`, the commit it was built from when known: `0.3.1
/// (abc1234)`, `nightly (abc1234)` when `PURE_CHANNEL=nightly`, or a
/// bare `0.3.1` when no commit is known.
fn stamp_version() {
    println!("cargo:rerun-if-env-changed=PURE_BUILD_SHA");
    println!("cargo:rerun-if-env-changed=PURE_CHANNEL");
    let commit = std::env::var("PURE_BUILD_SHA")
        .ok()
        .filter(|sha| !sha.is_empty())
        .or_else(packaged_commit)
        .or_else(checkout_commit);
    let version = if std::env::var("PURE_CHANNEL").as_deref() == Ok("nightly") {
        "nightly".to_owned()
    } else {
        std::env::var("CARGO_PKG_VERSION").expect("cargo sets the package version")
    };
    let version = match &commit {
        Some(sha) => format!("{version} ({})", &sha[..sha.len().min(7)]),
        None => version,
    };
    if let Some(sha) = &commit {
        println!("cargo:rustc-env=PURE_COMMIT={sha}");
    }
    println!("cargo:rustc-env=PURE_VERSION={version}");
}

/// The commit `cargo publish` recorded, for a build from crates.io.
fn packaged_commit() -> Option<String> {
    let info = std::fs::read_to_string(".cargo_vcs_info.json").ok()?;
    let rest = &info[info.find("\"sha1\"")? + 6..];
    let rest = &rest[rest.find('"')? + 1..];
    Some(rest[..rest.find('"')?].to_owned())
}

/// The commit a git checkout is at. The script reruns when `HEAD`
/// moves, so a new commit restamps without a clean build.
fn checkout_commit() -> Option<String> {
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git").args(args).output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let dir = git(&["rev-parse", "--absolute-git-dir"])?;
    println!("cargo:rerun-if-changed={dir}/HEAD");
    if let Some(head) = git(&["symbolic-ref", "-q", "HEAD"]) {
        println!("cargo:rerun-if-changed={dir}/{head}");
        println!("cargo:rerun-if-changed={dir}/packed-refs");
    }
    git(&["rev-parse", "HEAD"]).filter(|sha| !sha.is_empty())
}
