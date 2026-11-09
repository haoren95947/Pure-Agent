#![allow(clippy::not_unsafe_ptr_arg_deref)]

//! Plan mode as a user-triggered feature.
//!
//! `/plan` (or the desktop's Plan toggle) sets the session mode, which makes
//! the engine inject read-only planning guidance and makes this plugin expose
//! the `exit_plan` tool. The model investigates, then calls `exit_plan` once
//! with the whole plan; the call asks the user to approve, revise or reject.
//! While plan mode is off the tool is removed from the active tool set, so it
//! never reaches the model's context unless the user asked for it.

mod runtime;

use rpi_plugin_sdk::handles::ToolHandles;
use rpi_plugin_sdk::{
    CommandHandlerFn, EventHandlerFn, EventTag, RuntimeActionId, StablePluginEvent,
    StableToolSchema, StbString, StbStringRef, StepHandle, StepResult, ToolPartialCb,
    EVENT_HANDLER_ABORT, EVENT_HANDLER_CONTINUE,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const PLAN_TOOL: &str = "exit_plan";
const MAX_PLAN_CHARS: usize = 50_000;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

struct State {
    active: bool,
    /// Whether `exit_plan` is in the active tool set, so the turn-boundary
    /// unload does not pay a host round trip on every turn.
    visible: bool,
    /// The plan the user approved, kept for `/plan status`.
    plan: Option<String>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(State {
            active: false,
            visible: false,
            plan: None,
        })
    })
}

fn call_runtime(id: RuntimeActionId, args: Value) -> Result<Value, String> {
    let runtime = runtime::snapshot().ok_or("plan-mode runtime is unavailable")?;
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

/// Adds `exit_plan` to the active set, leaving the rest as they are.
fn show_tool() -> Result<(), String> {
    let mut tools = active_tools()?;
    if !tools.iter().any(|name| name == PLAN_TOOL) {
        tools.push(PLAN_TOOL.to_owned());
        set_tools(tools)?;
    }
    Ok(())
}

/// Removes `exit_plan` from the active set, leaving the rest as they are.
/// Reading the current list rather than restoring a saved one keeps tools
/// added mid-session (MCP servers, for instance).
fn hide_tool() -> Result<(), String> {
    let mut tools = active_tools()?;
    let before = tools.len();
    tools.retain(|name| name != PLAN_TOOL);
    if tools.len() != before {
        set_tools(tools)?;
    }
    Ok(())
}

fn activate() -> Result<bool, String> {
    let mut guard = state().lock().unwrap_or_else(|p| p.into_inner());
    if guard.active {
        return Ok(false);
    }
    show_tool()?;
    guard.active = true;
    guard.visible = true;
    Ok(true)
}

fn deactivate() -> Result<bool, String> {
    let mut guard = state().lock().unwrap_or_else(|p| p.into_inner());
    if !guard.active {
        return Ok(false);
    }
    hide_tool()?;
    guard.active = false;
    guard.visible = false;
    Ok(true)
}

/// Leaves plan mode without replacing the active tool set. An `exit_plan`
/// invocation runs under a narrowed execution bridge, which refuses that
/// replacement; the next turn boundary unloads the tool instead.
fn leave() -> bool {
    let mut guard = state().lock().unwrap_or_else(|p| p.into_inner());
    if !guard.active {
        return false;
    }
    guard.active = false;
    true
}

/// The numbered steps an outline can show, or the plan's first lines.
fn plan_outline(plan: &str) -> String {
    let steps: Vec<&str> = plan
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.split_once('.').is_some_and(|(number, text)| {
                number.trim().parse::<usize>().is_ok() && !text.trim().is_empty()
            })
        })
        .collect();
    if steps.is_empty() {
        return "(the plan above; use `/plan` to review it)".to_string();
    }
    let total = steps.len();
    let mut lines = vec![format!("Progress: 0/{total} complete")];
    for step in steps.iter().take(12) {
        lines.push(format!("- \u{25cb} {step}"));
    }
    if total > 12 {
        lines.push("... (open the plan to read the remaining steps)".to_string());
    }
    lines.join("\n")
}

fn plan_status() -> String {
    let guard = state().lock().unwrap_or_else(|p| p.into_inner());
    match (&guard.plan, guard.active) {
        (_, true) => {
            "Plan mode is on. Investigate, then call `exit_plan` with the whole plan.".to_string()
        }
        (Some(plan), false) => format!("## Plan\n\n{plan}"),
        (None, false) => "No plan yet. Use `/plan on` to start one.".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Approve,
    Revise,
    Reject,
}

fn review_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!(
        "plan-review-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn answer_of(response: &Value) -> (Option<String>, Option<String>) {
    let answer = response.get("answer").unwrap_or(&Value::Null);
    let choice = answer
        .get("values")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            answer
                .get("value")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let text = answer
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_owned);
    (choice, text)
}

fn choice_of(label: &str) -> Choice {
    let lowered = label.to_lowercase();
    if lowered.contains("approve") {
        Choice::Approve
    } else if lowered.contains("revise") {
        Choice::Revise
    } else {
        Choice::Reject
    }
}

/// Asks the user about `plan` through the host's permission dialog and
/// waits for the verdict, polling so a cancelled turn stops the wait.
fn ask_review(
    tool_call_id: &str,
    cancelled: &AtomicBool,
) -> Result<(Choice, Option<String>), String> {
    let request_id = review_id();
    let ui = json!({
        "kind": "confirm",
        "purpose": "permission",
        "id": "plan-mode",
        "question": PLAN_TOOL,
        "message": "Review this plan before it is carried out.",
        "options": [
            {"title": "Approve plan", "permission": "allow_once"},
            {"title": "Revise", "permission": "reject_once"},
            {"title": "Reject and exit", "permission": "reject_once"}
        ]
    });
    let opened = call_runtime(
        RuntimeActionId::UiDialog,
        json!({
            "op": "open",
            "requestId": request_id,
            "toolCallId": tool_call_id,
            "ui": ui,
        }),
    )?;
    if let Some(error) = opened.get("error").and_then(Value::as_str) {
        return Err(format!("cannot open the plan review: {error}"));
    }
    loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = call_runtime(
                RuntimeActionId::UiDialog,
                json!({"op": "cancel", "requestId": request_id}),
            );
            return Err("plan review cancelled".into());
        }
        let response = call_runtime(
            RuntimeActionId::UiDialog,
            json!({"op": "poll", "requestId": request_id}),
        )?;
        match response
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("pending")
        {
            "answered" => {
                let (label, text) = answer_of(&response);
                let label = label.unwrap_or_default();
                return Ok((choice_of(&label), text));
            }
            "cancelled" => return Err("plan review cancelled".into()),
            _ => thread::sleep(POLL_INTERVAL),
        }
    }
}

fn tool_envelope(text: String, details: Value) -> String {
    json!({
        "content": [{"type": "text", "text": text}],
        "details": details,
    })
    .to_string()
}

#[derive(Clone)]
struct Verdict {
    choice: Choice,
    revision: Option<String>,
}

/// The answers the `ToolCall` handler collected, keyed by tool call id.
/// The review runs there because only event handlers reach the host's
/// dialog bridge with an invocation context a tool worker lacks.
fn verdicts() -> &'static Mutex<HashMap<String, Verdict>> {
    static VERDICTS: OnceLock<Mutex<HashMap<String, Verdict>>> = OnceLock::new();
    VERDICTS.get_or_init(|| Mutex::new(HashMap::new()))
}

// Asks the user about an `exit_plan` call before it runs, the same place and
// shape the permission gate uses. The macro's trampoline is the one unsafe
// ABI boundary this crate has: it validates the host payload before the safe
// view reads it.
rpi_plugin_sdk::event_handler! {
fn on_tool_call(event, user_data) {
    let _scope = runtime::enter(runtime::callback(user_data));
    let Some(call) = event.tool_call() else {
        return EVENT_HANDLER_CONTINUE;
    };
    if call.tool_name != PLAN_TOOL {
        return EVENT_HANDLER_CONTINUE;
    }
    if !state().lock().unwrap_or_else(|p| p.into_inner()).active {
        return EVENT_HANDLER_CONTINUE;
    }
    let idle = AtomicBool::new(false);
    match ask_review(&call.tool_call_id, &idle) {
        Ok((choice, revision)) => {
            verdicts()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(call.tool_call_id, Verdict { choice, revision });
            EVENT_HANDLER_CONTINUE
        }
        Err(error) => {
            eprintln!("[pure-plan-mode] plan review unavailable: {error}");
            EVENT_HANDLER_ABORT
        }
    }
}
}

/// Runs one `exit_plan` call: validates the plan and applies the verdict
/// the `ToolCall` handler collected.
fn review_plan(tool_call_id: &str, params: &Value) -> Result<String, String> {
    let plan = params
        .get("plan")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .ok_or("`plan` must be a non-empty Markdown document")?;
    if plan.chars().count() > MAX_PLAN_CHARS {
        return Err(format!(
            "`plan` must not exceed {MAX_PLAN_CHARS} characters"
        ));
    }
    let active = state().lock().unwrap_or_else(|p| p.into_inner()).active;
    if !active {
        return Ok(tool_envelope(
            "Plan mode is already off; carry out the work.".to_owned(),
            json!({"kind": "plan", "active": false}),
        ));
    }
    let Some(verdict) = verdicts()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(tool_call_id)
    else {
        return Err("the plan was not reviewed; keep plan mode on".into());
    };
    let revision = verdict.revision;
    match verdict.choice {
        Choice::Approve => {
            leave();
            state().lock().unwrap_or_else(|p| p.into_inner()).plan = Some(plan.to_owned());
            Ok(tool_envelope(
                "The user approved the plan. Plan mode is off: carry out the plan now.".to_owned(),
                json!({
                    "kind": "plan",
                    "plan": plan,
                    "outline": plan_outline(plan),
                    "active": false,
                    "markdown": true,
                }),
            ))
        }
        Choice::Revise => {
            let note = revision
                .filter(|text| !text.trim().is_empty())
                .map(|text| format!("\nThe user asked to change: {text}"))
                .unwrap_or_default();
            Ok(tool_envelope(
                format!(
                    "The user wants a different plan. Keep plan mode on, address the feedback and \
                     call `exit_plan` again.{note}"
                ),
                json!({"kind": "plan", "active": true, "markdown": true}),
            ))
        }
        Choice::Reject => {
            leave();
            Ok(tool_envelope(
                "The user rejected the plan. Plan mode is off; do not carry it out.".to_owned(),
                json!({"kind": "plan", "active": false}),
            ))
        }
    }
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
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _scope = runtime::enter(runtime::snapshot());
        let result = review_plan(&id, &value);
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
        return StepResult::err(StbString::from_string("null plan handle".into()));
    }
    let Some(entry) = JOBS.get(handle) else {
        return StepResult::err(StbString::from_string("invalid plan handle".into()));
    };
    if entry.cancelled.load(Ordering::SeqCst) {
        return StepResult::err(StbString::from_string("plan review cancelled".into()));
    }
    let mut job = entry.lock();
    if job.done {
        return StepResult::err(StbString::from_string(
            "plan tool polled after completion".into(),
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
            StepResult::err(StbString::from_string("plan review worker stopped".into()))
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
        name: StbString::from_string(PLAN_TOOL.into()),
        description: StbString::from_string(
            "Present your plan for the user's review and leave plan mode when it is approved. \
             Only available while plan mode is on. Pass the whole plan in Markdown: a `#` title, \
             the goal, the steps with the files each one touches, and how to verify the result. \
             Call it once, when the investigation is done, and do not start implementing before \
             the user approves."
                .into(),
        ),
        parameters: StbString::from_string(
            json!({
                "type": "object",
                "additionalProperties": true,
                "required": ["plan"],
                "properties": {
                    "plan": {
                        "type": "string",
                        "pattern": "\\S",
                        "description": format!(
                            "The whole plan in Markdown. The trimmed document must hold 1 to {MAX_PLAN_CHARS} Unicode characters."
                        )
                    }
                }
            })
            .to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// `/plan` command
// ---------------------------------------------------------------------------

fn command_output(out: &mut rpi_plugin_sdk::callbacks::Output<'_>, value: Value) -> i32 {
    if !out.is_available() {
        return 1;
    }
    out.write(value.to_string());
    0
}

rpi_plugin_sdk::string_handler! {
fn plan_command(args_json, out, user_data) {
    let _scope = runtime::enter(runtime::callback(user_data));
    let raw = args_json.to_owned();
    let envelope: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let args = envelope
        .get("args")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    let first = args.split_whitespace().next().unwrap_or("").to_lowercase();
    let text = match first.as_str() {
        "on" | "start" => match activate() {
            Ok(true) => "Plan mode on. Inspect the codebase, then call `exit_plan` with the whole plan."
                .to_string(),
            Ok(false) => "Plan mode is already on.".to_string(),
            Err(error) => format!("Unable to enter plan mode: {error}"),
        },
        "off" | "exit" => match deactivate() {
            Ok(true) => "Plan mode off. Full tool access restored.".to_string(),
            Ok(false) => "Plan mode is already off.".to_string(),
            Err(error) => format!("Unable to leave plan mode: {error}"),
        },
        "show" | "status" | "" => plan_status(),
        _ => format!(
            "Plan mode: use `/plan on`, `/plan off` or `/plan status`.\n\nRequest: {args}"
        ),
    };
    command_output(out, json!({"kind": "message", "text": text}))
}
}

extern "C" fn on_session_start(_: StablePluginEvent, user_data: *mut c_void) -> i32 {
    let _scope = runtime::enter(runtime::callback(user_data));
    // Every registered tool starts active; the feature's tool is only
    // loaded while the user asked for plan mode.
    let _ = hide_tool();
    state().lock().unwrap_or_else(|p| p.into_inner()).visible = false;
    0
}

/// An approval or rejection ends plan mode inside the tool invocation, where
/// the active set cannot be replaced; unload `exit_plan` here, before the
/// next model turn.
extern "C" fn on_turn_end(_: StablePluginEvent, user_data: *mut c_void) -> i32 {
    let _scope = runtime::enter(runtime::callback(user_data));
    let unload = {
        let mut guard = state().lock().unwrap_or_else(|p| p.into_inner());
        if guard.active || !guard.visible {
            false
        } else {
            guard.visible = false;
            true
        }
    };
    if unload && hide_tool().is_err() {
        state().lock().unwrap_or_else(|p| p.into_inner()).visible = true;
    }
    0
}

rpi_plugin_sdk::export_plugin!(|api| {
    runtime::install(*api);
    if let Some(register_event) = api.register_event_handler {
        for (tag, handler) in [
            (EventTag::SessionStart, on_session_start as EventHandlerFn),
            (EventTag::TurnEnd, on_turn_end as EventHandlerFn),
            (EventTag::ToolCall, on_tool_call as EventHandlerFn),
        ] {
            let rc = register_event(tag, handler, api.user_data);
            if rc != 0 {
                return rc;
            }
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
        let name = StbStringRef::from_str("plan");
        let description =
            StbStringRef::from_str("Enter, leave or show the user-triggered plan mode");
        let _ = register_command(name, description, plan_command as CommandHandlerFn);
    }
    0
});

#[cfg(test)]
mod tests {
    use super::{choice_of, plan_outline, Choice};

    #[test]
    fn answer_labels_map_to_the_three_choices() {
        assert_eq!(choice_of("Approve plan"), Choice::Approve);
        assert_eq!(choice_of("Revise"), Choice::Revise);
        assert_eq!(choice_of("Reject and exit"), Choice::Reject);
    }

    #[test]
    fn the_outline_lists_numbered_steps() {
        let outline = plan_outline("## Plan\n\n1. Add the parser\n2. Update the UI\n\nDetails");
        assert!(outline.contains("Progress: 0/2"));
        assert!(outline.contains("1. Add the parser"));
        assert!(outline.contains("2. Update the UI"));
    }
}
