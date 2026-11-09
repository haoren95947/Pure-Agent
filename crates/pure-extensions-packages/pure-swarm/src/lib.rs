#![forbid(unsafe_code)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

//! Swarm batches as a user-triggered feature.
//!
//! `/swarm on` (or the desktop's Swarm toggle) makes the engine forward the
//! switch here: the plugin activates its `swarm` tool and appends the swarm
//! guidance to the session context. The model can then call `swarm` once with
//! a prompt template and one item per child; the tool starts one host-managed
//! child per item, waits for every child and returns one aggregated report.
//! `/swarm off` removes the tool from the active set again, so it never
//! reaches the model's context unless the user asked for a swarm.

mod runtime;

use rpi_plugin_sdk::handles::ToolHandles;
use rpi_plugin_sdk::{
    CommandHandlerFn, EventHandlerFn, EventTag, PluginApi, RuntimeActionId, StablePluginEvent,
    StableToolSchema, StbString, StbStringRef, StepHandle, StepResult, ToolPartialCb,
};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// ABI ids of the host's child-agent bridge (as pure-subagents uses).
const GET_EXECUTION_CONTEXT: u32 = 23;
const CHILD_AGENT: u32 = 24;

const SWARM_TOOL: &str = "swarm";
const MAX_ITEMS: usize = 8;
const MAX_REPORT_CHARS: usize = 2_000;
const MAX_RESULT_CHARS: usize = 24_000;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

const SWARM_ON: &str = "[swarm mode on] Split the work into independent items and delegate with \
one `swarm` call: a prompt_template containing {{item}} plus one item per child. Every child \
starts with zero context, so the template must carry the paths and details a child needs. Keep \
scopes disjoint: children may edit in parallel while each touches its own files. Prefer more, \
smaller items over one big one. Turn swarm mode off with `/swarm off`.";

const SWARM_OFF: &str = "[swarm mode off] Back to the normal workflow: do the work yourself or \
use delegate_task for separate tasks.";

fn state() -> &'static Mutex<bool> {
    static STATE: OnceLock<Mutex<bool>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(false))
}

#[derive(Clone, Copy)]
struct HostFns {
    action: extern "C" fn(u32, StbStringRef, *mut StbString, *mut c_void) -> i32,
    free: extern "C" fn(StbString),
}

static HOST: Mutex<Option<HostFns>> = Mutex::new(None);

fn runtime() -> Result<PluginApi, String> {
    runtime::snapshot().ok_or("swarm runtime is unavailable".into())
}

fn call_runtime(id: RuntimeActionId, args: Value) -> Result<Value, String> {
    let runtime = runtime()?;
    let input = args.to_string();
    let mut output = StbString::empty();
    let rc = (runtime.runtime_action)(
        id as u32,
        StbStringRef::from_str(&input),
        &mut output,
        runtime.user_data,
    );
    let text = output.to_string_lossy();
    (runtime.free_string)(output);
    if rc != 0 {
        return Err(if text.is_empty() {
            format!("runtime action {id:?} failed")
        } else {
            text
        });
    }
    if text.trim().is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_str(&text).map_err(|error| format!("invalid runtime response: {error}"))
    }
}

fn host_action(
    host: &HostFns,
    runtime_handle: usize,
    action: u32,
    args: Value,
) -> Result<Value, String> {
    let encoded = args.to_string();
    let mut out = StbString::empty();
    let rc = (host.action)(
        action,
        StbStringRef::from_str(&encoded),
        &mut out,
        runtime_handle as *mut c_void,
    );
    let text = out.to_string_lossy();
    (host.free)(out);
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid child response (status {rc}): {error}"))?;
    if rc != 0 {
        return Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("child action failed")
            .into());
    }
    Ok(value)
}

fn active_tools() -> Result<Vec<String>, String> {
    let value = call_runtime(RuntimeActionId::GetActiveTools, json!({}))?;
    value
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| "runtime did not return an active tool list".to_string())
        .and_then(|tools| {
            tools
                .iter()
                .map(|tool| {
                    tool.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "active tool list contains a non-string value".to_string())
                })
                .collect()
        })
}

fn set_tools(tools: Vec<String>) -> Result<(), String> {
    call_runtime(RuntimeActionId::SetActiveTools, json!({"tools": tools}))?;
    Ok(())
}

fn show_tool() -> Result<(), String> {
    let mut tools = active_tools()?;
    if !tools.iter().any(|name| name == SWARM_TOOL) {
        tools.push(SWARM_TOOL.to_owned());
        set_tools(tools)?;
    }
    Ok(())
}

fn hide_tool() -> Result<(), String> {
    let mut tools = active_tools()?;
    let before = tools.len();
    tools.retain(|name| name != SWARM_TOOL);
    if tools.len() != before {
        set_tools(tools)?;
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Appends a custom message the context builder forwards to the model.
fn append_note(text: &str) {
    let message = json!({
        "kind": "custom",
        "role": "custom",
        "content": [{"type": "text", "text": text}],
        "data": {"customType": "swarm-mode", "display": true},
        "timestamp": now_ms(),
    });
    let _ = call_runtime(RuntimeActionId::AppendEntry, json!({"message": message}));
}

fn activate() -> Result<bool, String> {
    let mut on = state().lock().unwrap_or_else(|p| p.into_inner());
    if *on {
        return Ok(false);
    }
    show_tool()?;
    *on = true;
    drop(on);
    append_note(SWARM_ON);
    Ok(true)
}

fn deactivate() -> Result<bool, String> {
    let mut on = state().lock().unwrap_or_else(|p| p.into_inner());
    if !*on {
        return Ok(false);
    }
    hide_tool()?;
    *on = false;
    drop(on);
    append_note(SWARM_OFF);
    Ok(true)
}

// ---------------------------------------------------------------------------
// One swarm batch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Request {
    description: String,
    prompt_template: String,
    items: Vec<String>,
    role: String,
    max_turns: u64,
    timeout_seconds: u64,
}

fn parse_request(params: &Value) -> Result<Request, String> {
    let items: Vec<String> = params
        .get("items")
        .and_then(Value::as_array)
        .ok_or("`items` must be an array of strings")?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "`items` must be an array of strings".to_string())
        })
        .collect::<Result<_, _>>()?;
    if items.len() < 2 {
        return Err(format!(
            "a swarm needs at least 2 items (got {})",
            items.len()
        ));
    }
    if items.len() > MAX_ITEMS {
        return Err(format!("a swarm is capped at {MAX_ITEMS} items"));
    }
    if items.iter().any(|item| item.trim().is_empty()) {
        return Err("every item must contain non-whitespace text".into());
    }
    let prompt_template = params
        .get("prompt_template")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or("`prompt_template` is required with items")?;
    if !prompt_template.contains("{{item}}") {
        return Err("`prompt_template` must contain {{item}}".into());
    }
    let description = params
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("swarm")
        .trim()
        .to_owned();
    let role = params
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("worker")
        .trim()
        .to_owned();
    let max_turns = params
        .get("maxTurns")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, 32);
    let timeout_seconds = params
        .get("timeoutSeconds")
        .and_then(Value::as_u64)
        .unwrap_or(300)
        .clamp(10, 1800);
    Ok(Request {
        description,
        prompt_template,
        items,
        role,
        max_turns,
        timeout_seconds,
    })
}

struct Member {
    child_id: String,
    session_id: Option<String>,
    item: String,
    index: usize,
    state: &'static str,
    report: String,
}

fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

fn aggregate(request: &Request, members: &[Member]) -> String {
    let completed = members.iter().filter(|m| m.state == "completed").count();
    let failed = members.iter().filter(|m| m.state == "failed").count();
    let cancelled = members
        .iter()
        .filter(|m| !matches!(m.state, "completed" | "failed"))
        .count();
    let mut out = format!(
        "completed: {completed}, failed: {failed}, cancelled: {cancelled}\n\
         swarm: {}\n",
        request.description
    );
    let mut budget = MAX_RESULT_CHARS.saturating_sub(out.len());
    for member in members {
        let body = if member.report.trim().is_empty() {
            "(no reply)".to_owned()
        } else {
            clip(member.report.trim(), MAX_REPORT_CHARS.min(budget.max(1)))
        };
        let block = format!(
            "\n## {} — {} (item {})\n{}\n",
            member.index + 1,
            member.state,
            clip(&member.item, 120),
            body
        );
        if block.len() > budget {
            break;
        }
        out.push_str(&block);
        budget -= block.len();
    }
    out
}

/// Starts one child per item and waits for all of them.
fn run_batch(
    request: Request,
    tool_call_id: &str,
    params: &Value,
    cancelled: &AtomicBool,
) -> Result<String, String> {
    let host = (*HOST.lock().unwrap_or_else(|p| p.into_inner()))
        .ok_or("swarm host runtime is unavailable")?;
    let rpi = params.get("__rpi").cloned().unwrap_or(Value::Null);
    let runtime_handle = rpi
        .get("runtimeHandle")
        .and_then(Value::as_u64)
        .and_then(|id| usize::try_from(id).ok())
        .filter(|id| *id != 0)
        .ok_or("swarm requires a host-provided tool invocation runtimeHandle; upgrade the host")?;
    let runtime_key = rpi
        .get("runtimeKey")
        .and_then(Value::as_str)
        .ok_or("swarm requires a host-provided invocation runtimeKey; upgrade the host")?
        .to_owned();
    let action = move |mut args: Value| {
        args["__rpiRuntimeKey"] = json!(runtime_key);
        host_action(&host, runtime_handle, CHILD_AGENT, args)
    };
    // Fail closed if another registration replaced this worker's host context.
    let expected = rpi;
    let actual = host_action(&host, runtime_handle, GET_EXECUTION_CONTEXT, json!({}))
        .map_err(|error| format!("cannot read execution context: {error}"))?;
    for key in ["sessionId", "cwd"] {
        if let Some(want) = expected.get(key).and_then(Value::as_str) {
            if actual.get(key).and_then(Value::as_str) != Some(want) {
                return Err(
                    "swarm host context does not match this session; reload the extension".into(),
                );
            }
        }
    }

    let total = request.items.len();
    let batch = format!("swarm-{tool_call_id}");
    let mut members: Vec<Member> = Vec::with_capacity(total);
    for (index, item) in request.items.iter().enumerate() {
        if cancelled.load(Ordering::Acquire) {
            for member in &members {
                let _ = action(json!({"op": "cancel", "childId": member.child_id}));
            }
            return Err("swarm cancelled".into());
        }
        let prompt = request.prompt_template.replace("{{item}}", item);
        let started = action(json!({
            "op": "start",
            "task": prompt,
            "role": request.role,
            "maxTurns": request.max_turns,
            "timeoutSeconds": request.timeout_seconds,
            "toolCallId": tool_call_id,
            "swarm": {"id": batch, "item": item, "index": index, "total": total},
        }));
        match started {
            Ok(value) => {
                let Some(child_id) = value.get("childId").and_then(Value::as_str) else {
                    return Err("child start returned no childId".into());
                };
                members.push(Member {
                    child_id: child_id.to_owned(),
                    session_id: value
                        .get("sessionId")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    item: item.clone(),
                    index,
                    state: "running",
                    report: String::new(),
                });
            }
            Err(error) => members.push(Member {
                child_id: String::new(),
                session_id: None,
                item: item.clone(),
                index,
                state: "failed",
                report: error,
            }),
        }
    }

    let deadline = Instant::now() + Duration::from_secs(request.timeout_seconds + 30);
    let mut pending: VecDeque<usize> = (0..members.len())
        .filter(|ix| members[*ix].state == "running")
        .collect();
    while !pending.is_empty() {
        if cancelled.load(Ordering::Acquire) {
            for ix in pending.drain(..) {
                let _ = action(json!({"op": "cancel", "childId": members[ix].child_id}));
            }
            return Err("swarm cancelled".into());
        }
        if Instant::now() >= deadline {
            for ix in pending.drain(..) {
                let _ = action(json!({"op": "cancel", "childId": members[ix].child_id}));
                members[ix].state = "cancelled";
                members[ix].report = "timed out".into();
            }
            break;
        }
        let count = pending.len();
        for _ in 0..count {
            let Some(ix) = pending.pop_front() else { break };
            let polled = action(json!({"op": "poll", "childId": members[ix].child_id}));
            match polled {
                Ok(value) => match value.get("status").and_then(Value::as_str) {
                    Some("running") => pending.push_back(ix),
                    Some("done") => {
                        let result = value.get("result").cloned().unwrap_or(Value::Null);
                        members[ix].state = match result.get("status").and_then(Value::as_str) {
                            Some("completed") => "completed",
                            Some("cancelled") => "cancelled",
                            _ => "failed",
                        };
                        members[ix].report = result
                            .get("text")
                            .and_then(Value::as_str)
                            .or_else(|| result.get("error").and_then(Value::as_str))
                            .unwrap_or("")
                            .to_owned();
                        if members[ix].state != "completed" {
                            if let Some(error) = result.get("error").and_then(Value::as_str) {
                                if members[ix].report.is_empty() {
                                    members[ix].report = error.to_owned();
                                }
                            }
                        }
                        let _ = action(json!({"op": "dispose", "childId": members[ix].child_id}));
                    }
                    Some("failed") => {
                        members[ix].state = "failed";
                        members[ix].report = value
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("child failed")
                            .to_owned();
                        let _ = action(json!({"op": "dispose", "childId": members[ix].child_id}));
                    }
                    _ => pending.push_back(ix),
                },
                Err(error) => {
                    members[ix].state = "failed";
                    members[ix].report = error;
                    let _ = action(json!({"op": "cancel", "childId": members[ix].child_id}));
                    let _ = action(json!({"op": "dispose", "childId": members[ix].child_id}));
                }
            }
        }
        if !pending.is_empty() {
            thread::sleep(POLL_INTERVAL);
        }
    }

    let text = aggregate(&request, &members);
    Ok(json!({
        "content": [{"type": "text", "text": text}],
        "details": {
            "kind": "swarm",
            "description": request.description,
            "members": members.iter().map(|member| json!({
                "item": member.item,
                "index": member.index,
                "state": member.state,
                "sessionId": member.session_id,
            })).collect::<Vec<_>>(),
        },
    })
    .to_string())
}

// ---------------------------------------------------------------------------
// Tool drive
// ---------------------------------------------------------------------------

struct Job {
    receiver: Receiver<Result<String, String>>,
    done: bool,
    worker: Option<JoinHandle<()>>,
}

static JOBS: ToolHandles<Job> = ToolHandles::new();

rpi_plugin_sdk::execute_handler!(fn execute(tool_call_id, params, free_params) {
    let text = params.to_string_lossy();
    params.free_with(free_params);
    let value = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    let id = tool_call_id.to_owned();
    let cancelled = Arc::new(AtomicBool::new(false));
    let token = Arc::clone(&cancelled);
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _scope = runtime::enter(runtime::snapshot());
        let result = parse_request(&value)
            .and_then(|request| run_batch(request, &id, &value, &token));
        let _ = sender.send(result);
    });
    JOBS.insert_with_cancel(
        Job {
            receiver,
            done: false,
            worker: Some(worker),
        },
        cancelled,
    )
});

extern "C" fn poll(handle: StepHandle, _: Option<ToolPartialCb>, _: *mut c_void) -> StepResult {
    if handle.is_null() {
        return StepResult::err(StbString::from_string("null swarm handle".into()));
    }
    let Some(entry) = JOBS.get(handle) else {
        return StepResult::err(StbString::from_string("invalid swarm handle".into()));
    };
    if entry.cancelled.load(Ordering::SeqCst) {
        return StepResult::err(StbString::from_string("swarm cancelled".into()));
    }
    let mut job = entry.lock();
    if job.done {
        return StepResult::err(StbString::from_string(
            "swarm tool polled after completion".into(),
        ));
    }
    match job.receiver.try_recv() {
        Ok(Ok(text)) => {
            job.done = true;
            StepResult::done(StbString::from_string(text))
        }
        Ok(Err(error)) => {
            job.done = true;
            StepResult::err(StbString::from_string(error))
        }
        Err(TryRecvError::Empty) => StepResult::pending(StbString::empty()),
        Err(TryRecvError::Disconnected) => {
            job.done = true;
            StepResult::err(StbString::from_string("swarm worker stopped".into()))
        }
    }
}

extern "C" fn cancel(handle: StepHandle) {
    JOBS.cancel(handle);
}

extern "C" fn destroy(handle: StepHandle) {
    if let Some(entry) = JOBS.remove(handle) {
        let worker = entry.lock().worker.take();
        if let Some(worker) = worker {
            let _ = worker.join();
        }
    }
}

use rpi_plugin_sdk::callbacks::free_string;

fn tool_schema() -> StableToolSchema {
    StableToolSchema {
        name: StbString::from_string(SWARM_TOOL.into()),
        description: StbString::from_string(
            "Run one host-managed child per item from a prompt template and wait for every child. \
             Only available while swarm mode is on. Use it for many children doing the same kind \
             of task over different inputs, and prefer more, smaller, independent items. Every \
             child starts with zero context, so the template must be self-contained; keep write \
             scopes disjoint."
                .into(),
        ),
        parameters: StbString::from_string(
            json!({
                "type": "object",
                "additionalProperties": true,
                "required": ["description", "prompt_template", "items"],
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "Short label for the whole swarm."
                    },
                    "prompt_template": {
                        "type": "string",
                        "description": "Task template; every {{item}} is replaced with the item."
                    },
                    "items": {
                        "type": "array",
                        "items": {"type": "string", "minLength": 1},
                        "minItems": 2,
                        "maxItems": MAX_ITEMS,
                        "description": "One child per entry, substituted into the template."
                    },
                    "role": {
                        "type": "string",
                        "default": "worker",
                        "description": "Descriptive role label every child carries."
                    },
                    "maxTurns": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 32,
                        "default": 8,
                        "description": "Per-child turn budget."
                    },
                    "timeoutSeconds": {
                        "type": "integer",
                        "minimum": 10,
                        "maximum": 1800,
                        "default": 300,
                        "description": "Per-child timeout in seconds."
                    }
                }
            })
            .to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// `/swarm` command
// ---------------------------------------------------------------------------

fn command_output(out: &mut rpi_plugin_sdk::callbacks::Output<'_>, value: Value) -> i32 {
    if !out.is_available() {
        return 1;
    }
    out.write(value.to_string());
    0
}

fn send_user_message(text: &str) -> Result<(), String> {
    call_runtime(RuntimeActionId::SendUserMessage, json!({"text": text}))?;
    Ok(())
}

rpi_plugin_sdk::string_handler! {
fn swarm_command(args_json, out, user_data) {
    let _scope = runtime::enter(runtime::callback(user_data));
    let raw = args_json.to_owned();
    let envelope: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let args = envelope
        .get("args")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    let head = args.split_whitespace().next().unwrap_or("").to_lowercase();
    let text = match head.as_str() {
        "on" => match activate() {
            Ok(true) => "Swarm mode on. Describe the work; split it into independent items."
                .to_string(),
            Ok(false) => "Swarm mode is already on.".to_string(),
            Err(error) => format!("Unable to enter swarm mode: {error}"),
        },
        "off" => match deactivate() {
            Ok(true) => "Swarm mode off.".to_string(),
            Ok(false) => "Swarm mode is already off.".to_string(),
            Err(error) => format!("Unable to leave swarm mode: {error}"),
        },
        "status" | "" => {
            let on = *state().lock().unwrap_or_else(|p| p.into_inner());
            format!("Swarm mode is {}.", if on { "on" } else { "off" })
        }
        _ => {
            let task = args.as_str();
            match activate() {
                Ok(_) => match send_user_message(task) {
                    Ok(()) => "Swarm mode on and the task was submitted.".to_string(),
                    Err(error) => format!("Unable to submit the task: {error}"),
                },
                Err(error) => format!("Unable to enter swarm mode: {error}"),
            }
        }
    };
    command_output(out, json!({"kind": "message", "text": text}))
}
}

extern "C" fn on_session_start(_: StablePluginEvent, user_data: *mut c_void) -> i32 {
    let _scope = runtime::enter(runtime::callback(user_data));
    let _ = hide_tool();
    0
}

rpi_plugin_sdk::export_plugin!(|api| {
    runtime::install(*api);
    *HOST.lock().unwrap_or_else(|p| p.into_inner()) = Some(HostFns {
        action: api.runtime_action,
        free: api.free_string,
    });
    if let Some(register_event) = api.register_event_handler {
        let rc = register_event(
            EventTag::SessionStart,
            on_session_start as EventHandlerFn,
            api.user_data,
        );
        if rc != 0 {
            return rc;
        }
    }
    let Some(register) = api.register_tool else {
        return 1;
    };
    let schema = Box::new(tool_schema());
    let rc = register(&*schema, execute, poll, cancel, destroy, free_string);
    drop(schema);
    if rc != 0 {
        return rc;
    }
    if let Some(register_command) = api.register_command {
        let name = StbStringRef::from_str("swarm");
        let description = StbStringRef::from_str("Turn the user-triggered swarm mode on or off");
        let _ = register_command(name, description, swarm_command as CommandHandlerFn);
    }
    0
});

#[cfg(test)]
mod tests {
    use super::{aggregate, parse_request, MAX_ITEMS};
    use serde_json::json;

    #[test]
    fn a_swarm_needs_two_to_the_cap_items_and_a_placeholder() {
        assert!(parse_request(&json!({"items": ["a"]})).is_err());
        assert!(parse_request(&json!({"items": ["a", "b"]})).is_err());
        assert!(parse_request(&json!({
            "items": ["a", "b"], "prompt_template": "no placeholder"
        }))
        .is_err());
        let items: Vec<String> = (0..=MAX_ITEMS).map(|n| format!("i{n}")).collect();
        assert!(parse_request(&json!({
            "items": items, "prompt_template": "do {{item}}"
        }))
        .is_err());
        assert!(parse_request(&json!({
            "items": ["a", "b"], "prompt_template": "do {{item}}"
        }))
        .is_ok());
    }

    #[test]
    fn the_aggregate_counts_states_and_keeps_each_item() {
        let request = parse_request(&json!({
            "description": "pages",
            "items": ["a", "b"],
            "prompt_template": "do {{item}}"
        }))
        .unwrap();
        let members = vec![
            super::Member {
                child_id: "c1".into(),
                session_id: Some("s1".into()),
                item: "a".into(),
                index: 0,
                state: "completed",
                report: "did a".into(),
            },
            super::Member {
                child_id: "c2".into(),
                session_id: Some("s2".into()),
                item: "b".into(),
                index: 1,
                state: "failed",
                report: "boom".into(),
            },
        ];
        let text = aggregate(&request, &members);
        assert!(text.starts_with("completed: 1, failed: 1, cancelled: 0"));
        assert!(text.contains("## 1 — completed"));
        assert!(text.contains("did a"));
        assert!(text.contains("boom"));
    }
}
