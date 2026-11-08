//! `pure-rtk` — compress shell output with a bundled RTK binary.
//!
//! RTK filters command output before it reaches the model. This extension runs
//! the vendored `rtk` in Unix-pipe mode (`rtk pipe`) over finished `bash` /
//! `powershell` results and hands the model the shorter text. It never changes
//! the command, never re-runs it, and never enlarges a result.
//!
//! - **Fail open.** A missing binary, a spawn error, a timeout, a non-zero
//!   exit, a non-UTF-8 result, or a "compressed" result that is not strictly
//!   smaller all leave the tool result untouched.
//! - **Successful and failed calls.** The host offers a failed tool result to
//!   `tool_result` when its error carries native content (bash's non-zero exit
//!   does), so failing test/build output is compressed too.
//! - **Filter choice.** A shell command maps to RTK's named filters by its
//!   leading words (`git status`, `cargo test`, `pytest`, …); anything else
//!   falls through to `rtk pipe`'s auto-detection.
//! - **Shell tools only.** `read` is excluded deliberately: rewriting its text
//!   would break `edit`'s `old_string` matching, and `grep`/`find`/`ls`
//!   results are already structured for follow-up tool calls.
//!
//! Binary lookup order: `PURE_RTK_BIN`, `vendor/rtk/<platform>/` beside the
//! extension (each `RPI_EXTENSIONS_DIR` entry), the host executable's
//! `extensions/` and root directories, this crate's own `vendor/` (dev runs),
//! then `rtk` on `PATH`.
//!
//! Configuration lives in `<agent dir>/rtk.json` and is read per call so edits
//! apply without a session reload:
//!
//! ```json
//! {"enabled": true, "minBytes": 512, "timeoutMs": 3000, "awareness": false}
//! ```
//!
//! Env overrides: `PURE_RTK` (0 disables), `PURE_RTK_BIN`, `PURE_RTK_MIN_BYTES`,
//! `PURE_RTK_TIMEOUT_MS`, `PURE_RTK_AWARENESS` (1 injects a short note that the
//! binary exists — off by default because it costs prompt tokens).

use pure_plugin_sdk::callbacks::free_string;
use pure_plugin_sdk::{
    guard_or, register_entrypoint, register_tool_result, PluginApi, StbString, StbStringRef,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CONFIG_FILE: &str = "rtk.json";
const ENABLE_ENV: &str = "PURE_RTK";
const BIN_ENV: &str = "PURE_RTK_BIN";
const MIN_BYTES_ENV: &str = "PURE_RTK_MIN_BYTES";
const TIMEOUT_ENV: &str = "PURE_RTK_TIMEOUT_MS";
const AWARENESS_ENV: &str = "PURE_RTK_AWARENESS";

const DEFAULT_MIN_BYTES: usize = 512;
const DEFAULT_TIMEOUT_MS: u64 = 3_000;
/// `rtk pipe` refuses input above 10 MiB; below that, our own cap keeps a
/// pathological result from paying a process spawn for nothing.
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Settings {
    enabled: bool,
    min_bytes: usize,
    timeout_ms: u64,
    awareness: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            min_bytes: DEFAULT_MIN_BYTES,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            awareness: false,
        }
    }
}

/// The agent directory holding `rtk.json` (same convention as
/// `pure-permission-gate`).
fn agent_dir() -> Option<PathBuf> {
    if let Some(dir) =
        std::env::var_os("PURE_AGENT_DIR").or_else(|| std::env::var_os("RPI_CODING_AGENT_DIR"))
    {
        return Some(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("PURE_HOME") {
        return Some(Path::new(&home).join("agent"));
    }
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| Path::new(&home).join(".pure-agent").join("agent"))
}

fn config() -> Value {
    agent_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join(CONFIG_FILE)).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

fn env_bool(name: &str) -> Option<bool> {
    let value = std::env::var(name).ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(!matches!(
        value.to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    ))
}

fn settings() -> Settings {
    let defaults = Settings::default();
    let config = config();
    let config_bool = |key: &str| config.get(key).and_then(Value::as_bool);
    let config_u64 = |key: &str| config.get(key).and_then(Value::as_u64);

    let min_bytes = std::env::var(MIN_BYTES_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .or_else(|| config_u64("minBytes").map(|value| value as usize))
        .unwrap_or(defaults.min_bytes);
    let timeout_ms = std::env::var(TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .or_else(|| config_u64("timeoutMs"))
        .unwrap_or(defaults.timeout_ms);

    Settings {
        enabled: env_bool(ENABLE_ENV)
            .or_else(|| config_bool("enabled"))
            .unwrap_or(defaults.enabled),
        min_bytes,
        timeout_ms: timeout_ms.clamp(50, 30_000),
        awareness: env_bool(AWARENESS_ENV)
            .or_else(|| config_bool("awareness"))
            .unwrap_or(defaults.awareness),
    }
}

/// The vendored directory name for this host, matching `vendor/rtk/`.
fn platform_dir() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("win32-x64"),
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "x86_64") => Some("linux-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "rtk.exe"
    } else {
        "rtk"
    }
}

fn extension_dirs() -> Vec<PathBuf> {
    let Some(raw) = std::env::var_os("RPI_EXTENSIONS_DIR") else {
        return Vec::new();
    };
    let raw = raw.to_string_lossy().into_owned();
    let separator = if cfg!(windows) { ';' } else { ':' };
    raw.split(separator)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn resolve_rtk_binary() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os(BIN_ENV) {
        let configured = PathBuf::from(configured);
        if configured.is_file() {
            return Some(configured);
        }
    }

    let name = binary_name();
    let platform = platform_dir()?;
    let mut roots = extension_dirs();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            roots.push(parent.join("extensions"));
            roots.push(parent.to_path_buf());
        }
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    for root in roots {
        let candidate = root.join("vendor").join("rtk").join(platform).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    find_on_path(name)
}

/// A shell command split at `&&`, `||`, `;`, `|` and newlines. RTK filters the
/// whole output, so every segment is tried until one maps to a filter.
fn segments(command: &str) -> impl Iterator<Item = &str> {
    command
        .split(['\n', ';', '|'])
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
        .map(str::trim)
        .filter(|part| !part.is_empty())
}

/// The command words after dropping wrapper prefixes (`sudo`, `time`,
/// `env FOO=bar`, `npx`, leading `VAR=value` assignments).
fn command_tokens(segment: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = segment.split_whitespace().collect();
    loop {
        let Some(head) = tokens.first().copied() else {
            break;
        };
        if matches!(head, "sudo" | "command" | "nohup" | "time")
            || (head.contains('=') && !head.starts_with('-'))
        {
            tokens.remove(0);
            continue;
        }
        if head == "env" {
            tokens.remove(0);
            while tokens
                .first()
                .is_some_and(|token| token.contains('='))
            {
                tokens.remove(0);
            }
            continue;
        }
        if head == "npx" {
            tokens.remove(0);
            continue;
        }
        break;
    }
    tokens
}

fn filter_for_segment(segment: &str) -> Option<&'static str> {
    let tokens = command_tokens(segment);
    let first = tokens.first().copied()?;
    let second = tokens.get(1).copied().unwrap_or("");
    let third = tokens.get(2).copied().unwrap_or("");
    match first {
        "python" | "python3" if second == "-m" && third == "pytest" => Some("pytest"),
        "git" => match second {
            "status" => Some("git-status"),
            "log" => Some("git-log"),
            "diff" => Some("git-diff"),
            _ => None,
        },
        "cargo" if second == "test" => Some("cargo-test"),
        "pytest" => Some("pytest"),
        "go" => match second {
            "test" => Some("go-test"),
            "build" => Some("go-build"),
            _ => None,
        },
        "ctest" => Some("ctest"),
        "tsc" => Some("tsc"),
        "vitest" => Some("vitest"),
        "mypy" => Some("mypy"),
        "ruff" if second == "check" => Some("ruff-check"),
        "ruff" if second == "format" => Some("ruff-format"),
        "sqlfluff" if second == "lint" => Some("sqlfluff-lint"),
        "prettier" => Some("prettier"),
        "phpunit" => Some("phpunit"),
        "pest" => Some("pest"),
        "paratest" => Some("paratest"),
        "phpstan" => Some("phpstan"),
        "pint" => Some("pint"),
        "ecs" => Some("ecs"),
        "rg" => Some("rg"),
        "grep" => Some("grep"),
        "find" => Some("find"),
        "fd" => Some("fd"),
        "tail" | "journalctl" => Some("log"),
        "docker" if second == "logs" => Some("log"),
        "kubectl" if second == "logs" => Some("log"),
        _ => None,
    }
}

fn filter_for_command(command: &str) -> Option<&'static str> {
    segments(command).find_map(filter_for_segment)
}

fn already_compressed(command: &str) -> bool {
    command_tokens(command).first().is_some_and(|head| {
        let head = head.trim_matches(['"', '\'']);
        let name = head.rsplit(['/', '\\']).next().unwrap_or(head);
        name.eq_ignore_ascii_case("rtk") || name.eq_ignore_ascii_case("rtk.exe")
    })
}

fn command_of(input: &Value) -> Option<&str> {
    input.get("command").and_then(Value::as_str)
}

/// Join a tool result's text blocks. Anything that is not text (an image, an
/// unknown block) means "leave this result alone".
fn text_of(content: &Value) -> Option<String> {
    let blocks = content.as_array()?;
    let mut text = String::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(block.get("text").and_then(Value::as_str).unwrap_or(""));
            }
            _ => return None,
        }
    }
    Some(text)
}

fn run_pipe(binary: &Path, filter: Option<&str>, input: &str, timeout: Duration) -> Option<String> {
    let mut command = Command::new(binary);
    command.arg("pipe");
    if let Some(filter) = filter {
        command.arg("--filter").arg(filter);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("NO_COLOR", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }

    let mut child = command.spawn().ok()?;
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;

    let bytes = input.as_bytes().to_vec();
    let writer = std::thread::spawn(move || {
        use std::io::Write;
        let _ = stdin.write_all(&bytes);
    });
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        output
    });

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = reader.join().ok()?;
                let _ = writer.join();
                if !status.success() {
                    return None;
                }
                return String::from_utf8(stdout).ok();
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

/// The compressed text, or `None` when the result must stay as it is.
fn compress_text(text: &str, filter: Option<&str>, settings: &Settings) -> Option<String> {
    if text.len() < settings.min_bytes || text.len() > MAX_INPUT_BYTES {
        return None;
    }
    if text.as_bytes().contains(&0) {
        return None;
    }
    let binary = resolve_rtk_binary()?;
    let compressed = run_pipe(
        &binary,
        filter,
        text,
        Duration::from_millis(settings.timeout_ms),
    )?;
    (compressed.len() < text.len()).then_some(compressed)
}

fn replacement(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}]})
}

extern "C" fn on_tool_result(
    event_json: StbStringRef,
    out: *mut StbString,
    _user_data: *mut std::ffi::c_void,
) -> i32 {
    guard_or(1, || {
        let settings = settings();
        if !settings.enabled {
            return 0;
        }
        let envelope: Value = match serde_json::from_str(unsafe { event_json.as_str() }) {
            Ok(envelope) => envelope,
            Err(_) => return 0,
        };
        let tool = envelope
            .get("toolName")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !matches!(tool, "bash" | "powershell") {
            return 0;
        }
        let input = envelope.get("input").unwrap_or(&Value::Null);
        let Some(command) = command_of(input) else {
            return 0;
        };
        if already_compressed(command) {
            return 0;
        }
        let Some(text) = text_of(envelope.get("content").unwrap_or(&Value::Null)) else {
            return 0;
        };
        let Some(compressed) = compress_text(&text, filter_for_command(command), &settings) else {
            return 0;
        };
        unsafe { *out = StbString::from_string(replacement(&compressed).to_string()) };
        0
    })
}

fn awareness_block(binary: &Path) -> String {
    format!(
        "## RTK output compression\n\
         A bundled `rtk` binary is available at `{}`. Output of common shell commands \
         (tests, builds, git, grep, find) is compressed automatically after the command runs; \
         do not re-run a command to re-read the same output. When you need a compact listing \
         of a command RTK covers (for example `ls`, `tree`, `docker ps`), run it as \
         `\"{}\" <command-and-args>`.",
        binary.display(),
        binary.display()
    )
}

extern "C" fn on_before_agent_start(
    event_json: StbStringRef,
    out: *mut StbString,
    _user_data: *mut std::ffi::c_void,
) -> i32 {
    guard_or(1, || {
        let Some(binary) = resolve_rtk_binary() else {
            return 0;
        };
        let envelope: Value = match serde_json::from_str(unsafe { event_json.as_str() }) {
            Ok(envelope) => envelope,
            Err(_) => return 0,
        };
        let current = envelope
            .get("systemPrompt")
            .and_then(Value::as_str)
            .unwrap_or("");
        let next = format!("{current}\n\n{}", awareness_block(&binary));
        unsafe {
            *out = StbString::from_string(json!({"systemPrompt": next}).to_string());
        }
        0
    })
}

#[no_mangle]
// The host resolves this symbol and calls it with the unified-ABI signature;
// the raw pointer is validated by `register_entrypoint` before use.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn rpi_plugin_register(api: *const PluginApi) -> i32 {
    unsafe {
        register_entrypoint(api, |api| {
            let settings = settings();
            if !settings.enabled {
                return 0;
            }
            let _ = register_tool_result(api, on_tool_result, std::ptr::null_mut());
            if settings.awareness {
                if let Some(register) = api.register_before_agent_start {
                    let _ = register(on_before_agent_start, free_string, std::ptr::null_mut());
                }
            }
            0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_map_to_rtk_pipe_filters() {
        for (command, expected) in [
            ("git status", Some("git-status")),
            ("cd /repo && git log -n 5", Some("git-log")),
            ("git diff HEAD~1", Some("git-diff")),
            ("cargo test --all", Some("cargo-test")),
            ("cargo build", None),
            ("python -m pytest -q", Some("pytest")),
            ("sudo npx tsc --noEmit", Some("tsc")),
            ("docker logs api | tail -100", Some("log")),
            ("echo hi && cargo test 2>&1 | tail -20", Some("cargo-test")),
            ("FOO=1 go test ./...", Some("go-test")),
            ("ls -la", None),
            ("rtk git status", None),
        ] {
            assert_eq!(filter_for_command(command), expected, "{command}");
        }
    }

    #[test]
    fn an_rtk_prefix_is_recognized_through_paths_and_quotes() {
        assert!(already_compressed("rtk git status"));
        assert!(already_compressed("C:\\tools\\rtk.exe cargo test"));
        assert!(already_compressed("\"/opt/bin/rtk\" cargo test"));
        assert!(!already_compressed("git status"));
        assert!(!already_compressed("echo rtk"));
    }

    #[test]
    fn only_text_blocks_are_compressible() {
        assert_eq!(
            text_of(&json!([{"type": "text", "text": "a"}, {"type": "text", "text": "b"}])),
            Some("a\nb".to_string())
        );
        assert_eq!(
            text_of(&json!([{"type": "image", "data": "UE5H", "mimeType": "image/png"}])),
            None
        );
        assert_eq!(text_of(&Value::Null), None);
    }

    #[test]
    fn short_or_wasteful_results_are_left_alone() {
        let settings = Settings {
            min_bytes: 8,
            timeout_ms: 3_000,
            ..Settings::default()
        };
        assert_eq!(compress_text("short", None, &settings), None);
        // Below the default threshold, nothing is spawned.
        assert_eq!(compress_text("hello", None, &Settings::default()), None);
    }

    #[test]
    fn the_vendored_binary_compresses_a_failing_test_run() {
        if resolve_rtk_binary().is_none() {
            eprintln!("skipping: no vendored rtk binary for this platform");
            return;
        }
        let input = "\
   Compiling pure-tools v0.21.0
    Finished `test` profile [unoptimized + debuginfo] target(s) in 12.34s
     Running unittests src\\lib.rs

running 42 tests
test tools::bash::tests::ok_one ... ok
test tools::bash::tests::ok_two ... ok
test tools::bash::tests::fail_one ... FAILED
test tools::bash::tests::ok_three ... ok

failures:

---- tools::bash::tests::fail_one stdout ----
thread 'tools::bash::tests::fail_one' panicked at crates\\bash.rs:100:5:
assertion `left == right` failed
  left: 1
 right: 2

failures:
    tools::bash::tests::fail_one

test result: FAILED. 41 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
";
        let settings = Settings::default();
        let compressed = compress_text(input, Some("cargo-test"), &settings)
            .expect("the vendored binary must compress this fixture");
        assert!(compressed.len() < input.len(), "{compressed}");
        assert!(compressed.contains("FAILED"), "{compressed}");
    }
}