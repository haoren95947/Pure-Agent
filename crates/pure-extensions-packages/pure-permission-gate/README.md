# pure-permission-gate

A **generic tool-call permission gate** for Pure Agent. Before a potentially
dangerous tool runs (`bash`, `powershell`, `write`, `edit`), it asks the user to
confirm through the host's extension UI dialog and blocks the call if declined.
`agent/permissions.json` can allow or deny calls without a dialog, and
`confine_paths` keeps file tools inside the session workspace.

```
agent loop ──before_tool_call──▶ on("tool_call")  ──▶ pure-permission-gate
                                                        │  permissions.json
                                                        │  UiDialog confirm
                                                        ▼
                           TUI dialog   or   `--mode rpc` UI bridge ──▶ client
```

It depends on two generic host capabilities — the `on("tool_call")` veto
(`before_tool_call`) and the `UiDialog` runtime action (17) — and knows nothing
about any protocol. The same plugin works in the interactive TUI and, via the
rpc UI bridge, under an ACP client such as `pure-acp` (which renders the confirm
as `session/request_permission`).

## Enable

Off by default, because a headless client that cannot answer UI dialogs would
otherwise hang the run.

```bash
pure --permission-gate          # flag
RPI_PERMISSION_GATE=1 pure      # or env
```

`pure-acp` passes `--permission-gate` to its child sessions by default. Use
`--bypass` to skip those approvals, or `RPI_ACP_NO_PERMISSION_GATE=1` to stop
ACP from adding the flag. `--bypass` wins over both activation methods. The
desktop's **Full access** mode launches sessions with `--bypass`.

## Behavior

| Tool | Summary shown | Gate |
| --- | --- | --- |
| `bash` | the command (truncated) | yes |
| `powershell` | the command (truncated) | yes |
| `write` | `write <path> (<n> bytes)` | yes |
| `edit` | `edit <path> (<n> replacements)` | yes |
| anything else | — | not gated |

* **Allow** → the tool runs unchanged.
* **Decline** → the handler returns `EVENT_HANDLER_ABORT`; the host blocks the
  tool and the model sees an error result.
* **No UI / dialog failure / timeout** → **blocked** (fail closed).

## Rules

Read from `<agent dir>/permissions.json` on every tool call, so Settings edits
apply without reloading the session. `deny` wins over `allow`, which wins over
`default` (`ask` when omitted). Patterns are globs matched against the shell
command or file path:

```json
{
  "confine_paths": true,
  "tools": {
    "bash":  {"default": "ask", "allow": ["echo *", "cargo test*"], "deny": ["rm -rf *"]},
    "write": {"default": "ask", "allow": ["src/**"], "deny": ["*.env"]}
  }
}
```

- `default: "allow"` / `"deny"` skip the dialog; `"ask"` (or a missing rule)
  prompts.
- With `confine_paths: true`, `read` / `write` / `edit` / `ls` / `find` /
  `grep` calls whose `path` resolves outside the session working directory are
  blocked without a prompt.

## Install

Copy `pure_permission_gate.dll` into the host's `extensions` directory (the
desktop build stages it automatically).

## Build and test

```sh
cargo build -p pure-permission-gate
cargo test -p pure-permission-gate
cargo clippy -p pure-permission-gate --all-targets -- -D warnings
```

## Limitations

* Only the tools in the table above are gated; rules for other tools are
  ignored (except `confine_paths` on the listed file tools).
* One confirm per tool call (no batching of a parallel batch).
* Shell commands are matched as text; the gate does not parse arguments.

## License

MIT. Ported from `rpi-permission-gate` 0.1.4 with the local rule and
`confine_paths` layer.
