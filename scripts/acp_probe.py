"""Drive pure-backend over ACP stdio with a local mock model server.

Usage: python scripts/acp_probe.py [--backend PATH] [--rpi PATH] [--home DIR]
Prints every JSON-RPC frame it receives and a PASS/FAIL summary.
"""
import argparse
import base64
import hashlib
import itertools
import json
import os
import queue
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPLY = "mock reply: hello from pure"
# Tool names each model request carried, newest last, so the probe can assert
# which tools reach the model context.
TOOLS_SEEN = []
TOOLS_LOCK = threading.Lock()


def last_tools():
    with TOOLS_LOCK:
        return TOOLS_SEEN[-1] if TOOLS_SEEN else []


# Hosts remember cancelled tool call ids, so every mock call needs its own.
CALL_SEQ = itertools.count(1)


class MockHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args):
        pass

    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            body = json.dumps({"data": [{"id": "mock-1", "name": "Mock One"}]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path == "/api.json":
            body = json.dumps({
                "lab": {
                    "id": "lab",
                    "name": "Lab",
                    "api": f"http://127.0.0.1:{self.server.server_port}/v1",
                    "env": ["LAB_KEY"],
                    "npm": "@ai-sdk/anthropic",
                    "models": {
                        "m1": {
                            "id": "m1",
                            "name": "Tool Model",
                            "tool_call": True,
                            "reasoning": True,
                            "limit": {"context": 1000, "output": 100},
                            "modalities": {"input": ["text"], "output": ["text"]},
                            "cost": {"input": 1.0, "output": 2.0, "cache_read": 0.5},
                        },
                        "m2": {"id": "m2", "name": "Chat Only", "tool_call": False},
                    },
                }
            }).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_error(404)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        if "chat/completions" not in self.path:
            self.send_error(404)
            return
        try:
            body = json.loads(raw.decode("utf-8"))
        except Exception:
            body = {}
        names = sorted(
            str(tool.get("function", {}).get("name") or tool.get("name") or "")
            for tool in body.get("tools") or []
        )
        with TOOLS_LOCK:
            TOOLS_SEEN.append(names)
        messages = body.get("messages") or []

        def flatten(message):
            content = message.get("content") or ""
            if isinstance(content, str):
                return content
            return " ".join(str(part.get("text") or "") for part in content if isinstance(part, dict))

        text = " ".join(flatten(message) for message in messages).lower()
        user_messages = [flatten(message) for message in messages if message.get("role") == "user"]
        last_user = user_messages[-1].lower() if user_messages else ""
        last_role = messages[-1].get("role") if messages else None
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        if "delegated" in text and "subagent" in text:
            self._text("child says hi")
        elif "slow" in last_user:
            self._text_slow()
        elif last_role == "tool":
            self._text("tool done: all good")
        elif "bash" in last_user:
            self._tool_call("bash", {"command": "echo permission ok"})
        elif "codemode" in last_user:
            self._tool_call("codemode", {"code": "return {ok: true, message: 'codemode ran'};"})
        elif "make a plan" in last_user:
            self._tool_call("exit_plan", {"plan": "1. Inspect\n2. Implement"})
        elif "compact" in last_user or "summar" in last_user:
            self._text("mock summary of earlier turns")
        elif "delegate" in last_user:
            self._tool_call("delegate_task", {"task": "say hi to the parent", "role": "tester", "maxTurns": 2})
        elif "hello" in last_user:
            self._text(REPLY)
        else:
            self._tool_call("todo", {"action": "add", "text": "verify closed loop"})

    def _text(self, reply):
        for chunk in (reply[:10], reply[10:]):
            frame = {
                "id": "chatcmpl-mock",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": "mock-1",
                "choices": [{"index": 0, "delta": {"content": chunk}, "finish_reason": None}],
            }
            self.wfile.write(("data: " + json.dumps(frame) + "\n\n").encode())
            self.wfile.flush()
            time.sleep(0.02)
        self._finish("stop")

    def _text_slow(self):
        frame = {
            "id": "chatcmpl-mock",
            "object": "chat.completion.chunk",
            "created": int(time.time()),
            "model": "mock-1",
            "choices": [{"index": 0, "delta": {"content": "slow start"}, "finish_reason": None}],
        }
        self.wfile.write(("data: " + json.dumps(frame) + "\n\n").encode())
        self.wfile.flush()
        time.sleep(6)
        self._finish("stop")

    def _tool_call(self, name, arguments):
        frame = {
            "id": "chatcmpl-mock",
            "object": "chat.completion.chunk",
            "created": int(time.time()),
            "model": "mock-1",
            "choices": [{
                "index": 0,
                "delta": {"tool_calls": [{
                    "index": 0,
                    "id": f"call_mock_{next(CALL_SEQ)}",
                    "type": "function",
                    "function": {"name": name, "arguments": json.dumps(arguments)},
                }]},
                "finish_reason": None,
            }],
        }
        self.wfile.write(("data: " + json.dumps(frame) + "\n\n").encode())
        self.wfile.flush()
        self._finish("tool_calls")

    def _finish(self, reason):
        final = {
            "id": "chatcmpl-mock",
            "object": "chat.completion.chunk",
            "created": int(time.time()),
            "model": "mock-1",
            "choices": [{"index": 0, "delta": {}, "finish_reason": reason}],
            "usage": {"prompt_tokens": 8, "completion_tokens": 6, "total_tokens": 14},
        }
        self.wfile.write(("data: " + json.dumps(final) + "\n\n").encode())
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


def seed_config(home, port):
    agent = os.path.join(home, "agent")
    os.makedirs(agent, exist_ok=True)
    models = {
        "providers": {
            "mock": {
                "name": "Mock",
                "api": "openai-completions",
                "baseUrl": f"http://127.0.0.1:{port}/v1",
                "models": [{
                    "id": "mock-1",
                    "name": "Mock One",
                    "contextWindow": 200000,
                    "input": ["text", "image"],
                }],
            }
        }
    }
    with open(os.path.join(agent, "models.json"), "w", encoding="utf-8") as handle:
        json.dump(models, handle, indent=2)
    with open(os.path.join(agent, "auth.json"), "w", encoding="utf-8") as handle:
        json.dump({"mock": {"type": "api_key", "key": "test-key"}}, handle, indent=2)
    with open(os.path.join(agent, "settings.json"), "w", encoding="utf-8") as handle:
        json.dump({"defaultProvider": "mock", "defaultModel": "mock-1", "defaultThinkingLevel": "medium", "enableSkillCommands": True}, handle, indent=2)
    skill = os.path.join(agent, "skills", "demo")
    os.makedirs(skill, exist_ok=True)
    with open(os.path.join(skill, "SKILL.md"), "w", encoding="utf-8") as handle:
        handle.write("""---
name: demo
description: Demo skill for the Pure Agent closed-loop probe
---

Follow the demo instructions.
""")


def list_all_sessions(send, wait):
    sessions = []
    cursor = None
    while True:
        params = {"cursor": cursor} if cursor else {}
        page = wait(send("session/list", params))
        result = page.get("result") or {}
        sessions.extend(result.get("sessions") or [])
        cursor = result.get("nextCursor")
        if not cursor:
            return sessions


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--backend", default=os.path.join(ROOT, "target", "debug", "pure-backend.exe"))
    parser.add_argument(
        "--rpi",
        default=None,
        help="optional separate pure host; defaults to the backend executable, "
        "which serves the host role as well",
    )
    parser.add_argument("--home", default=os.path.join(tempfile.gettempdir(), "pure-probe"))
    parser.add_argument("--extensions", default=os.path.join(ROOT, "target", "debug"))
    args = parser.parse_args()

    os.makedirs(args.home, exist_ok=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), MockHandler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    seed_config(args.home, server.server_port)

    env = dict(os.environ)
    env["PURE_HOME"] = args.home
    env["RPI_BIN"] = args.rpi or args.backend
    env["RPI_ACP_DEBUG"] = "1"
    env["RPI_EXTENSIONS_DIR"] = args.extensions

    proc = subprocess.Popen(
        [args.backend, "rpc"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )
    lines = queue.Queue()

    def reader(stream, label):
        for raw in iter(stream.readline, b""):
            text = raw.decode("utf-8", "replace").rstrip("\r\n")
            lines.put((label, text))
        lines.put((label, None))

    threading.Thread(target=reader, args=(proc.stdout, "out"), daemon=True).start()
    threading.Thread(target=reader, args=(proc.stderr, "err"), daemon=True).start()

    next_id = [0]
    transcript = []
    permission_seen = [False]

    def send(method, params):
        next_id[0] += 1
        request_id = next_id[0]
        frame = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        proc.stdin.write((json.dumps(frame) + "\n").encode())
        proc.stdin.flush()
        return request_id

    def wait_for(request_id, timeout=60, collect=None):
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                label, text = lines.get(timeout=0.2)
            except queue.Empty:
                continue
            if text is None:
                break
            if collect is not None:
                collect.append((label, text))
            if label == "out" and text.startswith("{"):
                try:
                    frame = json.loads(text)
                except json.JSONDecodeError:
                    continue
                if frame.get("method") == "session/request_permission":
                    permission_seen[0] = True
                    proc.stdin.write((json.dumps({
                        "jsonrpc": "2.0",
                        "id": frame["id"],
                        "result": {"outcome": {"outcome": "selected", "optionId": "0"}},
                    }) + "\n").encode())
                    proc.stdin.flush()
                    continue
                if frame.get("id") == request_id:
                    return frame
        raise TimeoutError(f"no response for id {request_id}")

    def drain_commands(session, timeout=6.0):
        deadline = time.time() + timeout
        commands = []
        while time.time() < deadline:
            try:
                label, text = lines.get(timeout=0.2)
            except queue.Empty:
                continue
            if label != "out" or not text or not text.startswith("{"):
                continue
            try:
                frame = json.loads(text)
            except json.JSONDecodeError:
                continue
            params = frame.get("params") or {}
            if params.get("sessionId") != session:
                continue
            update = params.get("update") or {}
            if update.get("sessionUpdate") == "available_commands_update":
                commands = [c.get("name") for c in update.get("availableCommands") or []]
                break
        return commands

    results = []
    cwd = os.path.join(args.home, "workspace")
    os.makedirs(cwd, exist_ok=True)

    init = wait_for(send("initialize", {
        "protocolVersion": 1,
        "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}, "terminal": False},
        "clientInfo": {"name": "acp-probe", "version": "0.0.1"},
    }))
    print("initialize:", json.dumps(init.get("result"))[:400])
    results.append(("initialize", "agentCapabilities" in (init.get("result") or {})))

    new = wait_for(send("session/new", {"cwd": cwd, "mcpServers": []}), timeout=60)
    print("session/new:", json.dumps(new)[:1200])
    session_id = (new.get("result") or {}).get("sessionId")
    results.append(("session/new", bool(session_id)))
    mode_option = next(
        (option for option in (new.get("result") or {}).get("configOptions") or []
         if option.get("id") == "mode"),
        {},
    )
    mode_values = [value.get("value") for value in mode_option.get("options") or []]
    print("mode values:", mode_values)
    results.append(("mode option offers full access", "full" in mode_values))
    swarm_option = next(
        (option for option in (new.get("result") or {}).get("configOptions") or []
         if option.get("id") == "swarm"),
        {},
    )
    results.append(("swarm option defaults off", swarm_option.get("currentValue") == "off"))
    time.sleep(1.0)
    commands = []
    while True:
        try:
            label, raw = lines.get(timeout=0.1)
        except queue.Empty:
            break
        if label == "out" and raw.startswith("{"):
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "available_commands_update":
                commands = [c.get("name") for c in update.get("availableCommands") or []]
    print("commands:", commands)
    results.append(("extension commands", all(name in commands for name in ("goal", "todos"))))
    results.append(("on-demand mode commands", all(name in commands for name in ("plan", "swarm"))))
    results.append(("skill command", any("demo" in str(name) for name in commands)))

    off = wait_for(send("_pure/config/set", {
        "path": ["plugins", "todo", "enabled"],
        "value": False,
    }))
    installed = {
        plugin.get("name"): plugin.get("enabled")
        for plugin in (off.get("result") or {}).get("installedPlugins") or []
    }
    print("plugins after disable:", installed)
    results.append((
        "plugin toggle persists",
        installed.get("todo") is False and installed.get("goal") is True,
    ))

    off_session = wait_for(send("session/new", {"cwd": cwd, "mcpServers": []}), timeout=90)
    off_id = (off_session.get("result") or {}).get("sessionId")
    off_commands = drain_commands(off_id)
    print("commands with todo disabled:", off_commands)
    results.append((
        "disabled plugin command gone",
        bool(off_id) and "todos" not in off_commands and "goal" in off_commands,
    ))

    on = wait_for(send("_pure/config/set", {
        "path": ["plugins", "todo", "enabled"],
        "value": True,
    }))
    reenabled = {
        plugin.get("name"): plugin.get("enabled")
        for plugin in (on.get("result") or {}).get("installedPlugins") or []
    }
    results.append(("plugin re-enabled", reenabled.get("todo") is True))
    on_session = wait_for(send("session/new", {"cwd": cwd, "mcpServers": []}), timeout=90)
    on_id = (on_session.get("result") or {}).get("sessionId")
    on_commands = drain_commands(on_id)
    print("commands after re-enable:", on_commands)
    results.append(("re-enabled plugin command back", "todos" in on_commands))

    if session_id:
        updates = []
        prompt = wait_for(
            send("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "use the todo tool"}]}),
            timeout=90,
            collect=updates,
        )
        text = ""
        for label, raw in updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "agent_message_chunk":
                text += (update.get("content") or {}).get("text") or ""
        print("prompt stopReason:", (prompt.get("result") or {}).get("stopReason"), "text:", repr(text))
        tool_names = []
        plan_seen = False
        turn_updates = []
        for label, raw in updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "tool_call":
                tool_names.append(update.get("name") or update.get("title"))
            if update.get("sessionUpdate") == "plan":
                plan_seen = True
            if update.get("sessionUpdate") == "_pure/turn":
                turn_updates.append((update.get("phase"), update.get("reason")))
        print("tool calls:", tool_names, "plan update:", plan_seen, "turns:", turn_updates)
        results.append(("prompt text", "tool done" in text))
        results.append(("prompt stop", (prompt.get("result") or {}).get("stopReason") == "end_turn"))
        results.append(("todo tool", any(name == "todo" for name in tool_names)))
        results.append(("todo plan update", plan_seen))
        results.append((
            "_pure/turn updates",
            ("start", None) in turn_updates and ("end", "tool_calls") in turn_updates,
        ))

        code_updates = []
        code_prompt = wait_for(
            send("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "run codemode now"}]}),
            timeout=90,
            collect=code_updates,
        )
        code_tools = []
        code_text = ""
        for label, raw in code_updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "agent_message_chunk":
                code_text += (update.get("content") or {}).get("text") or ""
            if update.get("sessionUpdate") == "tool_call":
                code_tools.append(update.get("name") or update.get("title"))
        print("codemode tools:", code_tools, "text:", repr(code_text))
        results.append((
            "codemode tool",
            "codemode" in code_tools
            and "tool done" in code_text
            and (code_prompt.get("result") or {}).get("stopReason") == "end_turn",
        ))

        deleg_updates = []
        deleg = wait_for(
            send("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "delegate please"}]}),
            timeout=120,
            collect=deleg_updates,
        )
        deleg_text = ""
        deleg_tools = []
        for label, raw in deleg_updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "agent_message_chunk":
                deleg_text += (update.get("content") or {}).get("text") or ""
            if update.get("sessionUpdate") == "tool_call":
                deleg_tools.append(update.get("name") or update.get("title"))
        print("delegate tools:", deleg_tools, "text:", repr(deleg_text))
        results.append(("subagent tool", any(name == "delegate_task" for name in deleg_tools)))
        results.append(("subagent run", (deleg.get("result") or {}).get("stopReason") == "end_turn"))

        slow_id = send("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "slow please"}]})
        time.sleep(1.5)
        send("session/cancel", {"sessionId": session_id})
        cancelled = wait_for(slow_id, timeout=60)
        stop = (cancelled.get("result") or {}).get("stopReason")
        print("cancel frame:", json.dumps(cancelled)[:400], "stopReason:", stop)
        results.append(("session/cancel", stop == "cancelled"))

        bash_updates = []
        bash = wait_for(
            send("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "run bash now"}]}),
            timeout=90,
            collect=bash_updates,
        )
        bash_text = ""
        bash_tools = []
        for label, raw in bash_updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "agent_message_chunk":
                bash_text += (update.get("content") or {}).get("text") or ""
            if update.get("sessionUpdate") == "tool_call":
                bash_tools.append(update.get("name") or update.get("title"))
        print("permission seen:", permission_seen[0], "bash tools:", bash_tools, "text:", repr(bash_text))
        results.append(("permission prompt", permission_seen[0]))
        results.append(("bash allowed", "bash" in bash_tools and "tool done" in bash_text))

        wait_for(send("_pure/config/set", {
            "path": ["permissions", "tools", "bash"],
            "value": {"default": "deny", "allow": [], "deny": []},
        }))
        asked_before = permission_seen[0]
        denied_updates = []
        denied = wait_for(
            send("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "run bash again"}]}),
            timeout=90,
            collect=denied_updates,
        )
        denied_tools = []
        denied_text = ""
        for label, raw in denied_updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "tool_call":
                denied_tools.append(update.get("name") or update.get("title"))
            if update.get("sessionUpdate") == "agent_message_chunk":
                denied_text += (update.get("content") or {}).get("text") or ""
        print("deny rule:", denied_tools, "asked:", permission_seen[0] - asked_before, repr(denied_text))
        results.append((
            "permission rule denies bash",
            "bash" in denied_tools
            and permission_seen[0] == asked_before
            and (denied.get("result") or {}).get("stopReason") == "end_turn",
        ))
        wait_for(send("_pure/config/set", {"path": ["permissions", "tools", "bash"], "value": None}))

        confine = wait_for(send("_pure/config/set", {
            "path": ["permissions", "confine_paths"],
            "value": True,
        }))
        results.append((
            "permissions snapshot keeps confine_paths",
            ((confine.get("result") or {}).get("permissions") or {}).get("confine_paths") is True,
        ))
        wait_for(send("_pure/config/set", {"path": ["permissions", "confine_paths"], "value": False}))

        wait_for(send("_pure/config/set", {
            "path": ["permissions", "tools", "bash"],
            "value": {"default": "allow", "allow": [], "deny": []},
        }))
        asked_before = permission_seen[0]
        allowed_updates = []
        allowed = wait_for(
            send("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "run bash now"}]}),
            timeout=90,
            collect=allowed_updates,
        )
        allowed_tools = []
        for label, raw in allowed_updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "tool_call":
                allowed_tools.append(update.get("name") or update.get("title"))
        print("allow rule:", allowed_tools, "asked:", permission_seen[0] - asked_before)
        results.append((
            "permission rule allows bash",
            "bash" in allowed_tools
            and permission_seen[0] == asked_before
            and (allowed.get("result") or {}).get("stopReason") == "end_turn",
        ))
        wait_for(send("_pure/config/set", {"path": ["permissions", "tools", "bash"], "value": None}))

        full = wait_for(send("_pure/config/set", {
            "path": ["permissions", "mode"],
            "value": "full",
        }))
        results.append((
            "permissions mode full",
            ((full.get("result") or {}).get("permissions") or {}).get("mode") == "full",
        ))
        asked_before = permission_seen[0]
        full_updates = []
        full_prompt = wait_for(
            send("session/prompt", {"sessionId": session_id, "prompt": [{"type": "text", "text": "run bash now"}]}),
            timeout=90,
            collect=full_updates,
        )
        full_tools = []
        full_text = ""
        for label, raw in full_updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "tool_call":
                full_tools.append(update.get("name") or update.get("title"))
            if update.get("sessionUpdate") == "agent_message_chunk":
                full_text += (update.get("content") or {}).get("text") or ""
        print("full access bash:", full_tools, "asked:", permission_seen[0] - asked_before, repr(full_text))
        results.append((
            "full access skips approvals",
            "bash" in full_tools
            and permission_seen[0] == asked_before
            and "tool done" in full_text
            and (full_prompt.get("result") or {}).get("stopReason") == "end_turn",
        ))
        default = wait_for(send("_pure/config/set", {
            "path": ["permissions", "mode"],
            "value": "default",
        }))
        results.append((
            "permissions mode back to default",
            ((default.get("result") or {}).get("permissions") or {}).get("mode") == "default",
        ))

        session_full = wait_for(send("session/set_config_option", {
            "sessionId": session_id,
            "configId": "mode",
            "value": "full",
        }))
        session_mode = next(
            (option.get("currentValue")
             for option in (session_full.get("result") or {}).get("configOptions") or []
             if option.get("id") == "mode"),
            None,
        )
        persisted = wait_for(send("_pure/config/get", {"sessionId": session_id}))
        results.append((
            "session mode selects full access",
            session_mode == "full"
            and ((persisted.get("result") or {}).get("permissions") or {}).get("mode") == "full",
        ))
        session_default = wait_for(send("session/set_config_option", {
            "sessionId": session_id,
            "configId": "mode",
            "value": "default",
        }))
        session_mode = next(
            (option.get("currentValue")
             for option in (session_default.get("result") or {}).get("configOptions") or []
             if option.get("id") == "mode"),
            None,
        )
        results.append(("session mode returns to default", session_mode == "default"))

        compact = wait_for(send("_pure/session/compact", {"sessionId": session_id}), timeout=30)
        print("compact frame:", json.dumps(compact)[:300])
        results.append(("_pure/session/compact", "error" not in compact))

        cfg = wait_for(send("_pure/config/get", {"sessionId": session_id}))
        providers = (cfg.get("result") or {}).get("providers") or {}
        print("_pure/config/get providers:", json.dumps(providers)[:600])
        results.append(("_pure/config/get", "custom" in providers and "mock" in providers["custom"]))

        models = wait_for(send("_pure/models/list", {}))
        model_providers = (models.get("result") or {}).get("providers") or []
        print("_pure/models/list:", json.dumps(model_providers)[:600])
        results.append(("_pure/models/list", any(group.get("id") == "mock" for group in model_providers)))

        folders = wait_for(send("_pure/folders", {"path": args.home}))
        results.append(("_pure/folders", "folders" in (folders.get("result") or {})))

        goal = wait_for(send("session/set_config_option", {"sessionId": session_id, "configId": "goal", "value": "finish pure"}))
        print("set goal frame:", json.dumps(goal)[:1000])
        goal_value = next((o.get("currentValue") for o in (goal.get("result") or {}).get("configOptions") or [] if o.get("id") == "goal"), None)
        print("goal ->", goal_value)
        results.append(("goal option", goal_value == "finish pure"))

        mode = wait_for(send("session/set_config_option", {"sessionId": session_id, "configId": "mode", "value": "plan"}))
        print("set mode frame:", json.dumps(mode)[:1000])
        mode_value = next((o.get("currentValue") for o in (mode.get("result") or {}).get("configOptions") or [] if o.get("id") == "mode"), None)
        print("mode ->", mode_value)
        results.append(("mode option", mode_value == "plan"))

        swarm = wait_for(send("session/set_config_option", {"sessionId": session_id, "configId": "swarm", "value": "on"}))
        swarm_value = next((o.get("currentValue") for o in (swarm.get("result") or {}).get("configOptions") or [] if o.get("id") == "swarm"), None)
        print("swarm ->", swarm_value)
        results.append(("swarm option on", swarm_value == "on"))
        swarm_off = wait_for(send("session/set_config_option", {"sessionId": session_id, "configId": "swarm", "value": "off"}))
        swarm_off_value = next((o.get("currentValue") for o in (swarm_off.get("result") or {}).get("configOptions") or [] if o.get("id") == "swarm"), None)
        results.append(("swarm option off", swarm_off_value == "off"))

        rename = wait_for(send("_pure/session/rename", {"sessionId": session_id, "title": "Probe Session"}))
        results.append(("_pure/session/rename", "error" not in rename))

        sessions_list = list_all_sessions(send, wait_for)
        listed = next(
            (entry for entry in sessions_list if entry.get("sessionId") == session_id),
            {},
        )
        print("session/list entry:", json.dumps(listed)[:400])
        results.append(("session/list title", listed.get("title") == "Probe Session"))

        exported = wait_for(send("_pure/session/export", {"sessionId": session_id}))
        markdown = (exported.get("result") or {}).get("markdown") or ""
        print("export markdown:", repr(markdown[:300]))
        results.append((
            "_pure/session/export",
            "use the todo tool" in markdown and "tool done" in markdown,
        ))

        forked = wait_for(send("_pure/session/fork", {"sessionId": session_id}))
        fork_id = (forked.get("result") or {}).get("sessionId")
        print("fork:", json.dumps(forked.get("result"))[:300])
        results.append(("_pure/session/fork", bool(fork_id)))
        if fork_id:
            fork_load = wait_for(send("session/load", {"sessionId": fork_id, "cwd": cwd, "mcpServers": []}), timeout=60)
            results.append(("fork load", "error" not in fork_load))
            fork_entry = next(
                (entry for entry in list_all_sessions(send, wait_for)
                 if entry.get("sessionId") == fork_id),
                {},
            )
            results.append((
                "fork parent listed",
                ((fork_entry.get("_meta") or {}).get("pure") or {}).get("parentSessionId") == session_id,
            ))

        partial = wait_for(send("_pure/session/fork", {
            "sessionId": session_id,
            "before": {"text": "delegate please", "occurrence": 0},
        }))
        partial_id = (partial.get("result") or {}).get("sessionId")
        print("partial fork:", json.dumps(partial.get("result"))[:300])
        results.append(("_pure/session/fork before prompt", bool(partial_id)))
        if partial_id:
            replay = []
            partial_load = wait_for(
                send("session/load", {"sessionId": partial_id, "cwd": cwd, "mcpServers": []}),
                timeout=60,
                collect=replay,
            )
            user_texts = []
            for label, raw in replay:
                if label != "out" or not raw.startswith("{"):
                    continue
                try:
                    frame = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                update = (frame.get("params") or {}).get("update") or {}
                if update.get("sessionUpdate") == "user_message_chunk":
                    user_texts.append((update.get("content") or {}).get("text") or "")
            print("partial fork prompts:", user_texts)
            results.append((
                "fork before prompt truncates",
                "error" not in partial_load
                and user_texts == ["use the todo tool", "run codemode now"],
            ))

        # goal/plan/swarm are features, not tools: their tools only reach the
        # model context while the user switched them on.
        def mode_tools(config_id, value):
            wait_for(send("session/set_config_option", {
                "sessionId": session_id, "configId": config_id, "value": value,
            }))
            wait_for(send("session/prompt", {
                "sessionId": session_id,
                "prompt": [{"type": "text", "text": "hello"}],
            }), timeout=90)
            return last_tools()

        hidden = mode_tools("mode", "default")
        print("tools with modes off:", hidden)
        results.append((
            "on-demand tools hidden",
            not any(name in hidden for name in ("goal", "exit_plan", "swarm"))
            and "todo" in hidden,
        ))
        results.append(("swarm tool loads on demand", "swarm" in mode_tools("swarm", "on")))
        results.append(("swarm tool unloads", "swarm" not in mode_tools("swarm", "off")))
        results.append(("plan tool loads on demand", "exit_plan" in mode_tools("mode", "plan")))

        # The mock proposes an exit_plan call; wait_for auto-approves the
        # review. The approved plan must leave plan mode and unload the tool.
        review = []
        wait_for(
            send("session/prompt", {
                "sessionId": session_id,
                "prompt": [{"type": "text", "text": "make a plan"}],
            }),
            timeout=90,
            collect=review,
        )
        plan_updates = []
        asked = 0
        for label, raw in review:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            if frame.get("method") == "session/request_permission":
                asked += 1
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "plan":
                plan_updates.append(update)
        print("plan review permission asks:", asked)
        results.append(("exit_plan asks for a review", asked == 1))
        results.append(("exit_plan approval emits a plan", bool(plan_updates)))
        wait_for(send("session/prompt", {
            "sessionId": session_id,
            "prompt": [{"type": "text", "text": "hello"}],
        }), timeout=90)
        results.append(("plan approval unloads exit_plan", "exit_plan" not in last_tools()))

        image = base64.b64decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
        )
        image_sha = hashlib.sha256(image).hexdigest()
        image_new = wait_for(send("session/new", {"cwd": cwd, "mcpServers": []}), timeout=60)
        image_session = (image_new.get("result") or {}).get("sessionId")
        image_updates = []
        image_prompt = wait_for(
            send("session/prompt", {"sessionId": image_session, "prompt": [
                {"type": "text", "text": "look at this image"},
                {"type": "image", "data": base64.b64encode(image).decode(), "mimeType": "image/png"},
            ]}),
            timeout=90,
            collect=image_updates,
        )
        print("image prompt:", json.dumps(image_prompt)[:400])
        saved_image = wait_for(send("_pure/image/save", {
            "sessionId": image_session,
            "sha256": image_sha,
        }))
        saved_path = os.path.join(
            args.home, "acp-sessions", image_session or "", "generated", f"{image_sha[:16]}.png"
        )
        print("image save:", json.dumps(saved_image)[:300], "exists:", os.path.exists(saved_path))
        results.append((
            "_pure/image/save",
            "error" not in saved_image and os.path.exists(saved_path),
        ))

        auth = wait_for(send("_pure/auth/set", {"provider": "mock", "key": "test-key"}))
        results.append(("_pure/auth/set", "error" not in auth))

        added = wait_for(send("_pure/config/set", {
            "sessionId": session_id,
            "path": ["providers", "custom", "lab2"],
            "value": {
                "kind": "openai",
                "base_url": f"http://127.0.0.1:{server.server_port}/v1",
                "display_name": "Lab Two",
                "api_key_env": "",
                "headers": {},
                "models": [{
                    "id": "mock-1", "name": "Mock One", "context": 200000,
                    "max_output": 8192, "reasoning": True,
                    "input": ["text", "image"],
                    "cost": {"input": 1.0, "output": 2.0, "cache_read": 0.5},
                    "efforts": ["low", "high"],
                }],
            },
        }))
        lab2 = (((added.get("result") or {}).get("providers") or {}).get("custom") or {}).get("lab2")
        results.append(("_pure/config/set", bool(lab2)))
        try:
            lab2_model = (lab2 or {}).get("models", [{}])[0]
            print("lab2 config echo:", json.dumps(lab2_model)[:500])
            results.append((
                "config/get keeps model metadata",
                lab2_model.get("max_output") == 8192
                and lab2_model.get("reasoning") is True
                and lab2_model.get("input") == ["text", "image"]
                and (lab2_model.get("cost") or {}).get("cache_read") == 0.5
                and lab2_model.get("efforts") == ["none", "low", "high"],
            ))
        except (IndexError, AttributeError):
            results.append(("config/get keeps model metadata", False))
        try:
            with open(os.path.join(args.home, "agent", "models.json"), encoding="utf-8") as handle:
                saved = json.load(handle)["providers"]["lab2"]["models"][0]
            print("lab2 models.json:", json.dumps(saved)[:500])
            tmap = saved.get("thinkingLevelMap") or {}
            results.append((
                "models.json keeps model metadata",
                saved.get("maxTokens") == 8192
                and saved.get("reasoning") is True
                and (saved.get("cost") or {}).get("cacheRead") == 0.5
                and tmap.get("low") == "low"
                and "minimal" in tmap and tmap.get("minimal") is None,
            ))
        except (KeyError, IndexError, OSError):
            results.append(("models.json keeps model metadata", False))

        probe = wait_for(send("_pure/config/test", {"provider": {"id": "lab2", "kind": "openai", "baseUrl": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "test-key"}}), timeout=60)
        probe_result = probe.get("result") or {}
        print("config/test:", json.dumps(probe_result)[:500])
        results.append(("_pure/config/test", probe_result.get("ok") is True and bool(probe_result.get("models"))))

        keyless = wait_for(send("_pure/config/test", {"provider": {"id": "lab2", "kind": "openai", "baseUrl": f"http://127.0.0.1:{server.server_port}/v1", "apiKeyEnv": ""}}), timeout=60)
        keyless_result = keyless.get("result") or {}
        print("config/test keyless:", json.dumps(keyless_result)[:400])
        results.append(("_pure/config/test keyless", keyless_result.get("ok") is True))

        directory = wait_for(send("_pure/providers/directory", {"url": f"http://127.0.0.1:{server.server_port}/api.json"}), timeout=60)
        dir_providers = (directory.get("result") or {}).get("providers") or []
        print("directory:", json.dumps(dir_providers)[:600])
        lab_entry = next((entry for entry in dir_providers if entry.get("id") == "lab"), {})
        lab_models = lab_entry.get("models") or []
        results.append((
            "_pure/providers/directory",
            lab_entry.get("kind") == "anthropic"
            and [model.get("id") for model in lab_models] == ["m1"]
            and (lab_models[0] if lab_models else {}).get("output") == 100,
        ))

        with open(os.path.join(cwd, "probe.txt"), "w", encoding="utf-8") as handle:
            handle.write("probe")
        fs_result = wait_for(send("_pure/fs", {"sessionId": session_id, "op": "list", "path": ""}))
        entries = (fs_result.get("result") or {}).get("entries") or []
        results.append(("_pure/fs", any(entry.get("path") == "probe.txt" for entry in entries)))

        themes = wait_for(send("_pure/themes", {}))
        options = wait_for(send("_pure/options/list", {}))
        results.append(("_pure/themes", "themes" in (themes.get("result") or {})))
        results.append(("_pure/options", "options" in (options.get("result") or {})))
        option_rows = (options.get("result") or {}).get("options") or []
        subagent_names = {"agent_max_running", "agent_max_turns", "agent_timeout"}
        results.append((
            "engine options listed",
            any(option.get("name") == "thinking_level" for option in option_rows)
            and subagent_names.issubset({option.get("name") for option in option_rows}),
        ))
        option_set = wait_for(send("_pure/options/set", {"name": "thinking_level", "value": "high"}))
        option_value = next(
            (option.get("value") for option in (option_set.get("result") or {}).get("options") or []
             if option.get("name") == "thinking_level"),
            None,
        )
        results.append(("engine option set", option_value == "high"))
        subagent_set = wait_for(send("_pure/options/set", {
            "name": "agent_max_turns",
            "value": 5,
        }))
        subagent_value = next(
            (option.get("value") for option in (subagent_set.get("result") or {}).get("options") or []
             if option.get("name") == "agent_max_turns"),
            None,
        )
        results.append(("subagent option set", subagent_value == 5))
        wait_for(send("_pure/options/set", {"name": "agent_max_turns", "value": 8}))

        mcp_add = wait_for(send("_pure/config/set", {
            "path": ["mcp", "servers", "broken"],
            "value": {"command": "pure-probe-no-such-mcp-binary", "args": []},
        }))
        broken = (((mcp_add.get("result") or {}).get("mcp") or {}).get("servers") or {}).get("broken")
        results.append(("_pure/config/set mcp", bool(broken)))
        mcp_updates = []
        mcp_new = wait_for(
            send("session/new", {"cwd": cwd, "mcpServers": []}),
            timeout=90,
            collect=mcp_updates,
        )
        mcp_session = (mcp_new.get("result") or {}).get("sessionId")
        deadline = time.time() + 2.0
        while time.time() < deadline:
            try:
                label, raw = lines.get(timeout=0.2)
            except queue.Empty:
                continue
            if label == "out" and raw and raw.startswith("{"):
                try:
                    frame = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                mcp_updates.append((label, raw))
        statuses = []
        for label, raw in mcp_updates:
            if label != "out" or not raw.startswith("{"):
                continue
            try:
                frame = json.loads(raw)
            except json.JSONDecodeError:
                continue
            update = (frame.get("params") or {}).get("update") or {}
            if update.get("sessionUpdate") == "_pure/mcp_status":
                statuses.append((update.get("name"), update.get("status")))
        print("mcp new:", json.dumps(mcp_new)[:500], "statuses:", statuses)
        results.append((
            "bad MCP server does not block session",
            bool(mcp_session) and ("broken", "failed") in statuses,
        ))
        wait_for(send("_pure/config/set", {"path": ["mcp", "servers", "broken"], "value": None}))

    proc.stdin.close()
    try:
        proc.wait(timeout=20)
    except subprocess.TimeoutExpired:
        proc.kill()
    stderr = proc.stderr.read().decode("utf-8", "replace")
    if stderr.strip():
        print("--- stderr tail ---")
        print(stderr[-2500:])

    proc2 = subprocess.Popen(
        [args.backend, "rpc"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )
    lines2 = queue.Queue()

    def reader2(stream, label):
        for raw in iter(stream.readline, b""):
            lines2.put((label, raw.decode("utf-8", "replace").rstrip("\r\n")))
        lines2.put((label, None))

    threading.Thread(target=reader2, args=(proc2.stdout, "out"), daemon=True).start()
    threading.Thread(target=reader2, args=(proc2.stderr, "err"), daemon=True).start()
    next2 = [100]

    def send2(method, params):
        next2[0] += 1
        proc2.stdin.write((json.dumps({"jsonrpc": "2.0", "id": next2[0], "method": method, "params": params}) + "\n").encode())
        proc2.stdin.flush()
        return next2[0]

    def wait2(request_id, timeout=60):
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                label, text = lines2.get(timeout=0.2)
            except queue.Empty:
                continue
            if text is None:
                break
            if label == "out" and text.startswith("{"):
                try:
                    frame = json.loads(text)
                except json.JSONDecodeError:
                    continue
                if frame.get("id") == request_id:
                    return frame
        raise TimeoutError(f"no response for id {request_id}")

    wait2(send2("initialize", {
        "protocolVersion": 1,
        "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}, "terminal": False},
        "clientInfo": {"name": "acp-probe", "version": "0.0.1"},
    }))
    cfg2 = wait2(send2("_pure/config/get", {}))
    providers2 = (cfg2.get("result") or {}).get("providers") or {}
    custom2 = providers2.get("custom") or {}
    print("restart providers:", json.dumps(custom2)[:400])
    results.append(("restart keeps provider", "lab2" in custom2))
    models2 = wait2(send2("_pure/models/list", {}))
    groups2 = (models2.get("result") or {}).get("providers") or []
    results.append(("restart keeps models", any(group.get("id") == "lab2" for group in groups2)))
    sessions2 = list_all_sessions(send2, wait2)
    listed2 = next(
        (entry for entry in sessions2 if entry.get("sessionId") == session_id),
        {},
    )
    results.append(("restart keeps title", listed2.get("title") == "Probe Session"))
    loaded2 = wait2(
        send2("session/load", {"sessionId": session_id, "cwd": cwd, "mcpServers": []}),
        timeout=90,
    )
    goal2 = next(
        (option.get("currentValue") for option in (loaded2.get("result") or {}).get("configOptions") or []
         if option.get("id") == "goal"),
        None,
    )
    results.append(("restart keeps goal", goal2 == "finish pure"))
    with open(os.path.join(args.home, "agent", "auth.json"), encoding="utf-8") as handle:
        auth2 = json.load(handle)
    results.append(("auth.json keeps key", (auth2.get("mock") or {}).get("key") == "test-key"))
    proc2.stdin.close()
    try:
        proc2.wait(timeout=20)
    except subprocess.TimeoutExpired:
        proc2.kill()

    print("\n=== summary ===")
    failed = 0
    for name, ok in results:
        print(f"{'PASS' if ok else 'FAIL'}  {name}")
        failed += 0 if ok else 1
    print(f"exit={proc.returncode}")
    server.shutdown()
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
