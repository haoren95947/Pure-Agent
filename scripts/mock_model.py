"""Minimal OpenAI-compatible mock server for manual desktop testing.

Usage: python scripts/mock_model.py [--port 8799]

GET  /v1/models          -> one model
POST /v1/chat/completions -> SSE stream; replies with plain text, or a todo
                             tool call when the last user message is not a
                             greeting. Enough to exercise Pure Agent's chat,
                             tool card and plan panel without a real API key.
"""
import argparse
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

REPLY = "mock reply: hello from pure"


class Handler(BaseHTTPRequestHandler):
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
        messages = body.get("messages") or []

        def flatten(message):
            content = message.get("content") or ""
            if isinstance(content, str):
                return content
            return " ".join(str(part.get("text") or "") for part in content if isinstance(part, dict))

        last_user = ""
        for message in reversed(messages):
            if message.get("role") == "user":
                last_user = flatten(message).lower()
                break
        last_role = messages[-1].get("role") if messages else None
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        joined = last_user
        if last_role == "tool":
            self._text("tool done: all good")
        elif "delegated subagent check" in joined:
            self._text("child says hi")
        elif "summar" in joined or "compact" in joined:
            self._text("mock summary of earlier turns")
        elif "hello" in joined:
            self._text(REPLY)
        elif "failing tests" in joined:
            self._text_slow()
        elif "unhandled errors" in joined:
            self._reasoning("Checking each module for unwrap and panic sites.")
            self._tool_call("bash", {"command": "echo permission ok"})
        elif "run codemode" in joined:
            self._tool_call(
                "codemode",
                {"code": "return {ok: true, message: 'codemode ran'};"},
            )
        elif "delegate" in joined:
            self._tool_call(
                "delegate_task",
                {"task": "delegated subagent check", "role": "tester", "maxTurns": 2},
            )
        else:
            self._tool_call("todo", {"action": "add", "text": "desktop smoke todo"})

    def _frame(self, frame):
        self.wfile.write(("data: " + json.dumps(frame) + "\n\n").encode())
        self.wfile.flush()

    def _text(self, reply):
        for chunk in (reply[:10], reply[10:]):
            self._frame({
                "id": "chatcmpl-mock",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": "mock-1",
                "choices": [{"index": 0, "delta": {"content": chunk}, "finish_reason": None}],
            })
            time.sleep(0.02)
        self._finish("stop")

    def _reasoning(self, text):
        for chunk in (text[:20], text[20:]):
            self._frame({
                "id": "chatcmpl-mock",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": "mock-1",
                "choices": [{"index": 0, "delta": {"reasoning_content": chunk}, "finish_reason": None}],
            })
            time.sleep(0.05)

    def _text_slow(self):
        pieces = ["slow ", "reply ", "still ", "streaming ", "for ", "cancel"]
        for piece in pieces:
            self._frame({
                "id": "chatcmpl-mock",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": "mock-1",
                "choices": [{"index": 0, "delta": {"content": piece}, "finish_reason": None}],
            })
            time.sleep(0.9)
        self._finish("stop")

    def _tool_call(self, name, arguments):
        self._frame({
            "id": "chatcmpl-mock",
            "object": "chat.completion.chunk",
            "created": int(time.time()),
            "model": "mock-1",
            "choices": [{
                "index": 0,
                "delta": {"tool_calls": [{
                    "index": 0,
                    "id": "call_mock_1",
                    "type": "function",
                    "function": {"name": name, "arguments": json.dumps(arguments)},
                }]},
                "finish_reason": None,
            }],
        })
        self._finish("tool_calls")

    def _finish(self, reason):
        self._frame({
            "id": "chatcmpl-mock",
            "object": "chat.completion.chunk",
            "created": int(time.time()),
            "model": "mock-1",
            "choices": [{"index": 0, "delta": {}, "finish_reason": reason}],
            "usage": {"prompt_tokens": 8, "completion_tokens": 6, "total_tokens": 14},
        })
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8799)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"mock model server on http://127.0.0.1:{args.port}/v1", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
