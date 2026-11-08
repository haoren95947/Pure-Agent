//! `pure-permission-gate` — confirm dangerous tool calls through the host's
//! generic extension UI dialog.
//!
//! A generic policy plugin: it subscribes to `on("tool_call")` (the host's
//! `before_tool_call` veto) and asks the user before `bash` / `powershell` /
//! `write` / `edit` runs. Rules in the agent directory's `permissions.json`
//! can allow/deny calls without a dialog, and `confine_paths` blocks file
//! tools from escaping the session workspace.
//!
//! Off by default. Enable with `--permission-gate` or `RPI_PERMISSION_GATE=1`.
//! `--bypass` overrides both activation methods for this process. A declined
//! call returns `EVENT_HANDLER_ABORT`, which the host turns into a blocked
//! tool; an unanswerable dialog also blocks (fail closed).

use globset::Glob;
use rpi_plugin_sdk::{
    register_entrypoint, EventTag, FreeStringFn, PluginApi, RuntimeActionFn, RuntimeActionId,
    StbString, StbStringRef, EVENT_HANDLER_ABORT, EVENT_HANDLER_CONTINUE,
};
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// CLI flag that turns gating on.
const GATE_FLAG: &str = "permission-gate";
const BYPASS_FLAG: &str = "bypass";
/// Env equivalent (useful when the flag cannot be passed).
const GATE_ENV: &str = "RPI_PERMISSION_GATE";

/// Tools confirmed through the dialog by default.
const GATED_TOOLS: [&str; 4] = ["bash", "powershell", "write", "edit"];
/// Tools whose `path` argument is subject to `confine_paths`.
const PATH_TOOLS: [&str; 6] = ["read", "write", "edit", "ls", "find", "grep"];

#[derive(Clone, Copy)]
struct HostRuntime {
    action: RuntimeActionFn,
    free: FreeStringFn,
    user_data: *mut std::ffi::c_void,
}
// SAFETY: the host warrants the runtime-action fn + its opaque `user_data` are
// safe to call from any thread (the same contract every host-runtime plugin
// relies on, e.g. pure-subagents / pure-goal).
unsafe impl Send for HostRuntime {}
unsafe impl Sync for HostRuntime {}

static RUNTIME: OnceLock<HostRuntime> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(false);
static REQUEST_SEQ: AtomicU64 = AtomicU64::new(0);

fn get_cli_flag(name: &str) -> Option<Value> {
    let runtime = *RUNTIME.get()?;
    let args = json!({"name": name}).to_string();
    let mut out = StbString::empty();
    let rc = (runtime.action)(
        RuntimeActionId::GetCliFlag as u32,
        StbStringRef::from_str(&args),
        &mut out,
        runtime.user_data,
    );
    let text = if rc == 0 {
        Some(out.to_string_lossy())
    } else {
        None
    };
    (runtime.free)(out);
    let value: Value = serde_json::from_str(&text?).ok()?;
    value.get("value").cloned()
}

fn flag_is_on(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(on)) => *on,
        Some(Value::String(text)) => !text.is_empty() && !text.eq_ignore_ascii_case("false"),
        _ => false,
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            let value = value.trim();
            !value.is_empty() && !value.eq_ignore_ascii_case("false") && value != "0"
        })
        .unwrap_or(false)
}

fn gate_enabled(bypass: bool, flag: bool, environment: bool) -> bool {
    !bypass && (flag || environment)
}

fn next_request_id() -> String {
    let seq = REQUEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("gate-{}-{seq}-{nanos}", std::process::id())
}

fn invoke_ui_dialog(runtime: &HostRuntime, args: &Value) -> Result<Value, String> {
    let input = args.to_string();
    let mut out = StbString::empty();
    let rc = (runtime.action)(
        RuntimeActionId::UiDialog as u32,
        StbStringRef::from_str(&input),
        &mut out,
        runtime.user_data,
    );
    let text = out.to_string_lossy();
    (runtime.free)(out);
    if rc != 0 {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                return Err(error.to_owned());
            }
        }
        return Err(if text.is_empty() {
            format!("UI dialog failed with status {rc}")
        } else {
            text
        });
    }
    serde_json::from_str(&text).map_err(|error| format!("UI dialog returned invalid JSON: {error}"))
}

/// The `UiDialog` payload for one tool call. It must satisfy the SDK's dialogs
/// contract (`purpose: "permission"` + an explicit `permission` decision per
/// option) or the rpc/ACP bridges answer it as cancelled instead of asking.
fn permission_ui(tool_name: &str, args: &Value) -> Value {
    json!({
        "kind": "confirm",
        "purpose": "permission",
        "id": "permission-gate",
        "question": format!("Allow {tool_name}?"),
        "message": summarize(tool_name, args),
        "options": [{"title": "Yes", "permission": "allow_once"}, {"title": "No", "permission": "reject_once"}],
    })
}

/// Ask the user to confirm one tool call. `Ok(true)` = allowed.
fn confirm(
    runtime: &HostRuntime,
    tool_call_id: &str,
    tool_name: &str,
    args: &Value,
) -> Result<bool, String> {
    let request_id = next_request_id();
    let open = json!({
        "op": "open",
        "requestId": request_id,
        "toolCallId": tool_call_id,
        "ui": permission_ui(tool_name, args),
    });
    invoke_ui_dialog(runtime, &open)?;

    // The plugin-side dialog protocol is open → poll → answer. Sleep briefly
    // between polls; the host's UI bridge delivers the answer asynchronously.
    loop {
        let response = invoke_ui_dialog(runtime, &json!({"op": "poll", "requestId": request_id}))?;
        match response
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("pending")
        {
            "answered" => {
                let answer = response.get("answer").cloned().unwrap_or(Value::Null);
                let allowed = answer_values(&answer)
                    .first()
                    .map(|value| value.eq_ignore_ascii_case("yes"))
                    .unwrap_or(false);
                return Ok(allowed);
            }
            "cancelled" => return Ok(false),
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn answer_values(answer: &Value) -> Vec<String> {
    if let Some(values) = answer.get("values").and_then(Value::as_array) {
        return values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
    }
    if let Some(value) = answer.get("value").and_then(Value::as_str) {
        return vec![value.to_string()];
    }
    Vec::new()
}

/// A short human summary of what the call will do.
fn summarize(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "bash" | "powershell" => args
            .get("command")
            .and_then(Value::as_str)
            .map(|command| truncate(command, 400))
            .unwrap_or_else(|| "run a shell command".into()),
        "write" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("file");
            let bytes = args
                .get("content")
                .and_then(Value::as_str)
                .map(str::len)
                .unwrap_or(0);
            format!("write {path} ({bytes} bytes)")
        }
        "edit" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("file");
            let edits = args
                .get("edits")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            format!("edit {path} ({edits} replacement(s))")
        }
        other => other.to_string(),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{head}…")
}

/// The agent directory holding `permissions.json` (and the other config).
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

/// `agent/permissions.json`, read per call so Settings edits apply without a
/// session reload. Missing or malformed config means "no rules".
fn permissions() -> Value {
    agent_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join("permissions.json")).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

/// The value a tool rule matches: a shell command or a file path.
fn subject<'a>(tool_name: &str, args: &'a Value) -> &'a str {
    let field = if matches!(tool_name, "bash" | "powershell") {
        "command"
    } else {
        "path"
    };
    args.get(field).and_then(Value::as_str).unwrap_or("")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    Allow,
    Deny,
    Ask,
}

/// `deny` wins over `allow`, which wins over `default` (`ask` when omitted).
fn rule_decision(tool_name: &str, subject: &str, permissions: &Value) -> Decision {
    let Some(rule) = permissions
        .get("tools")
        .and_then(|tools| tools.get(tool_name))
    else {
        return Decision::Ask;
    };
    if matches_any(rule.get("deny"), subject) {
        return Decision::Deny;
    }
    if matches_any(rule.get("allow"), subject) {
        return Decision::Allow;
    }
    match rule.get("default").and_then(Value::as_str).unwrap_or("ask") {
        "allow" => Decision::Allow,
        "deny" => Decision::Deny,
        _ => Decision::Ask,
    }
}

fn matches_any(patterns: Option<&Value>, subject: &str) -> bool {
    patterns.and_then(Value::as_array).is_some_and(|patterns| {
        patterns.iter().filter_map(Value::as_str).any(|pattern| {
            Glob::new(pattern)
                .map(|glob| glob.compile_matcher().is_match(subject))
                .unwrap_or(false)
        })
    })
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Whether a `path` argument resolves outside the session's working directory.
fn path_escapes_workspace(path: &str) -> bool {
    let Ok(cwd) = std::env::current_dir() else {
        return false;
    };
    let cwd = normalize(&cwd);
    let candidate = Path::new(path);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        cwd.join(candidate)
    };
    !normalize(&joined).starts_with(&cwd)
}

rpi_plugin_sdk::event_handler! {
/// `on("tool_call")`: consult `permissions.json`, confirm dangerous tools
/// before they run, and veto (`EVENT_HANDLER_ABORT`) when declined.
fn on_tool_call(event, _data) {
    if !ENABLED.load(Ordering::Acquire) {
        return EVENT_HANDLER_CONTINUE;
    }
    let Some(call) = event.tool_call() else {
        return EVENT_HANDLER_CONTINUE;
    };
    let tool_name = call.tool_name.as_str();
    let permissions = permissions();
    let confine = permissions
        .get("confine_paths")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !(GATED_TOOLS.contains(&tool_name) || (confine && PATH_TOOLS.contains(&tool_name))) {
        return EVENT_HANDLER_CONTINUE;
    }
    let Some(runtime) = RUNTIME.get().copied() else {
        // No host runtime captured — fail closed.
        return EVENT_HANDLER_ABORT;
    };
    let args: Value = serde_json::from_str(&call.params).unwrap_or(Value::Null);
    let subject = subject(tool_name, &args);

    if confine && PATH_TOOLS.contains(&tool_name) && !subject.is_empty() && path_escapes_workspace(subject) {
        eprintln!("[pure-permission-gate] {tool_name} escaped the workspace: {subject}");
        return EVENT_HANDLER_ABORT;
    }
    if !GATED_TOOLS.contains(&tool_name) {
        return EVENT_HANDLER_CONTINUE;
    }

    match rule_decision(tool_name, subject, &permissions) {
        Decision::Allow => return EVENT_HANDLER_CONTINUE,
        Decision::Deny => {
            eprintln!("[pure-permission-gate] {tool_name} denied by rule");
            return EVENT_HANDLER_ABORT;
        }
        Decision::Ask => {}
    }

    match confirm(&runtime, &call.tool_call_id, tool_name, &args) {
        Ok(true) => EVENT_HANDLER_CONTINUE,
        Ok(false) => {
            eprintln!("[pure-permission-gate] {tool_name} blocked by user");
            EVENT_HANDLER_ABORT
        }
        Err(error) => {
            eprintln!("[pure-permission-gate] {error}; blocking {tool_name} by default");
            EVENT_HANDLER_ABORT
        }
    }
}
}

#[no_mangle]
// The host resolves this symbol and calls it with the unified-ABI signature; the
// raw pointer is validated by `register_entrypoint` before use.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn rpi_plugin_register(api: *const PluginApi) -> i32 {
    unsafe {
        register_entrypoint(api, |api| {
            // Capture the host callbacks first so the flag can be read during
            // register (the `PluginApi` pointer is only valid for this call).
            let _ = RUNTIME.set(HostRuntime {
                action: api.runtime_action,
                free: api.free_string,
                user_data: api.user_data,
            });

            if let Some(register_flag) = api.register_flag {
                let _ = register_flag(
                    StbStringRef::from_str(BYPASS_FLAG),
                    StbStringRef::from_str(
                        "Skip permission-gate approvals for this process and ACP sessions",
                    ),
                );
                let _ = register_flag(
                    StbStringRef::from_str(GATE_FLAG),
                    StbStringRef::from_str(
                        "Confirm dangerous bash/write/edit tool calls via the host UI dialog",
                    ),
                );
            }

            // Off unless explicitly enabled — a gate can hang a headless run
            // whose client cannot answer UI dialogs.
            if !gate_enabled(
                flag_is_on(get_cli_flag(BYPASS_FLAG).as_ref()),
                flag_is_on(get_cli_flag(GATE_FLAG).as_ref()),
                env_flag(GATE_ENV),
            ) {
                return 0;
            }
            ENABLED.store(true, Ordering::Release);

            match api.register_event_handler {
                Some(register) => register(EventTag::ToolCall, on_tool_call, std::ptr::null_mut()),
                None => 0,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_overrides_both_gate_activation_methods() {
        for flag in [false, true] {
            for environment in [false, true] {
                assert!(!gate_enabled(true, flag, environment));
                assert_eq!(gate_enabled(false, flag, environment), flag || environment);
            }
        }
    }

    #[test]
    fn summarize_covers_gated_tools() {
        assert_eq!(
            summarize("bash", &json!({"command": "rm -rf /"})),
            "rm -rf /"
        );
        assert_eq!(
            summarize("powershell", &json!({"command": "Remove-Item x"})),
            "Remove-Item x"
        );
        assert_eq!(
            summarize("write", &json!({"path": "a.txt", "content": "hi"})),
            "write a.txt (2 bytes)"
        );
        assert_eq!(
            summarize(
                "edit",
                &json!({"path": "a.rs", "edits": [{"oldText":"a","newText":"b"}]})
            ),
            "edit a.rs (1 replacement(s))"
        );
    }

    #[test]
    fn answer_values_reads_yes_no_conventions() {
        assert_eq!(answer_values(&json!({"values": ["Yes"]})), vec!["Yes"]);
        assert_eq!(answer_values(&json!({"value": "No"})), vec!["No"]);
        assert!(answer_values(&json!({})).is_empty());
    }

    /// The ACP bridge only asks the client when the dialog satisfies the SDK's
    /// dialogs contract; otherwise it answers "cancelled" and every gated tool
    /// is silently blocked. Guard that shape here.
    #[test]
    fn permission_ui_satisfies_the_dialogs_contract() {
        let ui = permission_ui("bash", &json!({"command": "rm -rf /"}));
        assert!(rpi_plugin_sdk::dialogs::validate(&ui, Some("tool-1")).is_ok());
        let options = rpi_plugin_sdk::dialogs::permission_options(&ui).unwrap();
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].permission, "allow_once");
        assert_eq!(options[1].permission, "reject_once");
        // The allow title must be the one `confirm` treats as approval.
        assert!(options[0].title.eq_ignore_ascii_case("yes"));
    }

    #[test]
    fn env_flag_truthiness() {
        // Not set.
        assert!(!env_flag("RPI_PERMISSION_GATE_TEST_UNSET"));
    }

    #[test]
    fn deny_wins_over_allow_and_default() {
        let permissions = json!({"tools": {"bash": {
            "default": "allow",
            "allow": ["echo *"],
            "deny": ["rm *"],
        }}});
        assert_eq!(
            rule_decision("bash", "rm -rf /", &permissions),
            Decision::Deny
        );
        assert_eq!(
            rule_decision("bash", "echo hi", &permissions),
            Decision::Allow
        );
        assert_eq!(
            rule_decision("bash", "cargo test", &permissions),
            Decision::Allow
        );
        assert_eq!(rule_decision("write", "a.txt", &permissions), Decision::Ask);
    }

    #[test]
    fn rule_defaults_can_allow_or_deny_without_dialogs() {
        let deny = json!({"tools": {"bash": {"default": "deny"}}});
        assert_eq!(rule_decision("bash", "echo hi", &deny), Decision::Deny);
        let allow = json!({"tools": {"bash": {"default": "allow"}}});
        assert_eq!(rule_decision("bash", "rm -rf /", &allow), Decision::Allow);
        assert_eq!(
            rule_decision("bash", "echo hi", &Value::Null),
            Decision::Ask
        );
    }

    #[test]
    fn file_rules_match_paths_with_globs() {
        let permissions = json!({"tools": {"write": {"deny": ["*.env"], "allow": ["src/**"]}}});
        assert_eq!(rule_decision("write", ".env", &permissions), Decision::Deny);
        assert_eq!(
            rule_decision("write", "src/main.rs", &permissions),
            Decision::Allow
        );
        assert_eq!(
            rule_decision("write", "README.md", &permissions),
            Decision::Ask
        );
    }

    #[test]
    fn confine_paths_rejects_relative_escape() {
        assert!(path_escapes_workspace("../outside.txt"));
        assert!(!path_escapes_workspace("inside.txt"));
    }
}
