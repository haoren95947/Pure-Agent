# pure-codemode

Port of `rpi-codemode` 0.1.15: execute one bounded JavaScript **async function
body** against the tools already active in the calling invocation. No extra
model turn, MCP server, subprocess interpreter or TypeScript compiler. Ordinary
tools remain visible and usable normally.

The extension registers one tool, `codemode`, and requires the pure host's
ToolProgram **action 30 / protocol v1** with all six exact integer v1
capabilities: `resultEnvelope`, `hostAccounting`, `parentProgress`,
`losslessValues`, `resultReferences`, `contentAttachments`. Older or mismatched
hosts fail before guest execution; there is no fallback.

## Public API

```json
{"action":"list","offset":0,"limit":32}
```

Returns `{protocolVersion:1,capabilities:{…},tools:[{name,description,
parameters}],nextOffset:number|null}` in the result details. Follow `nextOffset`
for another page. Only the effective current invocation catalog is visible, not
the root registry; self/ancestor tools are excluded.

```json
{"code":"const r = await tools.read({path:'Cargo.toml'}); print(r.content.length); return r.details;","timeoutMs":30000}
```

| Parameter | Contract |
| --- | --- |
| action | execute (default) or list |
| code | Required for execute, nonblank, ≤65536 UTF-8 bytes; strict-mode JavaScript async body with await/return, not TypeScript/module/full request |
| timeoutMs | Strict integer 1..120000, default 30000; includes catalog discovery |
| offset | List offset 0..1024, default 0 |
| limit | List page size 1..128, default 32 |

Registration sets `x-pure-execution-mode: sequential` and
`x-pure-result-envelope: 1`. The outer program cannot overlap a model tool batch
peer; independent nested Parallel tools can overlap. The host rejects starts
without Sequential admission.

## Guest globals

- `await tools.<name>(args)` / `await tools.call(name,args)` require JSON object
  arguments. Use `tools.call` for punctuation or collisions with helper names
  **call/list/emit/result/release**. A native `isError` rejects with an Error
  carrying the finalized result/receipt at `error.result`; lifecycle failures
  abort even if caught.
- Results retain native content (including images), details, terminate,
  addedToolNames and usage where supplied. Small results are mutable; large
  receipts are read-only.
- `tools.list()` returns a fresh JSON copy of the bounded frozen catalog.
- `tools.emit(resultOrContentArray, indices?)` selects ordered inline or genuine
  large-receipt text/image content while returning another summary.
- `tools.result(nativeResultOrErrorResult)` selects exactly one native child
  result with authoritative isError; returns undefined, not another tool result.
- `tools.release(nativeResult)` releases a large receipt.
- `print`, `console.log/info/warn/error` capture bounded logs.
- Helpers/globals are frozen/nonreplaceable. No direct filesystem, network,
  process, require, timers, module loader or ambient host context is installed.
  Allowed native tools retain their authority.

Independent starts occur before completion polling, so concurrency is genuine:

```js
const [a, b] = await Promise.all([
  tools.read({path: 'Cargo.toml'}),
  tools.read({path: 'crates/pure-extensions-packages/pure-codemode/Cargo.toml'})
]);
return {a: a.content, b: b.content};
```

At most **8 active starts** (128 per tree), with no excess-call queue; batch
larger sets yourself. Do not parallelize conflicting mutations. Native
arguments/schema validation, hooks, approvals, cwd and effective tools remain
authoritative.

## Results

Ordinary inline Done envelopes look like:

```json
{"$rpiToolResult":1,"isError":false,"result":{"content":[{"type":"text","text":"JSON summary"}],"details":{"status":"completed","result":42,"error":null,"logs":[],"calls":[],"durationMs":1,"terminate":false,"isError":false},"terminate":false}}
```

Summary statuses are completed/error/cancelled/timed_out/terminated. The **1 MiB
final envelope** bound includes framing; oversized inline output compacts to a
Done error rather than truncating into success.

| Native serialized result | Guest and tools.result behavior |
| --- | --- |
| Nonterminal **≤1 MiB** | Materialized mutable guest object, native reference immediately released. Selection snapshots the current guest-mutated supported content/details. |
| **>1 MiB** | Frozen lazy receipt with private WeakMap provenance. First field inspection reads bounded chunks and deep-freezes the parsed receipt in the same 32 MiB guest heap. Selection forwards the **original native result without materialization**. |
| Terminal child, either size | Never resumes JS; directly attaches the original native receipt and cancels/disposes other calls. |

Exact selection is allowed once:

```js
return tools.result(await tools.read({path: 'tiny.png'}));
```

`tools.emit` composes ordered output from multiple receipts and inline blocks
(128 selected parts/blocks, 32 MiB serialized content array), e.g.:

```js
const first = await tools.call('large_tool', {});
tools.emit([{type:'text',text:'before'}]);
tools.emit(first);
tools.emit(second, [2, 1, 0, 1]);
return {processed: 2};
```

`tools.result` cannot be mixed with `tools.emit`. Selecting or emitting a
receipt pins it; `tools.release` then fails before the native free. Repeated,
fabricated or conflicting selections latch failure even if caught.

## Bounds

| Resource | Bound |
| --- | --- |
| Guest QuickJS charged allocations | 32 MiB, including conservative allocation overhead |
| Guest stack | 256 KiB |
| Code | 64 KiB UTF-8 |
| Arguments / host response frame / generated returned JSON / final envelope | 1 MiB each; native attachment payload is not inline |
| Native retained references | 16/program, shared 64 MiB invocation-tree storage quota |
| Catalog | 1024 entries / 1 MiB; pages 1..128 |
| Logs | 256 entries / 64 KiB text total |
| Starts | 128/tree total / 8 active; 16 retained requests/program |
| Error text | 4096 UTF-8 bytes in summary; content diagnostic reason 512 UTF-8 bytes |

**RegExp is unsupported**, including literals, constructor and regex-backed
string operations: the pinned backtracking engine cannot poll interrupts. Plain
string replace/split remain available. Deadlines are cooperative; QuickJS
interrupts ordinary loops but other native builtins may finish first.

This is a bounded language environment, **not an OS security sandbox**. Allowed
shell/file/network tools retain their permissions and effects; errors,
cancellation and release never roll back or retry.

## Build and test

```sh
cargo build --release -p pure-codemode
cargo test -p pure-codemode
cargo clippy -p pure-codemode --all-targets -- -D warnings
```

Use `--tools read,write,codemode` (or similar) when starting a session so the
program can see the tools it intends to call.
