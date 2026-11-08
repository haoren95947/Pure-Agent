# pure-rtk

Compresses `bash` / `powershell` tool output with the RTK binary bundled under
`vendor/rtk/`. Enabled by default; turn it off on the desktop Plugins page (or
set `PURE_RTK=0`).

## What it does

After a shell command finishes, the host hands the extension its result. The
extension maps the command to an RTK pipe filter when it recognizes one
(`git status|log|diff`, `cargo test`, `pytest`, `go test|build`, `tsc`,
`vitest`, `mypy`, `ruff`, `prettier`, `phpunit`, `grep`, `rg`, `find`, logs,
…) and runs:

```
rtk pipe [--filter <name>]   # stdin: the tool's text, stdout: the compact form
```

The model reads the shorter text. If RTK has no filter, `rtk pipe` auto-detects
the format. Nothing is re-executed, and a result is only replaced when it is
strictly smaller.

Failed calls are covered too: a bash command that exits non-zero still owns its
output, and the host offers that structured failure to the same transform.

## Configuration

`<agent dir>/rtk.json` (read per call, defaults shown):

```json
{
  "enabled": true,
  "minBytes": 512,
  "timeoutMs": 3000,
  "awareness": false
}
```

| Key | Env override | Meaning |
| --- | --- | --- |
| `enabled` | `PURE_RTK=0` | Master switch |
| `minBytes` | `PURE_RTK_MIN_BYTES` | Skip results smaller than this |
| `timeoutMs` | `PURE_RTK_TIMEOUT_MS` | Kill `rtk pipe` after this (50–30000) |
| `awareness` | `PURE_RTK_AWARENESS=1` | Append a short note telling the model the binary exists (costs prompt tokens) |

`PURE_RTK_BIN` points at a specific `rtk` executable and wins over the lookup.

## Binary lookup

In order, the first existing file wins:

1. `PURE_RTK_BIN`
2. `vendor/rtk/<platform>/rtk[.exe]` under any `RPI_EXTENSIONS_DIR` entry
3. `<host exe>/extensions/vendor/rtk/<platform>/`, then `<host exe>/vendor/rtk/<platform>/`
4. `crates/pure-extensions-packages/pure-rtk/vendor/rtk/<platform>/` (dev runs)
5. `rtk` on `PATH`

Platform directories: `win32-x64`, `darwin-x64`, `darwin-arm64`,
`linux-x64`, `linux-arm64`. Only the first three are vendored today.

## Deliberate limits

- **No command rewriting.** The extension ABI can veto a tool call but not
  rewrite its arguments, so `git status` is not turned into `rtk git status`.
  Compression happens on the finished output instead.
- **Shell tools only.** `read`, `grep`, `find` and `ls` are left alone: their
  output is already structured for follow-up tool calls, and rewriting `read`
  text would break `edit`'s `old_string` matching.
- **Fail open.** Missing binary, spawn error, timeout, non-zero exit, non-UTF-8
  output, or a result that is not smaller all keep the original text.

## Packaging

`scripts/build.ps1` copies the DLL to `dist/extensions/` and the whole
`vendor/` tree to `dist/extensions/vendor/`. The loader only picks up files with
a cdylib suffix, so the binaries under `vendor/` are never mistaken for plugins.