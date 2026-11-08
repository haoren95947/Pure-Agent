//! pure-agent ACP backend and pure host in one binary.
//!
//! pure-desktop spawns this executable as `<engine> rpc` and speaks ACP v1 over
//! stdin/stdout. pure-acp, in turn, spawns this same executable as the per-session
//! host (`--mode rpc`), so the distribution ships a single engine binary. The
//! argument shape decides which role runs: the bare `rpc` is the ACP backend,
//! everything else belongs to the pure CLI.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("rpc") {
        run_acp();
    } else {
        run_rpi();
    }
}

/// The ACP backend: pure's `<engine> rpc` role.
fn run_acp() {
    init_environment();
    pure_acp::pure_compat::enable();

    let (input, pending) = match pure_acp::protocol::acp_stdin() {
        Ok(taken) => taken,
        Err(error) => {
            log(&format!("error: {error}"));
            std::process::exit(2);
        }
    };

    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            log(&format!("failed to create tokio runtime: {error}"));
            std::process::exit(2);
        }
    };

    log("serving ACP on stdin/stdout");
    let (_stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let code = runtime.block_on(pure_acp::protocol::run(stop_rx, input, pending));
    log(&format!("ACP server exited with {code}"));
    std::process::exit(code);
}

/// The pure host role: `--mode rpc` and every other pure CLI invocation. The
/// harness drives provider streams with `block_in_place`, so the runtime must
/// be multi-threaded.
fn run_rpi() {
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            log(&format!("failed to create tokio runtime: {error}"));
            std::process::exit(2);
        }
    };
    let code = runtime.block_on(pure_cli::app::run());
    std::process::exit(code);
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .thread_name("pure-backend")
        .build()
}

fn init_environment() {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let pure_home = pure_home();
    let agent_dir = pure_home.join("agent");
    let session_dir = pure_home.join("acp-sessions");
    let log_dir = pure_home.join("logs");
    for dir in [&pure_home, &agent_dir, &session_dir, &log_dir] {
        let _ = std::fs::create_dir_all(dir);
    }

    set_if_absent("PURE_HOME", &pure_home);
    set_if_absent("PURE_AGENT_DIR", &agent_dir);
    set_if_absent("RPI_CODING_AGENT_DIR", &agent_dir);
    set_if_absent("RPI_ACP_SESSION_DIR", &session_dir);
    set_if_absent("RPI_ACP_NO_AUTO_REGISTER_ZED", &PathBuf::from("1"));

    // pure-acp spawns the host through RPI_BIN. This executable is the host too,
    // so the per-session child is the same binary re-invoked with `--mode rpc`;
    // an explicit RPI_BIN (a separately built pure, for debugging) still wins.
    let pure = std::env::var_os("RPI_BIN")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| exe_dir.join(exe_name("pure")));
    if pure.is_file() {
        std::env::set_var("RPI_BIN", &pure);
    } else {
        log(&format!(
            "warning: pure host not found at {}; set RPI_BIN to a pure binary",
            pure.display()
        ));
    }

    let extensions = exe_dir.join("extensions");
    if extensions.is_dir() {
        std::env::set_var("RPI_EXTENSIONS_DIR", &extensions);
    }

    seed_settings(&agent_dir, &exe_dir);
    seed_models(&agent_dir);
    seed_skills(&agent_dir, &exe_dir);

    log(&format!(
        "home={} agent={} sessions={} exe={} pure={}",
        pure_home.display(),
        agent_dir.display(),
        session_dir.display(),
        exe_dir.display(),
        pure.display()
    ));
}

fn pure_home() -> PathBuf {
    if let Some(home) = std::env::var_os("PURE_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(home);
    }
    home_dir().join(".pure-agent")
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn exe_name(stem: &str) -> String {
    format!("{stem}{}", std::env::consts::EXE_SUFFIX)
}

fn set_if_absent(key: &str, value: &Path) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

fn seed_settings(agent_dir: &Path, exe_dir: &Path) {
    let path = agent_dir.join("settings.json");
    if path.exists() {
        return;
    }
    let skills = exe_dir.join("skills");
    let settings = serde_json::json!({
        "defaultThinkingLevel": "medium",
        "enableSkillCommands": true,
        "skillDirs": if skills.is_dir() { serde_json::json!([skills.display().to_string()]) } else { serde_json::json!([]) }
    });
    write_json(&path, &settings);
}

fn seed_models(agent_dir: &Path) {
    let path = agent_dir.join("models.json");
    if path.exists() {
        return;
    }
    write_json(&path, &serde_json::json!({"providers": {}}));
}

fn seed_skills(agent_dir: &Path, exe_dir: &Path) {
    let source = exe_dir.join("skills");
    let target = agent_dir.join("skills");
    if source.is_dir() && !target.exists() {
        if let Err(error) = copy_dir(&source, &target) {
            log(&format!("skills seed failed: {error}"));
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn write_json(path: &Path, value: &serde_json::Value) {
    if let Ok(text) = serde_json::to_string_pretty(value) {
        let _ = std::fs::write(path, text);
    }
}

pub fn log(message: &str) {
    let dir = pure_home().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("backend.log"))
    {
        let _ = writeln!(file, "[{stamp}] {message}");
    }
}
