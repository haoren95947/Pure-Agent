//! pure-desktop compatibility methods (`_pure/*`) for the ACP server.
//!
//! pure speaks standard ACP v1 for chat, sessions, tools, plans and
//! permissions. Its settings, goal pill, file panes and workspace browser use a
//! small vendor extension namespace instead. The methods here answer that
//! namespace on top of pure's own `models.json`/`auth.json` files; they do not
//! replace any ACP behavior.

use crate::diagln;
use crate::protocol::{ContentBlock, Writer};
use crate::session::Session;
use base64::Engine as _;
use pure_harness::session::jsonl::{
    JsonlSessionCreateOptions, JsonlSessionMetadata, JsonlSessionRepo, JsonlSessionRepoOptions,
};
use pure_harness::session::memory::SystemClock;
use pure_harness::session::types::{ForkOptions, ForkPosition};
use pure_harness::session::DefaultIdGenerator;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Mutex as AsyncMutex;

type Sessions = Arc<AsyncMutex<HashMap<String, Arc<Session>>>>;

/// pure-backend turns this on so the ACP server adds pure's session options,
/// slash-command list and turn updates on top of baseline ACP. Leaving it off
/// keeps the adapter's stock wire behavior for generic ACP clients.
static KAGE_COMPAT: AtomicBool = AtomicBool::new(false);

pub fn enable() {
    KAGE_COMPAT.store(true, Ordering::SeqCst);
    repair_model_costs();
}

/// The kernel requires all four cost rates. A provider saved by an older
/// build (or edited by hand) without them makes every later host start fail,
/// so complete the object once when compatibility turns on.
fn repair_model_costs() {
    let path = models_path();
    if !path.is_file() {
        return;
    }
    let mut file = read_object(&path);
    let Some(providers) = file.get_mut("providers").and_then(Value::as_object_mut) else {
        return;
    };
    let mut changed = false;
    for provider in providers.values_mut() {
        let Some(models) = provider.get_mut("models").and_then(Value::as_array_mut) else {
            continue;
        };
        for model in models {
            let Some(cost) = model.get_mut("cost").and_then(Value::as_object_mut) else {
                continue;
            };
            for key in ["input", "output", "cacheRead", "cacheWrite"] {
                if !cost.contains_key(key) {
                    cost.insert(key.to_owned(), json!(0.0));
                    changed = true;
                }
            }
        }
    }
    if changed {
        write_object(&path, &file);
    }
}

pub fn enabled() -> bool {
    KAGE_COMPAT.load(Ordering::SeqCst)
}

fn cwds() -> &'static Mutex<HashMap<String, PathBuf>> {
    static CWDS: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    CWDS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn kage_states() -> &'static Mutex<HashMap<String, (String, String)>> {
    static STATES: OnceLock<Mutex<HashMap<String, (String, String)>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record the pure-only option values for a session. `configuration.rs`
/// forwards live ACP config updates without a `Session` handle, so it reads
/// them from here when it rebuilds the option list.
pub fn note_state(session_id: &str, goal: Option<&str>, mode: Option<&str>) {
    if let Ok(mut states) = kage_states().lock() {
        let entry = states
            .entry(session_id.to_owned())
            .or_insert_with(|| (String::new(), "default".to_owned()));
        if let Some(goal) = goal {
            entry.0 = goal.trim().to_owned();
        }
        if let Some(mode) = mode {
            entry.1 = if mode == "plan" { "plan" } else { "default" }.to_owned();
        }
    }
    if let Some(goal) = goal {
        let path = session_dir().join("goals.json");
        let mut goals = read_object(&path);
        let goal = goal.trim();
        if goal.is_empty() {
            goals.remove(session_id);
        } else {
            goals.insert(session_id.to_owned(), json!(goal));
        }
        write_object(&path, &goals);
    }
}

/// The goal a session carried before this backend started.
pub fn saved_goal(session_id: &str) -> String {
    read_object(&session_dir().join("goals.json"))
        .get(session_id)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

/// The permission values pure's `mode` option advertises, in show
/// order: the rule-driven default, the legacy plan mode, and the
/// persisted full-access mode.
pub(crate) fn mode_values() -> Value {
    json!([
        {"value": "default", "name": "Default"},
        {"value": "plan", "name": "Plan"},
        {"value": "full", "name": "Full access"}
    ])
}

/// The `mode` select option with `current` selected.
pub(crate) fn mode_option(current: &str) -> Value {
    json!({
        "id": "mode",
        "name": "Mode",
        "type": "select",
        "currentValue": current,
        "options": mode_values(),
    })
}

/// Add the full-access value to a `mode` option that predates it, so a
/// configuration update never drops the choice Settings saved.
pub(crate) fn ensure_full_mode(option: &mut Value) {
    let Some(values) = option.get_mut("options").and_then(Value::as_array_mut) else {
        return;
    };
    if !values.iter().any(|value| value["value"] == "full") {
        values.push(json!({"value": "full", "name": "Full access"}));
    }
}

/// Append pure's `mode` and `goal` options to an ACP config payload.
pub fn augment_options(session_id: &str, config: &mut Value) {
    if !enabled() {
        return;
    }
    let (goal, mode) = kage_states()
        .lock()
        .ok()
        .and_then(|states| states.get(session_id).cloned())
        .unwrap_or_default();
    let Some(options) = config.get_mut("configOptions").and_then(Value::as_array_mut) else {
        return;
    };
    match options.iter_mut().find(|option| option["id"] == "mode") {
        Some(option) => ensure_full_mode(option),
        None => options.push(mode_option(if mode.is_empty() { "default" } else { &mode })),
    }
    if !options.iter().any(|option| option["id"] == "goal") {
        options.push(json!({
            "id": "goal",
            "name": "Goal",
            "type": "text",
            "currentValue": goal,
            "options": []
        }));
    }
}

/// Remember the workspace an ACP session was opened in. `_pure/fs` is rooted
/// at it, exactly like the session's tools are.
pub fn note_session(session_id: &str, cwd: &Path) {
    if let Ok(mut map) = cwds().lock() {
        map.insert(session_id.to_owned(), cwd.to_path_buf());
    }
    if let Ok(mut last) = last_session().lock() {
        *last = Some(session_id.to_owned());
    }
}

/// The newest session, whose pure child read the current models.json.
/// `_pure/models/list` has no session id, and HashMap order is arbitrary.
fn last_session() -> &'static Mutex<Option<String>> {
    static LAST: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

pub fn session_cwd(session_id: &str) -> Option<PathBuf> {
    cwds().lock().ok().and_then(|map| map.get(session_id).cloned())
}

/// Skill slash commands for pure's composer. pure's `get_commands` reports
/// extension commands only; skills live in the resource dirs and are invoked
/// as `/skill:<name>`.
pub fn skill_commands(cwd: Option<&Path>) -> Vec<Value> {
    let mut roots = vec![agent_dir().join("skills")];
    if let Some(cwd) = cwd {
        roots.insert(0, cwd.join(".pure").join("skills"));
    }
    let mut out = Vec::new();
    for root in roots {
        collect_skills(&root, 0, &mut out);
    }
    out
}

fn collect_skills(dir: &Path, depth: usize, out: &mut Vec<Value>) {
    if depth > 3 || out.len() >= 200 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skills(&path, depth + 1, out);
            continue;
        }
        if entry.file_name() != "SKILL.md" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut name = path
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut description = String::new();
        for line in text.lines().take(40) {
            if let Some(value) = line.strip_prefix("name:") {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    name = value.to_owned();
                }
            } else if let Some(value) = line.strip_prefix("description:") {
                description = value.trim().trim_matches('"').to_owned();
            }
        }
        if name.is_empty() {
            continue;
        }
        if !out.iter().any(|entry| entry["name"] == format!("skill:{name}")) {
            out.push(json!({
                "name": format!("skill:{name}"),
                "description": if description.is_empty() { "Invoke this skill".to_owned() } else { description },
                "input": {"hint": "Optional instructions"}
            }));
        }
    }
}

fn agent_dir() -> PathBuf {
    std::env::var_os("PURE_AGENT_DIR")
        .or_else(|| std::env::var_os("RPI_CODING_AGENT_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".pure").join("agent"))
}

fn session_dir() -> PathBuf {
    std::env::var_os("RPI_ACP_SESSION_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| agent_dir().join("acp").join("sessions"))
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn models_path() -> PathBuf {
    agent_dir().join("models.json")
}

fn auth_path() -> PathBuf {
    agent_dir().join("auth.json")
}

fn read_object(path: &Path) -> Map<String, Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Map::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Map::new();
    };
    value.as_object().cloned().unwrap_or_default()
}

fn write_object(path: &Path, value: &Map<String, Value>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(&Value::Object(value.clone())).unwrap_or_default();
    let temp = path.with_extension("json.pure-tmp");
    if std::fs::write(&temp, text).is_ok() {
        let _ = std::fs::rename(&temp, path);
    }
}

/// Session titles written by `_pure/session/rename`, keyed by session id.
fn titles() -> Map<String, Value> {
    read_object(&session_dir().join("titles.json"))
}

/// Every renamed session title, for one `session/list` page.
pub fn session_titles() -> Map<String, Value> {
    if !enabled() {
        return Map::new();
    }
    titles()
}

/// The configured MCP servers as ACP `mcpServers` entries, for sessions the
/// desktop opens. Disabled servers are left out; the settings page keeps them.
pub fn configured_mcp_servers() -> Vec<Value> {
    let servers = read_object(&agent_dir().join("mcp.json"))
        .get("servers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    servers
        .into_iter()
        .filter(|(_, server)| server.get("disabled").and_then(Value::as_bool) != Some(true))
        .filter_map(|(name, server)| {
            let mut entry = Map::new();
            entry.insert("name".to_owned(), json!(name));
            if let Some(url) = server.get("url").and_then(Value::as_str) {
                entry.insert("type".to_owned(), json!("http"));
                entry.insert("url".to_owned(), json!(url));
                entry.insert("headers".to_owned(), pairs(server.get("headers")));
            } else {
                let command = server.get("command").and_then(Value::as_str)?;
                entry.insert("type".to_owned(), json!("stdio"));
                entry.insert("command".to_owned(), json!(command));
                entry.insert(
                    "args".to_owned(),
                    server
                        .get("args")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                );
                entry.insert("env".to_owned(), pairs(server.get("env")));
            }
            Some(Value::Object(entry))
        })
        .collect()
}

/// The names of the MCP servers the settings page would start.
pub fn configured_mcp_names() -> Vec<String> {
    configured_mcp_servers()
        .into_iter()
        .filter_map(|server| {
            server
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

/// A name→value object as the ACP MCP wire's `[{name, value}]` list.
fn pairs(value: Option<&Value>) -> Value {
    let entries = value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(name, value)| json!({"name": name, "value": value}))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(entries)
}

/// The configured servers parsed for a session opened in `cwd`, or the empty
/// list when the client supplied its own servers or compatibility is off.
pub fn fallback_mcp_servers(
    servers: Option<Vec<pure_extcore::external_tools::ToolServer>>,
    cwd: &Path,
) -> Option<Vec<pure_extcore::external_tools::ToolServer>> {
    if !enabled() || servers.as_ref().is_some_and(|servers| !servers.is_empty()) {
        return servers;
    }
    let configured = configured_mcp_servers();
    if configured.is_empty() {
        return servers;
    }
    let mut params = Map::new();
    params.insert("mcpServers".to_owned(), Value::Array(configured));
    match crate::mcp::parse(&params, cwd) {
        Ok(Some(parsed)) => Some(parsed),
        Ok(None) => servers,
        Err(error) => {
            diagln!("[pure-acp] configured MCP servers ignored: {error}");
            None
        }
    }
}

fn providers_map() -> Map<String, Value> {
    read_object(&models_path())
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn auth_map() -> Map<String, Value> {
    read_object(&auth_path())
}

fn api_to_kind(api: &str) -> &'static str {
    match api {
        // pure reads a missing protocol as Anthropic Messages.
        "" | "anthropic-messages" => "anthropic",
        "google-generative-ai" => "gemini",
        _ => "openai",
    }
}

fn kind_to_api(kind: &str) -> &'static str {
    match kind {
        "anthropic" => "anthropic-messages",
        "openai" => "openai-completions",
        // pure parses the name but ships no Gemini transport. Routing a Gemini
        // endpoint through an OpenAI wire would fail later with a confusing
        // error, so the caller rejects it before any file is written.
        _ => "",
    }
}

/// The stored `api` a save keeps when it already speaks `kind`. pure's
/// three kinds are coarser than pure's protocol list: saving an
/// `openai-responses` provider through the openai form must not rewrite
/// it to Chat Completions.
fn api_for_kind(kind: &str, existing: &Value) -> Option<String> {
    let current = existing.get("api").and_then(Value::as_str);
    match kind {
        "anthropic" => Some(kind_to_api("anthropic").to_owned()),
        "openai" => Some(match current {
            Some(
                api @ ("openai-completions"
                | "openai-responses"
                | "openai-codex-responses"
                | "azure-openai-responses"
                | "mistral-conversations"),
            ) => api.to_owned(),
            _ => kind_to_api("openai").to_owned(),
        }),
        _ => None,
    }
}

/// The levels pure treats as available unless its map marks them
/// unsupported. `xhigh` and `max` are the reverse: listed only when the
/// map names them.
const BASE_THINKING_LEVELS: [(&str, &str); 5] = [
    ("off", "none"),
    ("minimal", "minimal"),
    ("low", "low"),
    ("medium", "medium"),
    ("high", "high"),
];

/// pure's `efforts` list for an pure `thinkingLevelMap`. pure accepts only
/// its own effort vocabulary, so pure levels map by name, `off` reads
/// `none`, and `xhigh`/`max` appear only when the map enables them.
fn thinking_map_to_efforts(map: Option<&Value>) -> Option<Value> {
    let map = map?.as_object()?;
    // pure reads an absent base level as supported; only an explicit `null`
    // removes it. `xhigh`/`max` are the reverse: absent hides them.
    let base_supported = |level: &str| !map.get(level).is_some_and(Value::is_null);
    let named = |level: &str| map.get(level).is_some_and(|value| !value.is_null());
    let mut efforts: Vec<Value> = BASE_THINKING_LEVELS
        .iter()
        .filter(|(level, _)| base_supported(level))
        .map(|(_, effort)| json!(effort))
        .collect();
    for level in ["xhigh", "max"] {
        if named(level) {
            efforts.push(json!(level));
        }
    }
    (!efforts.is_empty()).then(|| Value::Array(efforts))
}

/// The pure `thinkingLevelMap` a pure model's `efforts` list describes.
/// The base levels pure did not list are marked unsupported the way pure
/// reads a `null`; `xhigh`/`max` stay absent, which also hides them.
/// `off` stays absent so thinking can always be turned off.
fn efforts_to_thinking_map(efforts: &[Value]) -> Option<Value> {
    let mut map = Map::new();
    let mut listed = false;
    for effort in efforts.iter().filter_map(Value::as_str) {
        let (level, wire) = match effort {
            "none" => ("off", "none"),
            "minimal" | "low" | "medium" | "high" | "xhigh" | "max" => (effort, effort),
            // pure's ladder tops out at `max`; keep pure's wire value.
            "ultra" => ("max", "ultra"),
            _ => continue,
        };
        listed = true;
        map.insert(level.to_owned(), json!(wire));
    }
    if !listed {
        return None;
    }
    for (level, _) in BASE_THINKING_LEVELS {
        if level != "off" && !map.contains_key(level) {
            map.insert(level.to_owned(), Value::Null);
        }
    }
    Some(Value::Object(map))
}

fn model_to_pure(model: &Value) -> Value {    let id = model.get("id").and_then(Value::as_str).unwrap_or("");
    let name = model
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(id);
    let context = model
        .get("contextWindow")
        .or_else(|| model.get("context"))
        .or_else(|| model.get("context_window"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut out = json!({"id": id, "name": name});
    if context > 0 {
        out["context"] = json!(context);
    }
    if let Some(max_output) = model
        .get("maxTokens")
        .or_else(|| model.get("maxOutputTokens"))
        .or_else(|| model.get("max_output"))
        .and_then(Value::as_u64)
    {
        out["max_output"] = json!(max_output);
    }
    if let Some(reasoning) = model.get("reasoning").and_then(Value::as_bool) {
        out["reasoning"] = json!(reasoning);
    }
    if let Some(efforts) = thinking_map_to_efforts(model.get("thinkingLevelMap")) {
        out["efforts"] = efforts;
    }
    if let Some(input) = model.get("input").and_then(Value::as_array) {
        out["input"] = Value::Array(input.clone());
    }
    if let Some(cost) = model.get("cost").and_then(Value::as_object) {
        let mut out_cost = Map::new();
        for key in ["input", "output"] {
            if let Some(value) = cost.get(key) {
                out_cost.insert(key.to_owned(), value.clone());
            }
        }
        for (from, to) in [
            ("cacheRead", "cache_read"),
            ("cache_read", "cache_read"),
            ("cacheWrite", "cache_write"),
            ("cache_write", "cache_write"),
        ] {
            if let Some(value) = cost.get(from) {
                out_cost.insert(to.to_owned(), value.clone());
            }
        }
        if !out_cost.is_empty() {
            out["cost"] = Value::Object(out_cost);
        }
    }
    out
}

fn provider_to_pure(id: &str, provider: &Value) -> Value {
    let api = provider.get("api").and_then(Value::as_str).unwrap_or("");
    let kind = api_to_kind(api);
    let base_url = provider
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or("");
    let name = provider
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(id);
    let api_key = provider.get("apiKey").and_then(Value::as_str).unwrap_or("");
    let api_key_env = api_key
        .strip_prefix('$')
        .map(|value| value.trim_matches(|c: char| c == '{' || c == '}'))
        .unwrap_or("");
    let models = provider
        .get("models")
        .and_then(Value::as_array)
        .map(|models| models.iter().map(model_to_pure).collect::<Vec<_>>())
        .unwrap_or_default();
    let headers = provider
        .get("headers")
        .cloned()
        .unwrap_or_else(|| json!({}));
    json!({
        "kind": kind,
        "base_url": base_url,
        "display_name": name,
        "api_key_env": api_key_env,
        "headers": headers,
        "models": models,
    })
}

fn provider_key_source(id: &str, provider: &Value) -> Value {
    let auth = auth_map();
    let credential = auth
        .get(id)
        .and_then(|entry| entry.get("key"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    if let Some(key) = credential {
        let env = key
            .strip_prefix('$')
            .map(|value| value.trim_matches(|c: char| c == '{' || c == '}'))
            .unwrap_or("");
        return json!({
            "env": env,
            "source": if env.is_empty() { "auth" } else { "env" },
            "name": provider.get("name").cloned().unwrap_or_else(|| json!(id)),
        });
    }
    let declared = provider
        .get("apiKey")
        .and_then(Value::as_str)
        .unwrap_or("");
    let env_name = declared
        .strip_prefix('$')
        .map(|value| value.trim_matches(|c: char| c == '{' || c == '}'))
        .unwrap_or("");
    let conventional = match id {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        _ => "",
    };
    let env_name = if env_name.is_empty() { conventional } else { env_name };
    let present = !env_name.is_empty() && std::env::var_os(env_name).is_some();
    json!({
        "env": env_name,
        "source": if present { "env" } else { "missing" },
        "name": provider.get("name").cloned().unwrap_or_else(|| json!(id)),
    })
}

fn config_get() -> Value {
    let providers = providers_map();
    let mut custom = Map::new();
    let mut keys = Map::new();
    for (id, provider) in &providers {
        // pure has no built-in provider catalog: every configured provider
        // carries its own models, so the client must edit all of them
        // through the custom path that persists models.
        let entry = provider_to_pure(id, provider);
        keys.insert(id.clone(), provider_key_source(id, provider));
        custom.insert(id.clone(), entry);
    }
    let mut top = Map::new();
    if !custom.is_empty() {
        top.insert("custom".to_owned(), Value::Object(custom));
    }
    let mcp = read_object(&agent_dir().join("mcp.json"));
    let permissions = read_object(&agent_dir().join("permissions.json"));
    let acp = read_object(&agent_dir().join("acp.json"));
    let ui = read_object(&agent_dir().join("ui.json"));
    let access = access_mode(&permissions);
    let disabled = disabled_plugin_names();
    let enabled_names: Vec<String> = bundled_plugin_names()
        .into_iter()
        .filter(|name| !disabled.iter().any(|off| off == name))
        .collect();
    json!({
        "providers": Value::Object(top),
        "providerKeys": Value::Object(keys),
        "mcp": {
            "servers": mcp.get("servers").cloned().unwrap_or_else(|| json!({})),
            "allow_sampling": mcp.get("allow_sampling").and_then(Value::as_bool).unwrap_or(false),
        },
        "permissions": {
            "mode": access,
            "confine_paths": permissions.get("confine_paths").and_then(Value::as_bool).unwrap_or(false),
            "tools": permissions.get("tools").cloned().unwrap_or_else(|| json!({})),
            "mcp": permissions.get("mcp").cloned().unwrap_or_else(|| json!({})),
        },
        "plugins": {
            "dir": extensions_dir().map(|dir| dir.to_string_lossy().into_owned()),
            "disabled": disabled,
            "enabled": enabled_names,
            "capabilities": {},
            "config": {},
        },
        "installedPlugins": installed_extensions(),
        "acp": {"agents": acp.get("agents").cloned().unwrap_or_else(|| json!({}))},
        "ui": ui,
    })
}

/// The access mode the user chose in `permissions.json`: `default` or `full`.
fn access_mode(permissions: &Map<String, Value>) -> &'static str {
    match permissions.get("mode").and_then(Value::as_str) {
        Some("full") => "full",
        _ => "default",
    }
}

/// Whether the user chose full access: the permission gate is off and every
/// tool request is approved without a prompt.
pub(crate) fn full_access() -> bool {
    enabled() && access_mode(&read_object(&agent_dir().join("permissions.json"))) == "full"
}

/// Persists the access mode the desktop selected, so every later session
/// spawns with the matching gate policy.
pub(crate) fn set_access_mode(full: bool) {
    let path = agent_dir().join("permissions.json");
    let mut file = read_object(&path);
    file.insert(
        "mode".to_owned(),
        json!(if full { "full" } else { "default" }),
    );
    write_object(&path, &file);
}

/// The display names of the bundled plugins, in the same order the Plugins
/// page lists them.
fn bundled_plugin_names() -> Vec<String> {
    ["goal", "todo", "subagents", "codemode", "permission-gate", "rtk"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// The plugin names the Plugins page shows as turned off.
fn disabled_plugin_names() -> Vec<String> {
    let states = read_object(&agent_dir().join("plugins.json"));
    let mut names: Vec<String> = states
        .iter()
        .filter(|(_, state)| state.get("enabled").and_then(Value::as_bool) == Some(false))
        .map(|(name, _)| {
            // The desktop keys by display name; a hand-edited file may key
            // by the DLL stem. Normalize both to the display name.
            extension_summary(name).0
        })
        .collect();
    names.sort();
    names
}

/// The DLL stems the host must skip, in `PURE_DISABLED_EXTENSIONS` form.
pub(crate) fn disabled_extension_stems() -> Vec<String> {
    disabled_plugin_names()
        .iter()
        .filter_map(|name| extension_stem(name))
        .collect()
}

/// The DLL stem behind a plugin display name.
fn extension_stem(name: &str) -> Option<String> {
    let dir = extensions_dir();
    let has_dll = |stem: &str| {
        dir.as_ref().is_some_and(|dir| {
            dir.join(format!("{stem}{}", std::env::consts::DLL_SUFFIX))
                .is_file()
        })
    };
    if has_dll(name) {
        return Some(name.to_owned());
    }
    let stem = match name {
        "goal" => "pure_goal".to_owned(),
        "todo" => "pure_todo".to_owned(),
        "subagents" => "pure_subagents".to_owned(),
        "codemode" => "pure_codemode".to_owned(),
        "permission-gate" => "pure_permission_gate".to_owned(),
        "rtk" => "pure_rtk".to_owned(),
        other if other.starts_with("pure_") => other.to_owned(),
        other => format!("pure_{}", other.replace('-', "_")),
    };
    if has_dll(&stem) {
        return Some(stem);
    }
    // A third-party plugin keyed by a dashed display name still matches its
    // underscored file.
    let underscored = name.replace('-', "_");
    has_dll(&underscored).then_some(underscored)
}

/// The directory holding the extension DLLs: what the backend set for the
/// pure child, else the `extensions` directory beside the executable.
fn extensions_dir() -> Option<PathBuf> {
    std::env::var_os("RPI_EXTENSIONS_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("extensions")))
                .filter(|dir| dir.is_dir())
        })
}

/// A bundled extension's display name and what it adds to a session.
fn extension_summary(stem: &str) -> (String, Option<&'static str>) {
    match stem {
        "pure_goal" => (
            "goal".to_owned(),
            Some("Session goal, injected into every turn (/goal)"),
        ),
        "pure_todo" => (
            "todo".to_owned(),
            Some("Todo list with live plan updates (/todos)"),
        ),
        "pure_subagents" => (
            "subagents".to_owned(),
            Some("Delegate work to child agents (delegate_task)"),
        ),
        "pure_codemode" => (
            "codemode".to_owned(),
            Some("Run bounded JavaScript programs that call tools (codemode)"),
        ),
        "pure_permission_gate" => (
            "permission-gate".to_owned(),
            Some("Approval prompts and rules before tool calls"),
        ),
        "pure_rtk" => (
            "rtk".to_owned(),
            Some("Compress shell output with the bundled rtk binary"),
        ),
        other => (other.replace('_', "-"), None),
    }
}

/// The extension DLLs in [`extensions_dir`], as the Plugins page lists them.
fn installed_extensions() -> Value {
    let Some(dir) = extensions_dir() else {
        return json!([]);
    };
    let disabled = disabled_extension_stems();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        })
        .collect();
    files.sort();
    let rows: Vec<Value> = files
        .iter()
        .filter_map(|path| {
            let stem = path.file_stem()?.to_string_lossy().into_owned();
            let (name, about) = extension_summary(&stem);
            let enabled = !disabled
                .iter()
                .any(|off| off.eq_ignore_ascii_case(&stem));
            Some(json!({
                "name": name,
                "enabled": enabled,
                "description": about,
            }))
        })
        .collect();
    Value::Array(rows)
}

fn kage_provider_to_rpi(existing: &Value, entry: &Value) -> Result<Value, String> {
    let mut provider = existing.as_object().cloned().unwrap_or_default();
    let api = match entry.get("kind").and_then(Value::as_str) {
        Some(kind) => Some(api_for_kind(kind, existing).ok_or_else(|| {
            format!(
                "provider kind `{kind}` is not supported by the pure kernel; use anthropic or openai"
            )
        })?),
        // A registered provider's form sends only its overrides, so the
        // protocol the file already records stands.
        None => provider
            .get("api")
            .and_then(Value::as_str)
            .filter(|api| !api.is_empty())
            .map(str::to_owned),
    };
    match api {
        Some(api) => {
            provider.insert("api".to_owned(), json!(api));
        }
        None => {
            provider.remove("api");
        }
    }
    if let Some(base_url) = entry.get("base_url").and_then(Value::as_str) {
        if !base_url.trim().is_empty() {
            provider.insert("baseUrl".to_owned(), json!(base_url.trim()));
        }
    }
    match entry.get("display_name").and_then(Value::as_str) {
        Some(name) if !name.trim().is_empty() => {
            provider.insert("name".to_owned(), json!(name.trim()));
        }
        Some(_) => {
            provider.remove("name");
        }
        None => {}
    }
    if let Some(env_value) = entry.get("api_key_env") {
        let env = env_value.as_str().unwrap_or("").trim();
        if env.is_empty() {
            // pure's "None" or a key held in auth.json: models.json must not
            // point at a stale environment variable.
            provider.remove("apiKey");
        } else {
            provider.insert("apiKey".to_owned(), json!(format!("${env}")));
        }
    } else {
        // A key pasted into pure's form lives in auth.json.
        provider.remove("apiKey");
    }
    let saved_headers = provider.get("headers").and_then(Value::as_object).cloned();
    if let Some(headers) = entry.get("headers").and_then(Value::as_object) {
        let mut clean = Map::new();
        for (name, value) in headers {
            if value.as_str() == Some("<redacted>") {
                if let Some(old) = saved_headers.as_ref().and_then(|old| old.get(name)) {
                    clean.insert(name.clone(), old.clone());
                }
            } else {
                clean.insert(name.clone(), value.clone());
            }
        }
        if clean.is_empty() {
            provider.remove("headers");
        } else {
            provider.insert("headers".to_owned(), Value::Object(clean));
        }
    } else {
        provider.remove("headers");
    }
    if let Some(models) = entry.get("models").and_then(Value::as_array) {
        let saved = provider.get("models").and_then(Value::as_array).cloned();
        let mapped: Vec<Value> = models
            .iter()
            .filter_map(|model| {
                let id = model.get("id").and_then(Value::as_str)?;
                // Keep the pure-only fields (compat, baseUrl, headers, ...)
                // of a model the file already describes; pure's form does
                // not model them.
                let mut out = saved
                    .as_ref()
                    .and_then(|saved| {
                        saved
                            .iter()
                            .find(|saved| saved.get("id").and_then(Value::as_str) == Some(id))
                    })
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                out.insert("id".to_owned(), json!(id));
                match model.get("name").and_then(Value::as_str) {
                    Some(name) if !name.is_empty() => {
                        out.insert("name".to_owned(), json!(name));
                    }
                    _ => {
                        out.remove("name");
                    }
                }
                let context = model
                    .get("contextWindow")
                    .or_else(|| model.get("context"))
                    .and_then(Value::as_u64);
                match context {
                    Some(context) => {
                        out.insert("contextWindow".to_owned(), json!(context));
                    }
                    None => {
                        out.remove("contextWindow");
                        out.remove("context");
                    }
                }
                let max_tokens = model
                    .get("maxTokens")
                    .or_else(|| model.get("max_output"))
                    .or_else(|| model.get("maxOutputTokens"))
                    .and_then(Value::as_u64);
                match max_tokens {
                    Some(max_tokens) => {
                        out.insert("maxTokens".to_owned(), json!(max_tokens));
                    }
                    None => {
                        out.remove("maxTokens");
                        out.remove("max_output");
                    }
                }
                match model.get("reasoning").and_then(Value::as_bool) {
                    Some(false) => {
                        out.insert("reasoning".to_owned(), json!(false));
                        out.remove("thinkingLevelMap");
                    }
                    Some(true) => {
                        out.insert("reasoning".to_owned(), json!(true));
                    }
                    None => {
                        out.remove("reasoning");
                    }
                }
                if let Some(efforts) = model.get("efforts").and_then(Value::as_array) {
                    match efforts_to_thinking_map(efforts) {
                        Some(map) => {
                            out.insert("thinkingLevelMap".to_owned(), map);
                        }
                        None => {
                            out.remove("thinkingLevelMap");
                        }
                    }
                }
                match model.get("input").and_then(Value::as_array) {
                    Some(input) => {
                        out.insert("input".to_owned(), Value::Array(input.clone()));
                    }
                    None => {
                        out.remove("input");
                    }
                }
                if let Some(cost) = model.get("cost").and_then(Value::as_object) {
                    // The kernel's cost shape requires all four rates; a form
                    // that leaves cache pricing blank would otherwise make
                    // models.json unreadable on the next host start.
                    let mut out_cost = Map::new();
                    for (key, aliases) in [
                        ("input", &["input"][..]),
                        ("output", &["output"][..]),
                        (
                            "cacheRead",
                            &["cacheRead", "cache_read"][..],
                        ),
                        (
                            "cacheWrite",
                            &["cacheWrite", "cache_write"][..],
                        ),
                    ] {
                        let value = aliases
                            .iter()
                            .find_map(|alias| cost.get(*alias))
                            .cloned()
                            .unwrap_or_else(|| json!(0.0));
                        out_cost.insert(key.to_owned(), value);
                    }
                    out.insert("cost".to_owned(), Value::Object(out_cost));
                } else {
                    out.remove("cost");
                }
                Some(Value::Object(out))
            })
            .collect();
        if !mapped.is_empty() {
            provider.insert("models".to_owned(), Value::Array(mapped));
        }
    }
    Ok(Value::Object(provider))
}

fn config_set(params: &Value) -> Result<Value, String> {
    let path: Vec<String> = params
        .get("path")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let value = params.get("value").cloned().unwrap_or(Value::Null);
    let section = path.first().map(String::as_str).unwrap_or("");
    match section {
        "providers" => {
            let (id, entry) = if path.get(1).map(String::as_str) == Some("custom") {
                (path.get(2).cloned(), value)
            } else {
                (path.get(1).cloned(), value)
            };
            let id = id.ok_or_else(|| "providers config path needs a provider id".to_string())?;
            let mut file = read_object(&models_path());
            let mut providers = file
                .remove("providers")
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            if entry.is_null() {
                providers.remove(&id);
            } else {
                let existing = providers.get(&id).cloned().unwrap_or_else(|| json!({}));
                let provider = kage_provider_to_rpi(&existing, &entry)?;
                providers.insert(id, provider);
            }
            file.insert("providers".to_owned(), Value::Object(providers));
            write_object(&models_path(), &file);
        }
        "ui" => {
            let mut file = read_object(&agent_dir().join("ui.json"));
            if let Some(key) = path.get(1) {
                if value.is_null() {
                    file.remove(key);
                } else {
                    file.insert(key.clone(), value);
                }
                write_object(&agent_dir().join("ui.json"), &file);
            }
        }
        "mcp" => {
            let mut file = read_object(&agent_dir().join("mcp.json"));
            let mut servers = file
                .remove("servers")
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            if let Some(name) = path.get(2) {
                if value.is_null() {
                    servers.remove(name);
                } else {
                    servers.insert(name.clone(), value);
                }
                file.insert("servers".to_owned(), Value::Object(servers));
                write_object(&agent_dir().join("mcp.json"), &file);
            }
        }
        "permissions" | "plugins" | "acp" => {
            let path_file = agent_dir().join(format!("{section}.json"));
            let mut file = read_object(&path_file);
            set_path(&mut file, &path[1..], value);
            write_object(&path_file, &file);
        }
        _ => return Err(format!("unknown config section `{section}`")),
    }
    Ok(config_get())
}

fn set_path(root: &mut Map<String, Value>, path: &[String], value: Value) {
    let Some(first) = path.first() else {
        return;
    };
    if path.len() == 1 {
        if value.is_null() {
            root.remove(first);
        } else {
            root.insert(first.clone(), value);
        }
        return;
    }
    let slot = root.entry(first.clone()).or_insert_with(|| json!({}));
    if !slot.is_object() {
        *slot = json!({});
    }
    if let Some(object) = slot.as_object_mut() {
        set_path(object, &path[1..], value);
    }
}

fn auth_set(params: &Value) -> Result<Value, String> {
    let provider = params
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(|| "provider is required".to_string())?;
    let key = params.get("key").and_then(Value::as_str).unwrap_or("");
    let mut auth = auth_map();
    if key.trim().is_empty() {
        auth.remove(provider);
    } else {
        auth.insert(
            provider.to_owned(),
            json!({"type": "api_key", "key": key.trim()}),
        );
    }
    write_object(&auth_path(), &auth);
    Ok(json!({}))
}

fn resolve_api_key(id: &str, explicit: &str, env_name: &str) -> String {
    if !explicit.trim().is_empty() {
        return explicit.trim().to_owned();
    }
    if !env_name.trim().is_empty() {
        if let Ok(key) = std::env::var(env_name) {
            if !key.trim().is_empty() {
                return key;
            }
        }
    }
    if let Some(key) = auth_map()
        .get(id)
        .and_then(|entry| entry.get("key"))
        .and_then(Value::as_str)
    {
        if let Some(var) = key.strip_prefix('$') {
            let var = var.trim_matches(|c: char| c == '{' || c == '}');
            return std::env::var(var).unwrap_or_default();
        }
        if !key.is_empty() {
            return key.to_owned();
        }
    }
    providers_map()
        .get(id)
        .and_then(|provider| provider.get("apiKey"))
        .and_then(Value::as_str)
        .map(|key| {
            if let Some(var) = key.strip_prefix('$') {
                let var = var.trim_matches(|c: char| c == '{' || c == '}');
                std::env::var(var).unwrap_or_default()
            } else {
                key.to_owned()
            }
        })
        .unwrap_or_default()
}

async fn config_test(params: &Value) -> Result<Value, String> {
    let probe = params
        .get("provider")
        .cloned()
        .ok_or_else(|| "only provider probes are supported".to_string())?;
    let id = probe.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
    if id.is_empty() {
        return Err("provider.id is required".to_string());
    }
    let kind = probe.get("kind").and_then(Value::as_str).map(str::to_owned);
    let base_url = probe
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            providers_map()
                .get(&id)
                .and_then(|provider| provider.get("baseUrl"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let kind = kind.unwrap_or_else(|| {
        providers_map()
            .get(&id)
            .and_then(|provider| provider.get("api"))
            .and_then(Value::as_str)
            .map(api_to_kind)
            .unwrap_or("openai")
            .to_owned()
    });
    if base_url.trim().is_empty() {
        return Ok(json!({
            "ok": false,
            "message": "provider has no base URL",
            "millis": 0,
            "models": [],
        }));
    }
    if kind == "gemini" {
        return Ok(json!({
            "ok": false,
            "message": "the pure kernel has no gemini transport; use anthropic or an openai-compatible endpoint",
            "millis": 0,
            "models": [],
        }));
    }
    let env_declared = probe.get("apiKeyEnv").and_then(Value::as_str);
    let explicit = probe.get("apiKey").and_then(Value::as_str).unwrap_or("");
    // An explicitly empty variable is pure's spelling for an endpoint
    // that needs no key, such as a local model server.
    let keyless = explicit.trim().is_empty() && env_declared == Some("");
    let key = if keyless {
        String::new()
    } else {
        resolve_api_key(&id, explicit, env_declared.unwrap_or(""))
    };
    if key.is_empty() && !keyless {
        let message = match env_declared.unwrap_or("") {
            "" => "no API key configured (auth.json or environment)".to_owned(),
            env => format!("no key: {env} is not set and no key is saved for {id}"),
        };
        return Ok(json!({
            "ok": false,
            "message": message,
            "millis": 0,
            "models": [],
        }));
    }
    let url = match probe
        .get("modelsUrl")
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())
    {
        Some(url) if url.starts_with("http://") || url.starts_with("https://") => {
            url.trim().to_owned()
        }
        Some(url) => {
            return Ok(json!({
                "ok": false,
                "message": format!("{url} is not an http(s) URL"),
                "millis": 0,
                "models": [],
            }));
        }
        None => models_url(base_url.trim(), &kind),
    };
    // The key and headers belong to the provider's endpoint; a models URL
    // on another host reads them anonymously.
    let same_origin = origin(&url) == origin(base_url.trim());
    let saved_headers = providers_map()
        .get(&id)
        .and_then(|provider| provider.get("headers"))
        .and_then(Value::as_object)
        .cloned();
    let mut headers = Map::new();
    if let Some(map) = probe.get("headers").and_then(Value::as_object) {
        for (name, value) in map {
            let text = if value.as_str() == Some("<redacted>") {
                saved_headers
                    .as_ref()
                    .and_then(|saved| saved.get(name))
                    .and_then(Value::as_str)
            } else {
                value.as_str()
            };
            if let Some(text) = text {
                headers.insert(name.clone(), json!(text));
            }
        }
    }
    let sent_key = !key.is_empty();
    let result = tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build();
        let client = match client {
            Ok(client) => client,
            Err(error) => return (url, 0, Err(error.to_string())),
        };
        let mut request = client.get(&url);
        if same_origin && !key.is_empty() {
            if kind == "anthropic" {
                request = request.header("x-api-key", key).header(
                    "anthropic-version",
                    "2023-06-01",
                );
            } else {
                request = request.bearer_auth(key);
            }
        }
        if same_origin {
            for (name, value) in headers {
                if let Some(text) = value.as_str() {
                    request = request.header(name, text);
                }
            }
        }
        let response = request.send();
        let millis = started.elapsed().as_millis() as u64;
        match response {
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response.text().unwrap_or_default();
                (url, millis, Ok((status, body)))
            }
            Err(error) => (url, millis, Err(error.to_string())),
        }
    })
    .await
    .map_err(|error| error.to_string())?;
    let (url, millis, outcome) = result;
    match outcome {
        Ok((status, body)) => {
            let models = probe_models(&body);
            let hint = if !sent_key && matches!(status, 401 | 403) {
                " (no API key was sent)"
            } else {
                ""
            };
            Ok(json!({
                "ok": (200..300).contains(&status),
                "status": status,
                "message": format!("GET {url} -> {status} in {millis}ms{hint}"),
                "millis": millis,
                "models": models,
            }))
        }
        Err(error) => Ok(json!({
            "ok": false,
            "message": format!("GET {url} failed: {error}"),
            "millis": millis,
            "models": [],
        })),
    }
}

fn models_url(base: &str, kind: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/models") {
        return base.to_owned();
    }
    if kind == "anthropic" {
        format!("{base}/v1/models")
    } else if base.ends_with("/v1") || base.contains("/v1/") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

/// The `host:port` authority of an http(s) URL, for the same-origin check
/// that keeps a key off hosts the provider does not own.
fn origin(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split('/').next()?;
    (!authority.is_empty()).then_some(authority)
}

fn probe_models(body: &str) -> Vec<Value> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let array = value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.get("models").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default();
    array
        .iter()
        .filter_map(|model| {
            let id = model
                .get("id")
                .or_else(|| model.get("name"))
                .and_then(Value::as_str)?;
            let mut out = json!({"id": id, "name": id});
            if let Some(name) = model
                .get("display_name")
                .or_else(|| model.get("displayName"))
                .or_else(|| model.get("name"))
                .and_then(Value::as_str)
            {
                out["name"] = json!(name);
            }
            if let Some(context) = model
                .get("context_window")
                .or_else(|| model.get("context_length"))
                .or_else(|| model.get("max_input_tokens"))
                .and_then(Value::as_u64)
            {
                out["context"] = json!(context);
            }
            if let Some(output) = model
                .get("max_output_tokens")
                .or_else(|| model.get("max_tokens"))
                .and_then(Value::as_u64)
            {
                out["maxOutput"] = json!(output);
            }
            Some(out)
        })
        .collect()
}

/// models.dev's catalog is what pure reads when a request names no URL.
const MODELS_DEV: &str = "https://models.dev/api.json";

async fn providers_directory(params: &Value) -> Result<Value, String> {
    let requested = params.get("url").and_then(Value::as_str).unwrap_or("");
    let url = if requested.trim().is_empty() {
        MODELS_DEV.to_owned()
    } else {
        requested.trim().to_owned()
    };
    let api_key = params
        .get("apiKey")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let body = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|error| error.to_string())?;
        let mut request = client.get(&url);
        if !api_key.is_empty() {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().map_err(|error| error.to_string())?;
        let status = response.status();
        let text = response.text().unwrap_or_default();
        if !status.is_success() {
            return Err(format!("directory request -> {status}"));
        }
        Ok::<String, String>(text)
    })
    .await
    .map_err(|error| error.to_string())??;
    let value: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let providers = directory_providers(&value);
    if providers.is_empty() {
        return Err("the directory lists no providers with tool-calling models".to_owned());
    }
    Ok(json!({"providers": providers}))
}

fn directory_providers(value: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    // models.dev's api.json roots the provider map directly; the other
    // accepted spelling wraps it in `providers`.
    let map = value
        .get("providers")
        .filter(|providers| providers.is_object() || providers.is_array());
    match map {
        Some(Value::Array(array)) => {
            for provider in array {
                out.push(directory_provider(provider));
            }
        }
        Some(Value::Object(map)) => {
            for (id, provider) in map {
                let mut entry = directory_provider(provider);
                entry["id"] = json!(id);
                if entry.get("name").and_then(Value::as_str).unwrap_or("").is_empty() {
                    entry["name"] = json!(id);
                }
                out.push(entry);
            }
        }
        _ => {
            if let Some(map) = value.as_object() {
                for (id, provider) in map {
                    if provider.get("models").is_none() {
                        continue;
                    }
                    let mut entry = directory_provider(provider);
                    entry["id"] = json!(id);
                    if entry.get("name").and_then(Value::as_str).unwrap_or("").is_empty() {
                        entry["name"] = json!(id);
                    }
                    out.push(entry);
                }
            }
        }
    }
    out.retain(|provider| {
        provider
            .get("models")
            .and_then(Value::as_array)
            .is_some_and(|models| !models.is_empty())
    });
    out.sort_by_key(|provider| {
        provider
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase()
    });
    out
}

fn directory_model(model: &Value, id: &str) -> Value {
    let name = model.get("name").and_then(Value::as_str).unwrap_or(id);
    let mut out = json!({"id": id, "name": name});
    let context = model
        .get("context")
        .or_else(|| model.get("context_window"))
        .or_else(|| model.get("limit").and_then(|limit| limit.get("context")))
        .and_then(Value::as_u64);
    if let Some(context) = context {
        out["context"] = json!(context);
    }
    let output = model
        .get("output")
        .or_else(|| model.get("max_output"))
        .or_else(|| model.get("limit").and_then(|limit| limit.get("output")))
        .and_then(Value::as_u64);
    if let Some(output) = output {
        out["output"] = json!(output);
    }
    if let Some(reasoning) = model.get("reasoning").and_then(Value::as_bool) {
        out["reasoning"] = json!(reasoning);
    }
    let input = model
        .get("input")
        .or_else(|| model.get("modalities").and_then(|modalities| modalities.get("input")));
    if let Some(input) = input.and_then(Value::as_array) {
        out["input"] = Value::Array(input.clone());
    }
    let produces = model
        .get("produces")
        .or_else(|| model.get("modalities").and_then(|modalities| modalities.get("output")));
    if let Some(produces) = produces.and_then(Value::as_array) {
        out["produces"] = Value::Array(produces.clone());
    }
    if let Some(cost) = model.get("cost").and_then(Value::as_object) {
        let mut out_cost = Map::new();
        for key in ["input", "output"] {
            if let Some(value) = cost.get(key) {
                out_cost.insert(key.to_owned(), value.clone());
            }
        }
        for (from, to) in [
            ("cache_read", "cacheRead"),
            ("cacheRead", "cacheRead"),
            ("cache_write", "cacheWrite"),
            ("cacheWrite", "cacheWrite"),
        ] {
            if let Some(value) = cost.get(from) {
                out_cost.insert(to.to_owned(), value.clone());
            }
        }
        if !out_cost.is_empty() {
            out["cost"] = Value::Object(out_cost);
        }
    }
    out
}

fn directory_provider(provider: &Value) -> Value {
    let id = provider.get("id").and_then(Value::as_str).unwrap_or("");
    let name = provider
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(id);
    let api = provider.get("api").and_then(Value::as_str);
    let env = match provider.get("env") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(|value| json!(value))
            .collect::<Vec<_>>(),
        Some(Value::String(value)) => vec![json!(value)],
        _ => Vec::new(),
    };
    let kind = provider
        .get("kind")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            // models.dev names the protocol by its AI-SDK package.
            provider
                .get("npm")
                .and_then(Value::as_str)
                .and_then(npm_kind)
                .map(str::to_owned)
        });
    // api.json marks each model that can call tools; any predicate counts
    // when the payload omits the flag entirely.
    let models_dev_shape = provider
        .get("models")
        .and_then(Value::as_object)
        .is_some_and(|models| models.values().any(|model| model.get("tool_call").is_some()));
    let mut models = match provider.get("models") {
        Some(Value::Array(models)) => models
            .iter()
            .filter(|model| !models_dev_shape || model.get("tool_call").and_then(Value::as_bool) == Some(true))
            .filter_map(|model| {
                let id = model.get("id").and_then(Value::as_str)?;
                Some(directory_model(model, id))
            })
            .collect::<Vec<_>>(),
        Some(Value::Object(models)) => models
            .iter()
            .filter(|(_, model)| {
                !models_dev_shape || model.get("tool_call").and_then(Value::as_bool) == Some(true)
            })
            .map(|(id, model)| directory_model(model, id))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    models.sort_by_key(|model| {
        model
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase()
    });
    let mut out = json!({"id": id, "name": name, "env": env, "models": models});
    if let Some(api) = api {
        out["api"] = json!(api);
    }
    if let Some(kind) = kind {
        out["kind"] = json!(kind);
    }
    out
}

fn npm_kind(npm: &str) -> Option<&'static str> {
    match npm {
        "@ai-sdk/openai-compatible" | "@ai-sdk/openai" => Some("openai"),
        "@ai-sdk/anthropic" => Some("anthropic"),
        "@ai-sdk/google" => Some("gemini"),
        _ => None,
    }
}

async fn models_list(sessions: &Sessions) -> Value {
    let session = {
        let map = sessions.lock().await;
        let newest = last_session()
            .lock()
            .ok()
            .and_then(|last| last.as_ref().and_then(|id| map.get(id).cloned()));
        newest.or_else(|| map.values().next().cloned())
    };
    if let Some(session) = session {
        if let Ok(models) = session.available_models().await {
            let array = models
                .get("models")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if !array.is_empty() {
                return json!({"providers": group_available_models(&array)});
            }
        }
    }
    json!({"providers": group_configured_models()})
}

fn group_available_models(models: &[Value]) -> Vec<Value> {
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    for model in models {
        let id = model.get("id").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let provider = model.get("provider").and_then(Value::as_str).unwrap_or("");
        let name = model.get("name").and_then(Value::as_str).unwrap_or(id);
        let mut entry = json!({
            "id": if provider.is_empty() { id.to_owned() } else { format!("{provider}/{id}") },
            "name": name,
            "images": model.get("input").map(|input| input.to_string().contains("image")).unwrap_or(false),
        });
        if let Some(context) = model
            .get("contextWindow")
            .or_else(|| model.get("context"))
            .and_then(Value::as_u64)
        {
            entry["context"] = json!(context);
        }
        match groups.iter_mut().find(|(group, _)| group == provider) {
            Some((_, list)) => list.push(entry),
            None => groups.push((provider.to_owned(), vec![entry])),
        }
    }
    groups
        .into_iter()
        .map(|(provider, models)| {
            json!({
                "id": if provider.is_empty() { "other".to_owned() } else { provider.clone() },
                "name": if provider.is_empty() { "Other".to_owned() } else { provider },
                "models": models,
            })
        })
        .collect()
}

fn group_configured_models() -> Vec<Value> {
    providers_map()
        .into_iter()
        .map(|(id, provider)| {
            let name = provider
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .to_owned();
            let models = provider
                .get("models")
                .and_then(Value::as_array)
                .map(|models| {
                    models
                        .iter()
                        .filter_map(|model| {
                            let model_id = model.get("id").and_then(Value::as_str)?;
                            let mut entry = json!({
                                "id": format!("{id}/{model_id}"),
                                "name": model.get("name").and_then(Value::as_str).unwrap_or(model_id),
                            });
                            if let Some(context) = model
                                .get("contextWindow")
                                .and_then(Value::as_u64)
                            {
                                entry["context"] = json!(context);
                            }
                            Some(entry)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            json!({"id": id, "name": name, "models": models})
        })
        .collect()
}

/// The engine settings the desktop's Agent & Sessions page can edit, over
/// `agent/settings.json`.
fn options_list() -> Vec<Value> {
    let settings = read_object(&agent_dir().join("settings.json"));
    let thinking = settings
        .get("defaultThinkingLevel")
        .and_then(Value::as_str)
        .unwrap_or("medium");
    let reserve = settings
        .get("compaction")
        .and_then(|compaction| compaction.get("reserveTokens"))
        .and_then(Value::as_i64)
        .unwrap_or(16384);
    let retries = settings
        .get("retry")
        .and_then(|retry| retry.get("maxRetries"))
        .and_then(Value::as_i64)
        .unwrap_or(10);
    let subagents = settings.get("subagents");
    let subagent_turns = subagents
        .and_then(|value| value.get("maxTurns"))
        .and_then(Value::as_i64)
        .unwrap_or(8);
    let subagent_timeout = subagents
        .and_then(|value| value.get("timeoutSeconds"))
        .and_then(Value::as_i64)
        .unwrap_or(300);
    let subagent_running = subagents
        .and_then(|value| value.get("maxRunning"))
        .and_then(Value::as_i64)
        .unwrap_or(16);
    vec![
        json!({
            "name": "thinking_level",
            "toml": "defaultThinkingLevel",
            "doc": "Thinking effort for a new session.",
            "kind": "choice",
            "values": ["off", "minimal", "low", "medium", "high", "xhigh", "max"],
            "default": "medium",
            "value": thinking,
            "configured": settings.contains_key("defaultThinkingLevel"),
            "live": false,
        }),
        json!({
            "name": "compaction_threshold",
            "toml": "compaction.reserveTokens",
            "doc": "Context tokens kept free when the engine compacts older turns.",
            "kind": "int",
            "min": 0,
            "max": 1_000_000,
            "default": 16384,
            "value": reserve,
            "configured": settings
                .get("compaction")
                .is_some_and(|compaction| compaction.get("reserveTokens").is_some()),
            "live": false,
        }),
        json!({
            "name": "max_provider_retries",
            "toml": "retry.maxRetries",
            "doc": "Retries for a transient provider failure.",
            "kind": "int",
            "min": 0,
            "max": 20,
            "default": 10,
            "value": retries,
            "configured": settings
                .get("retry")
                .is_some_and(|retry| retry.get("maxRetries").is_some()),
            "live": false,
        }),
        json!({
            "name": "agent_max_running",
            "toml": "subagents.maxRunning",
            "doc": "Child agents one session may run at once.",
            "kind": "int",
            "min": 1,
            "max": 16,
            "default": 16,
            "value": subagent_running,
            "configured": subagents
                .is_some_and(|value| value.get("maxRunning").is_some()),
            "live": false,
        }),
        json!({
            "name": "agent_max_turns",
            "toml": "subagents.maxTurns",
            "doc": "Per-child turn budget when the tool call names none.",
            "kind": "int",
            "min": 1,
            "max": 32,
            "default": 8,
            "value": subagent_turns,
            "configured": subagents
                .is_some_and(|value| value.get("maxTurns").is_some()),
            "live": false,
        }),
        json!({
            "name": "agent_timeout",
            "toml": "subagents.timeoutSeconds",
            "doc": "Per-child timeout in seconds when the tool call names none.",
            "kind": "int",
            "min": 10,
            "max": 1800,
            "default": 300,
            "value": subagent_timeout,
            "configured": subagents
                .is_some_and(|value| value.get("timeoutSeconds").is_some()),
            "live": false,
        }),
    ]
}

fn options_set(params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "name is required".to_string())?;
    let value = params.get("value").cloned().unwrap_or(Value::Null);
    let path = agent_dir().join("settings.json");
    let mut settings = read_object(&path);
    match name {
        "thinking_level" => {
            let level = value
                .as_str()
                .filter(|level| {
                    matches!(
                        *level,
                        "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                    )
                })
                .ok_or_else(|| "invalid thinking level".to_string())?;
            settings.insert("defaultThinkingLevel".to_owned(), json!(level));
        }
        "compaction_threshold" => {
            let tokens = value
                .as_i64()
                .filter(|tokens| (0..=1_000_000).contains(tokens))
                .ok_or_else(|| "invalid compaction threshold".to_string())?;
            let slot = settings
                .entry("compaction".to_owned())
                .or_insert_with(|| json!({}));
            if !slot.is_object() {
                *slot = json!({});
            }
            slot["reserveTokens"] = json!(tokens);
        }
        "max_provider_retries" => {
            let retries = value
                .as_i64()
                .filter(|retries| (0..=20).contains(retries))
                .ok_or_else(|| "invalid retry count".to_string())?;
            let slot = settings
                .entry("retry".to_owned())
                .or_insert_with(|| json!({}));
            if !slot.is_object() {
                *slot = json!({});
            }
            slot["maxRetries"] = json!(retries);
        }
        "agent_max_running" | "agent_max_turns" | "agent_timeout" => {
            let (key, low, high, name_for_error) = match name {
                "agent_max_running" => ("maxRunning", 1, 16, "max running children"),
                "agent_max_turns" => ("maxTurns", 1, 32, "child turn budget"),
                _ => ("timeoutSeconds", 10, 1800, "child timeout"),
            };
            let number = value
                .as_i64()
                .filter(|number| (low..=high).contains(number))
                .ok_or_else(|| format!("invalid {name_for_error}"))?;
            let slot = settings
                .entry("subagents".to_owned())
                .or_insert_with(|| json!({}));
            if !slot.is_object() {
                *slot = json!({});
            }
            slot[key] = json!(number);
        }
        _ => return Err(format!("unknown engine option `{name}`")),
    }
    write_object(&path, &settings);
    Ok(json!({"options": options_list()}))
}

fn folders(params: &Value) -> Result<Value, String> {
    let requested = params.get("path").and_then(Value::as_str).unwrap_or("");
    let home = home_dir();
    let path = if requested.trim().is_empty() {
        home.clone()
    } else if let Some(rest) = requested.strip_prefix('~') {
        home.join(rest.trim_start_matches(['/', '\\']))
    } else {
        PathBuf::from(requested)
    };
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    let parent = path
        .parent()
        .filter(|parent| *parent != path)
        .map(|parent| parent.display().to_string());
    let mut folders = Vec::new();
    let mut skipped = 0usize;
    if let Ok(entries) = std::fs::read_dir(&path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                skipped += 1;
                continue;
            }
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                folders.push(name);
            }
        }
    }
    folders.sort();
    let truncated = folders.len() > 500;
    folders.truncate(500);
    Ok(json!({
        "path": path.display().to_string(),
        "parent": parent,
        "home": home.display().to_string(),
        "folders": folders,
        "truncated": truncated,
        "skipped": skipped,
    }))
}

async fn fs_op(_sessions: &Sessions, params: &Value) -> Result<Value, String> {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let cwd = cwds()
        .lock()
        .ok()
        .and_then(|map| map.get(session_id).cloned())
        .unwrap_or_else(home_dir);
    let op = params.get("op").and_then(Value::as_str).unwrap_or("list");
    let relative = params.get("path").and_then(Value::as_str).unwrap_or("");
    let target = cwd.join(relative.trim_start_matches(['/', '\\']));
    match op {
        "read" => {
            let bytes = std::fs::read(&target).map_err(|error| error.to_string())?;
            let cap = 512 * 1024;
            let truncated = bytes.len() > cap;
            let slice = &bytes[..bytes.len().min(cap)];
            match std::str::from_utf8(slice) {
                Ok(text) => Ok(json!({"op": "read", "content": text, "truncated": truncated, "binary": false})),
                Err(_) => Ok(json!({"op": "read", "content": "", "truncated": truncated, "binary": true})),
            }
        }
        _ => {
            let mut entries = Vec::new();
            walk(&cwd, &target, &mut entries, 0, 500);
            Ok(json!({"op": "list", "entries": entries, "truncated": entries.len() >= 500}))
        }
    }
}

fn walk(root: &Path, dir: &Path, entries: &mut Vec<Value>, depth: usize, cap: usize) {
    if depth > 2 || entries.len() >= cap {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<_> = read.flatten().collect();
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        if entries.len() >= cap {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let kind = if metadata.is_dir() { "directory" } else if metadata.is_file() { "file" } else { "other" };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");
        entries.push(json!({
            "path": relative,
            "kind": kind,
            "size": if metadata.is_file() { metadata.len() } else { 0 },
        }));
        if metadata.is_dir() {
            walk(root, &path, entries, depth + 1, cap);
        }
    }
}

async fn compact(sessions: &Sessions, writer: &Arc<Writer>, params: &Value) -> Result<Value, String> {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| "sessionId is required".to_string())?
        .to_owned();
    let session = {
        let map = sessions.lock().await;
        map.get(&session_id).cloned()
    }
    .ok_or_else(|| "unknown sessionId".to_string())?;
    if !session.compact_available() {
        return Err("native host does not support compact".to_string());
    }
    let writer = writer.clone();
    let id = session_id.clone();
    tokio::spawn(async move {
        let blocks = [ContentBlock::Text {
            text: "/compact".to_owned(),
        }];
        if let Err(error) = session.prompt(&writer, &id, &blocks).await {
            diagln!("[pure-backend] compact failed: {error}");
        }
    });
    Ok(Value::Null)
}

async fn rename(writer: &Arc<Writer>, params: &Value) -> Result<Value, String> {
    let session_id = params
        .get("sessionId")
        .or_else(|| params.get("session_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| "sessionId is required".to_string())?;
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| "title is required".to_string())?
        .trim();
    let path = session_dir().join("titles.json");
    let mut titles = read_object(&path);
    if title.is_empty() {
        titles.remove(session_id);
    } else {
        titles.insert(session_id.to_owned(), json!(title));
    }
    write_object(&path, &titles);
    writer
        .notify(
            "session/update",
            json!({
                "sessionId": session_id,
                "update": {"sessionUpdate": "session_info_update", "title": title},
            }),
        )
        .await;
    Ok(json!({}))
}

/// The recorded file of session `id`, as the native host wrote it.
fn saved_path(id: &str) -> Result<PathBuf, String> {
    crate::history::saved_session(id)
}

/// Every `entry` line of a saved session, by id.
fn session_entries(path: &Path) -> Result<HashMap<String, Value>, String> {
    let reader = std::io::BufReader::new(
        std::fs::File::open(path).map_err(|error| format!("open session history: {error}"))?,
    );
    let mut entries = HashMap::new();
    for line in reader.lines().skip(1) {
        let line = line.map_err(|error| format!("read session history: {error}"))?;
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if entry["kind"] == "entry" {
            if let Some(id) = entry["id"].as_str() {
                entries.insert(id.to_owned(), entry);
            }
        }
    }
    Ok(entries)
}

/// The newest entry of the saved session, the leaf of its active branch.
fn session_leaf(entries: &HashMap<String, Value>) -> Option<String> {
    entries
        .values()
        .filter(|entry| entry["lane"].as_str().is_none_or(|lane| lane == "main"))
        .max_by_key(|entry| entry["seq"].as_u64().unwrap_or(0))
        .and_then(|entry| entry["id"].as_str())
        .map(str::to_owned)
}

fn branch_messages(path: &Path) -> Result<Vec<Value>, String> {
    let entries = session_entries(path)?;
    let leaf = session_leaf(&entries).ok_or_else(|| "the session has no entries".to_string())?;
    pure_extcore::history::branch_messages_from_entries(&entries, Some(&leaf))
}

/// The first text block of a user message, as typed.
fn prompt_text(message: &Value) -> Option<&str> {
    match message.get("content") {
        Some(Value::String(text)) => Some(text.as_str()),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .find(|block| block["type"] == "text")
            .and_then(|block| block["text"].as_str()),
        _ => None,
    }
}

/// The entry a fork stops at: the message just before the prompt the client
/// named, or the whole active branch when no prompt was named.
fn fork_target(messages: &[Value], before: Option<(&str, u64)>, at: Option<&str>) -> Result<Option<String>, String> {
    if let Some(prefix) = at {
        return messages
            .iter()
            .find(|message| {
                message["entryId"]
                    .as_str()
                    .is_some_and(|id| id.starts_with(prefix))
            })
            .and_then(|message| message["entryId"].as_str())
            .map(|id| Some(id.to_owned()))
            .ok_or_else(|| "unknown session entry".to_string());
    }
    let Some((text, occurrence)) = before else {
        return Ok(None);
    };
    let mut seen = 0u64;
    let mut previous: Option<&str> = None;
    for message in messages {
        if message["role"] == "user" && prompt_text(message) == Some(text) {
            if seen == occurrence {
                return previous
                    .map(|id| Some(id.to_owned()))
                    .ok_or_else(|| "the prompt has no entry before it".to_string());
            }
            seen += 1;
        }
        if let Some(id) = message["entryId"].as_str() {
            previous = Some(id);
        }
    }
    Err("the session has no such prompt".to_string())
}

async fn fork(params: &Value) -> Result<Value, String> {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| "sessionId is required".to_string())?;
    let source = saved_path(session_id)?;
    let messages = branch_messages(&source)?;
    let before = params.get("before").and_then(|before| {
        let text = before.get("text").and_then(Value::as_str);
        let occurrence = before.get("occurrence").and_then(Value::as_u64).unwrap_or(0);
        text.map(|text| (text, occurrence))
    });
    let at = params.get("at").and_then(Value::as_str);
    let target = fork_target(&messages, before, at)?;
    let header = {
        let file = std::fs::File::open(&source).map_err(|error| format!("open session: {error}"))?;
        let mut first = String::new();
        std::io::BufReader::new(file)
            .read_line(&mut first)
            .map_err(|error| format!("read session header: {error}"))?;
        pure_harness::session::jsonl::parse_header(first.trim_end())
            .map_err(|error| format!("invalid session header: {error}"))?
    };
    let modified_at = std::fs::metadata(&source)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0);
    let source_metadata: JsonlSessionMetadata =
        pure_harness::session::jsonl::metadata_from_header(
            &header,
            &source.to_string_lossy(),
            modified_at,
        );
    let cwd = source_metadata.cwd.clone();
    let new_id = uuid::Uuid::new_v4().to_string();
    let root = crate::history::session_root()?.join(&new_id);
    std::fs::create_dir_all(&root).map_err(|error| format!("create fork directory: {error}"))?;

    let env = Arc::new(pure_tools::OsExecutionEnv::with_cwd(PathBuf::from(&cwd)));
    let fs: Arc<dyn pure_tools::FileSystem> = env;
    let repo = JsonlSessionRepo::new(
        JsonlSessionRepoOptions {
            fs,
            sessions_root: root.to_string_lossy().into_owned(),
            clock: Arc::new(SystemClock),
            ids: Arc::new(DefaultIdGenerator::new()),
        },
        cwd.clone(),
    );
    let fork = match target {
        Some(entry) => ForkOptions::Branch {
            entry_id: Some(entry),
            position: Some(ForkPosition::At),
        },
        None => ForkOptions::default(),
    };
    repo.fork_typed(
        &source_metadata,
        &JsonlSessionCreateOptions {
            id: Some(new_id.clone()),
            parent_session_id: Some(session_id.to_owned()),
            cwd,
            metadata: None,
        },
        &fork,
    )
    .await
    .map_err(|error| format!("fork session: {error}"))?;
    Ok(json!({"sessionId": new_id, "truncated": false}))
}

fn export_markdown(messages: &[Value]) -> String {
    let mut markdown = String::from("# Session\n\n");
    for message in messages {
        match message["role"].as_str().unwrap_or("") {
            "user" => {
                markdown.push_str("## User\n\n");
                markdown.push_str(prompt_text(message).unwrap_or(""));
                markdown.push_str("\n\n");
            }
            "assistant" => {
                markdown.push_str("## Assistant\n\n");
                if let Some(blocks) = message["content"].as_array() {
                    for block in blocks {
                        match block["type"].as_str().unwrap_or("") {
                            "text" => {
                                markdown.push_str(block["text"].as_str().unwrap_or(""));
                                markdown.push_str("\n\n");
                            }
                            "toolCall" => {
                                let name = block["name"].as_str().unwrap_or("tool");
                                let arguments = block["arguments"]
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| block["arguments"].to_string());
                                markdown.push_str(&format!("Tool call `{name}`:\n\n```json\n"));
                                markdown.push_str(&arguments);
                                markdown.push_str("\n```\n\n");
                            }
                            "image" => {
                                markdown.push_str(&format!(
                                    "[image: {}]\n\n",
                                    block["mimeType"].as_str().unwrap_or("image")
                                ));
                            }
                            _ => {}
                        }
                    }
                } else if let Some(text) = message["content"].as_str() {
                    markdown.push_str(text);
                    markdown.push_str("\n\n");
                }
            }
            "toolResult" => {
                if let Some(blocks) = message["content"].as_array() {
                    let text = blocks
                        .iter()
                        .filter(|block| block["type"] == "text")
                        .filter_map(|block| block["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !text.is_empty() {
                        let tool = message["toolName"].as_str().unwrap_or("tool");
                        markdown.push_str(&format!("Tool result ({tool}):\n\n```text\n{text}\n```\n\n"));
                    }
                }
            }
            _ => {}
        }
    }
    markdown
}

fn export(params: &Value) -> Result<Value, String> {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| "sessionId is required".to_string())?;
    let messages = branch_messages(&saved_path(session_id)?)?;
    Ok(json!({"markdown": export_markdown(&messages)}))
}

/// Find the image (or clip) whose bytes hash to `sha256` in this branch.
fn media_by_hash(value: &Value, sha256: &str) -> Option<(String, Vec<u8>)> {
    match value {
        Value::Array(items) => items
            .iter()
            .find_map(|item| media_by_hash(item, sha256)),
        Value::Object(object) => {
            let data = object.get("data").and_then(Value::as_str);
            let mime = object.get("mimeType").and_then(Value::as_str);
            if let (Some(data), Some(mime)) = (data, mime) {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                    let digest = hex(&Sha256::digest(&bytes));
                    if digest == sha256 {
                        return Some((mime.to_owned(), bytes));
                    }
                }
            }
            object
                .values()
                .find_map(|value| media_by_hash(value, sha256))
        }
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn media_extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        _ => "bin",
    }
}

async fn save_image(
    writer: &Arc<Writer>,
    _sessions: &Sessions,
    params: &Value,
) -> Result<Value, String> {
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| "sessionId is required".to_string())?;
    let sha256 = params
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "sha256 is required".to_string())?
        .to_ascii_lowercase();
    let path = saved_path(session_id)?;
    let messages = branch_messages(&path)?;
    let (mime, bytes) = messages
        .iter()
        .find_map(|message| media_by_hash(message, &sha256))
        .ok_or_else(|| "no media with that hash is in this session".to_string())?;
    let directory = crate::history::session_directory(session_id)?;
    let requested = params.get("path").and_then(Value::as_str);
    let relative = requested.map(PathBuf::from).unwrap_or_else(|| {
        let extension = media_extension(&mime);
        PathBuf::from("generated").join(format!("{}.{extension}", &sha256[..16.min(sha256.len())]))
    });
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("the save path must stay inside the session directory".to_string());
    }
    let destination = directory.join(relative);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("create media directory: {error}"))?;
    }
    std::fs::write(&destination, bytes).map_err(|error| format!("write media: {error}"))?;
    writer
        .notify(
            "session/update",
            json!({
                "sessionId": session_id,
                "update": {
                    "sessionUpdate": "_pure/notice",
                    "tone": "success",
                    "text": format!("Saved media to {}", destination.display()),
                },
            }),
        )
        .await;
    Ok(json!({}))
}

/// Dispatch one `_pure/*` request. Unknown methods answer `method not found`,
/// which pure turns into a toast and a degraded panel, never a blocked UI.
pub async fn dispatch(
    writer: &Arc<Writer>,
    sessions: &Sessions,
    id: &Value,
    method: &str,
    params: &Value,
) {
    let result: Result<Value, (i64, String)> = match method {
        "_pure/config/get" => Ok(config_get()),
        "_pure/config/set" => config_set(params).map_err(|error| (-32602, error)),
        "_pure/auth/set" => auth_set(params).map_err(|error| (-32602, error)),
        "_pure/config/test" => config_test(params)
            .await
            .map_err(|error| (-32603, error)),
        "_pure/providers/directory" => providers_directory(params)
            .await
            .map_err(|error| (-32603, error)),
        "_pure/models/list" => Ok(models_list(sessions).await),
        "_pure/options/list" => Ok(json!({"options": options_list()})),
        "_pure/options/set" => options_set(params).map_err(|error| (-32602, error)),
        "_pure/themes" => Ok(json!({"themes": []})),
        "_pure/folders" => folders(params).map_err(|error| (-32602, error)),
        "_pure/fs" => fs_op(sessions, params).await.map_err(|error| (-32603, error)),
        "_pure/session/compact" => compact(sessions, writer, params)
            .await
            .map_err(|error| (-32603, error)),
        "_pure/session/rename" => rename(writer, params)
            .await
            .map_err(|error| (-32602, error)),
        "_pure/session/fork" => fork(params).await.map_err(|error| (-32603, error)),
        "_pure/session/export" => export(params).map_err(|error| (-32603, error)),
        "_pure/image/save" => save_image(writer, sessions, params)
            .await
            .map_err(|error| (-32603, error)),
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    match result {
        Ok(value) => writer.respond(id, value).await,
        Err((code, message)) => writer.respond_error(id, code, &message).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Value {
        json!({
            "kind": "openai",
            "base_url": "http://127.0.0.1:1/v1",
            "display_name": "Local",
            "api_key_env": "",
            "headers": {"X-Test": "1"},
            "models": [{
                "id": "m1",
                "name": "Model One",
                "context": 128000,
                "max_output": 8192,
                "reasoning": true,
                "input": ["text", "image"],
                "cost": {"input": 1.0, "output": 2.0, "cache_read": 0.1, "cache_write": 0.2}
            }]
        })
    }

    #[test]
    fn kage_provider_keeps_model_limits_thinking_and_cost() {
        let provider = kage_provider_to_rpi(&json!({}), &entry()).unwrap();
        assert_eq!(provider["api"], "openai-completions");
        assert_eq!(provider["baseUrl"], "http://127.0.0.1:1/v1");
        assert_eq!(provider["name"], "Local");
        assert!(provider.get("apiKey").is_none());
        let model = &provider["models"][0];
        assert_eq!(model["contextWindow"], 128000);
        assert_eq!(model["maxTokens"], 8192);
        assert_eq!(model["reasoning"], true);
        assert_eq!(model["input"], json!(["text", "image"]));
        assert_eq!(model["cost"]["input"], 1.0);
        assert_eq!(model["cost"]["cacheRead"], 0.1);
        assert_eq!(model["cost"]["cacheWrite"], 0.2);
    }

    #[test]
    fn kage_provider_mirrors_key_source() {
        let mut env_entry = entry();
        env_entry["api_key_env"] = json!("LOCAL_API_KEY");
        let provider = kage_provider_to_rpi(&json!({"apiKey": "$OLD"}), &env_entry).unwrap();
        assert_eq!(provider["apiKey"], "$LOCAL_API_KEY");

        let mut pasted = entry();
        pasted.as_object_mut().unwrap().remove("api_key_env");
        let provider = kage_provider_to_rpi(&json!({"apiKey": "$OLD"}), &pasted).unwrap();
        assert!(provider.get("apiKey").is_none());

        let provider = kage_provider_to_rpi(&json!({"apiKey": "$OLD"}), &entry()).unwrap();
        assert!(provider.get("apiKey").is_none());
    }

    #[test]
    fn model_round_trips_pure_fields() {
        let provider = kage_provider_to_rpi(&json!({}), &entry()).unwrap();
        let back = model_to_pure(&provider["models"][0]);
        assert_eq!(back["context"], 128000);
        assert_eq!(back["max_output"], 8192);
        assert_eq!(back["reasoning"], true);
        assert_eq!(back["input"], json!(["text", "image"]));
        assert_eq!(back["cost"]["cache_read"], 0.1);
        assert_eq!(back["cost"]["cache_write"], 0.2);
    }

    #[test]
    fn directory_provider_maps_models_dev_shape() {
        let directory = json!({
            "providers": {
                "local": {
                    "name": "Local",
                    "api": "http://127.0.0.1:1/v1",
                    "env": ["LOCAL_API_KEY"],
                    "models": {
                        "m1": {
                            "name": "Model One",
                            "reasoning": true,
                            "limit": {"context": 64000, "output": 4096},
                            "modalities": {"input": ["text", "image"], "output": ["text"]},
                            "cost": {"input": 1.0, "output": 2.0, "cache_read": 0.5}
                        }
                    }
                }
            }
        });
        let providers = directory_providers(&directory);
        let model = &providers[0]["models"][0];
        assert_eq!(model["id"], "m1");
        assert_eq!(model["context"], 64000);
        assert_eq!(model["output"], 4096);
        assert_eq!(model["reasoning"], true);
        assert_eq!(model["input"], json!(["text", "image"]));
        assert_eq!(model["produces"], json!(["text"]));
        assert_eq!(model["cost"]["cacheRead"], 0.5);
        assert_eq!(provider_to_pure("local", &json!({
            "api": "openai-completions",
            "baseUrl": "http://127.0.0.1:1/v1",
            "name": "Local",
            "models": [{"id": "m1", "contextWindow": 64000}]
        }))["kind"], "openai");
    }

    #[test]
    fn a_registered_override_keeps_its_recorded_protocol() {
        let existing = json!({"api": "openai-responses", "baseUrl": "https://api.openai.com/v1"});
        let override_entry = json!({"base_url": "https://gw.example/v1", "api_key_env": ""});
        let provider = kage_provider_to_rpi(&existing, &override_entry).unwrap();
        assert_eq!(provider["api"], "openai-responses");
        assert_eq!(provider["baseUrl"], "https://gw.example/v1");
    }

    #[test]
    fn efforts_become_a_thinking_map_and_come_back() {
        let mut offered = entry();
        offered["models"][0]["efforts"] = json!(["none", "low", "high"]);
        let provider = kage_provider_to_rpi(&json!({}), &offered).unwrap();
        let map = &provider["models"][0]["thinkingLevelMap"];
        assert_eq!(map["off"], "none");
        assert_eq!(map["low"], "low");
        assert_eq!(map["high"], "high");
        assert!(map["minimal"].is_null());
        assert!(map["medium"].is_null());
        let back = model_to_pure(&provider["models"][0]);
        assert_eq!(back["efforts"], json!(["none", "low", "high"]));
    }

    #[test]
    fn saving_keeps_rpi_only_model_fields() {
        let existing = json!({
            "api": "openai-completions",
            "models": [{
                "id": "m1",
                "compat": {"requiresReasoningContent": true},
                "baseUrl": "https://m1.example/v1"
            }]
        });
        let provider = kage_provider_to_rpi(&existing, &entry()).unwrap();
        let model = &provider["models"][0];
        assert_eq!(model["compat"]["requiresReasoningContent"], true);
        assert_eq!(model["baseUrl"], "https://m1.example/v1");
        assert_eq!(model["maxTokens"], 8192);
    }

    #[test]
    fn a_missing_protocol_reads_as_anthropic() {
        let provider =
            kage_provider_to_rpi(&json!({}), &json!({"base_url": "https://a.example", "api_key_env": ""}))
                .unwrap();
        assert!(provider.get("api").is_none());
        assert_eq!(provider_to_pure("a", &provider)["kind"], "anthropic");
    }

    #[test]
    fn directory_keeps_tool_calling_models_and_reads_npm() {
        let directory = json!({
            "lab": {
                "id": "lab", "name": "Lab", "api": "https://lab/v1",
                "env": ["LAB_KEY"], "npm": "@ai-sdk/anthropic",
                "models": {
                    "a": {"id": "a", "name": "Alpha", "tool_call": true,
                          "limit": {"context": 1000, "output": 100}},
                    "b": {"id": "b", "name": "Chat only", "tool_call": false}
                }
            },
            "empty": {
                "id": "empty", "name": "Empty", "env": [],
                "models": {"c": {"id": "c", "name": "Chat only", "tool_call": false}}
            }
        });
        let providers = directory_providers(&directory);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0]["id"], "lab");
        assert_eq!(providers[0]["kind"], "anthropic");
        assert_eq!(providers[0]["models"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn export_renders_the_conversation_and_tool_output() {
        let messages = vec![
            json!({"role":"user","content":"hello"}),
            json!({"role":"assistant","content":[
                {"type":"thinking","thinking":"hidden"},
                {"type":"text","text":"hi"},
                {"type":"toolCall","id":"c1","name":"read","arguments":{"path":"a.txt"}}
            ]}),
            json!({"role":"toolResult","toolName":"read","content":[{"type":"text","text":"body"}]}),
        ];
        let markdown = export_markdown(&messages);
        assert!(markdown.starts_with("# Session"));
        assert!(markdown.contains("## User\n\nhello"));
        assert!(markdown.contains("## Assistant\n\nhi"));
        assert!(markdown.contains("Tool call `read`"));
        assert!(markdown.contains("Tool result (read):"));
        assert!(markdown.contains("body"));
        assert!(!markdown.contains("hidden"));
    }

    #[test]
    fn media_hash_finds_the_matching_image_block() {
        let bytes = [0x89u8, b'P', b'N', b'G', 1, 2, 3];
        let sha = hex(&Sha256::digest(bytes));
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let message = json!({"role":"toolResult","content":[
            {"type":"text","text":"made an image"},
            {"type":"image","data":encoded,"mimeType":"image/png"}
        ]});
        let (mime, found) = media_by_hash(&message, &sha).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(found, bytes);
        assert!(media_by_hash(&message, &"0".repeat(64)).is_none());
    }

    #[test]
    fn fork_target_stops_on_the_message_before_the_named_prompt() {
        let messages = vec![
            json!({"entryId":"a","role":"user","content":"one"}),
            json!({"entryId":"b","role":"assistant","content":"first"}),
            json!({"entryId":"c","role":"user","content":"two"}),
            json!({"entryId":"d","role":"assistant","content":"second"}),
        ];
        assert_eq!(
            fork_target(&messages, Some(("two", 0)), None).unwrap(),
            Some("b".to_owned())
        );
        assert_eq!(fork_target(&messages, None, None).unwrap(), None);
        assert!(fork_target(&messages, Some(("one", 0)), None).is_err());
        assert!(fork_target(&messages, Some(("missing", 0)), None).is_err());
        assert_eq!(
            fork_target(&messages, None, Some("c")).unwrap(),
            Some("c".to_owned())
        );
    }

    #[test]
    fn configured_servers_become_acp_entries() {
        let pairs = pairs(Some(&json!({"TOKEN": "canary"})));
        assert_eq!(pairs[0]["name"], "TOKEN");
        assert_eq!(pairs[0]["value"], "canary");
    }

    #[test]
    fn the_mode_option_offers_full_access_and_gains_it_when_missing() {
        let option = mode_option("default");
        let values = option["options"].as_array().unwrap();
        assert!(
            values.iter().any(|value| value["value"] == "full"),
            "a fresh mode option offers full access"
        );
        let mut legacy = json!({
            "id": "mode",
            "options": [{"value": "default"}, {"value": "plan"}]
        });
        ensure_full_mode(&mut legacy);
        assert_eq!(
            legacy["options"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|value| value["value"] == "full")
                .count(),
            1,
            "a legacy option gains full access once"
        );
        ensure_full_mode(&mut legacy);
        assert_eq!(
            legacy["options"].as_array().unwrap().len(),
            3,
            "the second pass adds nothing"
        );
    }
}
