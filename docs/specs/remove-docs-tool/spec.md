# Remove the docs tool

## 背景与目标

内核 `pure-cli` 自带一个 `docs` 内置工具（`src/docs_tool.rs`），用于检索本地文档。
桌面产品不使用它，却在每个会话的工具表与系统提示里占位。

目标：删除该内置工具及其注册、系统提示片段与保留名，工具表与系统提示不再出现 `docs`。

## 非目标

- 不改变 `read`/`bash`/`edit`/`write` 内置工具。
- 不改变内置名保留列表之外的外部/扩展工具行为。
- 不改变 ACP 线协议。

## 用户可见行为

- 模型可用工具中不再有 `docs`。
- 默认系统提示的 “Available tools” 片段不再列出 `docs`。
- `pure --help` 的工具说明为 `read, bash, edit, write`。
- 外部工具被命名为 `docs` 时不再被当作内置保留名拒绝。

## 接口/实现变更

- 删除 `crates/pure-engine/pure-cli/src/docs_tool.rs` 与 `lib.rs` 的模块声明。
- `session.rs`：移除 `create_docs_tool` 导入、`BUILTIN_TOOL_NAMES` 中的 `"docs"`、
  系统提示片段匹配分支与注册代码；相关单测改断言 `docs` 不在工具表。
- `args.rs`：帮助文本去掉 `docs`。
- `modes.rs`：外部工具保留名去掉 `"docs"`。
- 测试夹具中 `{"defaultTools":["docs"]}` 改为真实工具 `read`。

## 影响面

`pure-cli` 及依赖它的 `pure-backend`；扩展与桌面不变。

## 验收

```powershell
cargo test -j 1 -p pure-cli -- --test-threads=1
cargo build -j 1 -p pure-backend
```
