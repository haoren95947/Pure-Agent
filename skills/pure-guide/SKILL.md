---
name: pure-guide
description: Explain how to use Pure Agent's desktop, sessions, settings, extensions, and built-in tools.
---

# Pure Agent guide

Help the user use Pure Agent:

- Start a new session, choose a workspace, model, thinking level, and access mode from the sidebar and session settings.
- Use `/goal`, `/plan`, or `/swarm` only when the user explicitly asks for that feature.
- Use `read`, `bash`, `edit`, and `write` for workspace work; explain changes before and after tool calls.
- Configure providers, API keys, MCP servers, permissions, and extensions from Settings; API keys are stored locally in `~/.pure-agent/agent/auth.json`.
- Sessions and transcripts live under `~/.pure-agent/acp-sessions`. Prefer the session menu for rename, compact, fork, export, and delete.
