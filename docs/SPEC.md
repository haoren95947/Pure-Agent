# Pure Agent 规格说明（SPEC）

> 状态：v2（Pure Agent 桌面 + pure ACP 内核）
> 目标平台：Windows 优先（eframe 桌面）
> 形态：双可执行文件（桌面 + 双角色后端）、stdio ACP v1

## 1. 目标

交付可直接运行的桌面 Agent `Pure Agent`：

- **桌面壳**：`crates/pure-desktop/app` 的 `pure-desktop-app`，产品化定制见计划 4.1：
  引擎文件名 `pure-backend`、窗口标题与产物名 `pure-agent`、数据目录 `~/.pure-agent`；保存成功后关闭
  设置表单（`Settings::follow` 使用“写入已落地”信号）；会话操作 `⋯` 菜单放到侧栏
  每个会话行末尾（重命名、置顶、分叉、压缩、导出、复制 ID、关闭、归档按行执行）；
  Plugins 页改为展示内核扩展（goal/todo/subagents/codemode/permission-gate/rtk）；移除
  Vim 模式与 initialize 能力提示条（pure-acp 不提供 steering）。
- **ACP 内核**：`crates/pure-engine/pure-acp`（复用 pure-extcore），新增
  `pure-backend` 入口负责环境与默认配置，随后调用 `pure_acp::protocol::run`。
- **Agent 运行时**：内核 0.21.0 作为库链接进 `pure-backend`；每个 ACP 会话由同一个
  `pure-backend.exe --mode rpc` 子进程承载。
- **能力复用**：goal/todo/subagents/codemode/permission-gate/rtk/skills 全部使用
  `pure-goal`、`pure-todo`、`pure-subagents`、`pure-codemode`、
  `pure-permission-gate`、`pure-rtk` 与内核原生 `SKILL.md`，不写并行实现。

不允许的替代方案：自研 ACP 循环、自建会话存储、自写工具流或流式翻译；继续开发
用内置 goal/todo 取代内核扩展。

## 2. 非目标

- 不追求与上游引擎端 `_pure/*` 的全集兼容（undo/rewind、插件市场、插件 UI 未接入）。
- 不支持 Gemini 协议；`_pure/config/set` 对非 openai/anthropic 的 `kind` 明确报错。
- 不提供自动更新、安装器、代码签名。
- 旧的进程内自研实现已删除，不再维护。

## 3. 进程与传输

```
pure-agent.exe (pure-desktop)
  │  stdin/stdout: newline-delimited JSON-RPC
  ▼
pure-backend.exe rpc (pure-acp)
  │  pure RPC（stdin/stdout，每个会话一个子进程，仍是同一个 exe）
  ▼
pure-backend.exe --mode rpc (内核宿主)
  ├─ extensions/pure_goal.dll
  ├─ extensions/pure_todo.dll
  ├─ extensions/pure_subagents.dll
  ├─ extensions/pure_codemode.dll
  └─ extensions/pure_permission_gate.dll
```

- 桌面以 `<engine> rpc` 启动后端；同一 exe 不带 `rpc`（如 `--mode rpc`）时进入内核
  CLI 角色。`pure-backend` 把 `RPI_BIN` 指向自身，pure-acp 因此拉起同文件子进程。
- 后端退出时宿主子进程随连接关闭；桌面关闭窗口后全部退出。
- 会话历史由 pure-acp 写入 `~/.pure-agent/acp-sessions`。

## 4. 首启与目录

`pure-backend` 在 `PURE_HOME`（默认 `~/.pure-agent`）下创建：

```
agent\settings.json   默认思考等级、skills 目录（仅在不存在时写入）
agent\models.json     {"providers":{}}（仅在不存在时写入）
agent\skills\         从 dist\skills 递归复制（仅在不存在时）
acp-sessions\
logs\backend.log      每次启动追加 home/exe/宿主 路径与退出码
```

环境变量：`PURE_HOME`、`RPI_BIN`、`RPI_EXTENSIONS_DIR`、`RPI_CODING_AGENT_DIR`、
`RPI_ACP_SESSION_DIR`、`RPI_ACP_NO_AUTO_REGISTER_ZED`。

## 5. ACP 能力

| ACP 方法 | 行为 |
|----------|------|
| `initialize` | 返回 pure-acp 能力（工具程序、权限、使用量等） |
| `session/new` | 启动 内核子进程；返回 `model` / `thinking` 选项；兼容层下追加 `mode` / `goal`；随后发送 `available_commands_update` |
| `session/load` / `resume` | 恢复历史；配置与命令同上 |
| `session/prompt` | 流式 `agent_message_chunk`、`agent_thought_chunk`、`tool_call` / `tool_call_update`、`plan`；以 `stopReason` 结束 |
| `session/cancel` | 中止当前 run，返回 `cancelled` |
| `session/set_config_option` | `model`（`provider/model`）、`thinking`；兼容层下 `goal`（文本）、`mode`（default/plan/full，full 持久化到 `permissions.mode`） |
| 权限 | `pure-permission-gate` 对 bash/powershell/write/edit 按 `permissions.json`（deny > allow > default）询问或放行，并执行 `confine_paths`；确认经 `ui_dialog_request` → ACP `session/request_permission` |
| 命令 | `/compact` + 内核 `get_commands` 的扩展命令 + `SKILL.md` 派生的 `skill:<name>` |

`mode=plan` 依赖已移除的 `pure-plan-mode` 扩展，当前选项保留但静默降级。
`goal` 文本保存在会话内，并在每轮 prompt 前以固定前缀注入；`/goal clear` 写入空串。
`plan_mode_complete`（若扩展提供该工具）与 `todo` 工具结果会翻译为 ACP `plan` 更新；
`turn_start`/`turn_end` 翻译为 `_pure/turn` 更新。

## 6. `_pure/*` 兼容

| 方法 | 结果 |
|------|------|
| `_pure/config/get` | 读取 `models.json`：全部已配置 provider 放在 `providers.custom`（内核无内置模型目录，避免 registered override 丢失模型），另有 `providerKeys`；其余面板返回空集合 |
| `_pure/config/set` | 写 `providers.custom.<id>`（kind/base_url/models…），原子替换 `models.json` |
| `_pure/config/test` | `reqwest` GET `<baseUrl>/models`，Bearer 或 `x-api-key`，返回状态/耗时/模型列表 |
| `_pure/auth/set` | 写 `auth.json` 的 `{provider:{type:"api_key",key}}` |
| `_pure/models/list` | 优先内核会话可用模型，其次 `models.json`；id 为 `provider/model` |
| `_pure/providers/directory` | 代理上游 provider 目录请求 |
| `_pure/folders` / `_pure/fs` | 工作区目录浏览、读/写/列文件（读上限 512 KiB） |
| `_pure/themes` | 空集合（桌面降级到内置外观） |
| `_pure/options/list|set` | 思考等级、压缩保留量、provider 重试、`subagents.maxRunning/maxTurns/timeoutSeconds`；读写 `agent/settings.json` |
| `_pure/session/compact` | 以 `/compact` 触发内核原生压缩 |
| `_pure/session/fork` | 原生 JSONL 复制到新会话；支持 `before` 提示点与 `at` 条目；`session/list` 回读父会话 |
| `_pure/session/export` | 返回活动分支的 Markdown 记录 |
| `_pure/session/rename` | 写 `acp-sessions/titles.json`，并广播 `session_info_update` |
| `_pure/image/save` | 按 sha256 在会话分支里找图片/音频字节，写入会话目录 `generated/`，以 `_pure/notice` 回报 |
| `_pure/mcp_status` | 会话启动后逐服务器回报 `connected` / `failed`；配置来自 `agent/mcp.json` |
| `session/update: _pure/turn` | `{phase:"start"}` / `{phase:"end",reason:"tool_calls"|"no_tool_calls"}` |
| `session/update: _pure/compaction` | `compaction_progress` 事件翻译为 `started` / `complete` |
| `permissions.mode` | `agent/permissions.json` 的 `default` / `full`；full 时宿主带 `--bypass`，ACP 自动批准运行中会话的权限请求 |
| `plugins.<name>.enabled` | 写 `agent/plugins.json`；`PURE_DISABLED_EXTENSIONS` 传给宿主，加载器跳过被关闭的 DLL |
| 桌面 i18n | `crate::i18n` + `desktop.json.language`；System/English/简体中文，英语回退 |

兼容增强由 `pure_acp::pure_compat::enable()` 打开，只由 `pure-backend` 调用；关闭时
pure-acp 保持上游原始线格式，因此上游 223 个单测全部通过。

## 7. 构建与分发

`scripts/build.ps1` 依次构建两个工作区（release 默认，`-Configuration debug` 可选；
内核库随 `pure-backend` 编译），然后清空并汇总 `dist/`：

```
dist\pure-agent.exe
dist\pure-backend.exe
dist\extensions\{pure_goal,pure_todo,pure_subagents,pure_codemode,pure_permission_gate}.dll
dist\skills\{pure-guide,demo}\SKILL.md
dist\README.md  dist\THIRD_PARTY_NOTICES.md
```

脚本对必需产物做存在性检查，缺失即失败。

## 8. 验收映射（F1–F7）

| # | 要求 | 实现 | 验证 |
|---|------|------|------|
| F1 | 模型配置增删改、Key、Test、重启保留 | `_pure/config/get|set|test`、`_pure/auth/set` + `models.json`/`auth.json` | 探针：config/set → test(ok) → 重启 → config/get 仍含 `lab2`，`auth.json` 保留 key；模型 `max_output`/`reasoning`/`efforts`/`input`/`cost` 写入内核的 `maxTokens`/`thinkingLevelMap`/`cost.cacheRead` 并能读回 |
| F2 | 流式聊天、工具卡、思考、取消、历史 | pure-acp `session/prompt`、`session/load` | 探针：mock SSE 流式文本 + `end_turn`；桌面实测取消与历史 |
| F3 | Subagent | `pure-subagents` `delegate_task` | 探针：`delegate_task` 工具卡 + 子会话返回；桌面工具卡 |
| F4 | Todolist | `pure-todo` `todo` 工具 → ACP `plan` | 探针：`todo` 工具卡 + `plan` 更新 |
| F5 | `/goal` | 会话 `goal` 选项，每轮注入；`/goal clear` 置空 | 探针：set_config_option 回读 `finish pure` |
| F6 | Skills | 扫描 `agent\skills` 与 `.pure-agent\skills` 的 `SKILL.md` | 探针：命令表出现 `skill:demo` |
| F7 | 扩展命令 | `pure-goal` / `pure-todo` / `pure-subagents` / `pure-codemode` | 探针：命令表含 `goal`、`todos`；`codemode` 工具出现在工具表 |

补充验收：`_pure/turn` 起止、`_pure/session/compact`、`_pure/fs`、`_pure/folders`、
`_pure/themes`、`_pure/options/*`、插件开关、完全访问与子代理 options 均在探针的 63 项检查内。

## 9. 测试

```powershell
cargo test -j 4 --workspace -- --test-threads=1
cargo test -p pure-acp
python scripts/acp_probe.py
```

- pure-acp：227 个单测，包含兼容层映射（模型元数据/密钥来源/目录解析）与 `turn_events_emit_pure_turn_updates`。
- 探针：真实启动 `pure-backend rpc`，本地 mock OpenAI 兼容服务，覆盖 63 项；退出码
  非 0 表示存在 FAIL。
- 桌面并发渲染探针（`parity_rig` 等）在内存紧张的机器上需 `--test-threads=1`；
  全量桌面套件 657 项通过。

## 10. 已知差异

- goal 使用桌面的会话文本语义，注入每一轮并写入 `goals.json` 以便重启回读；
  不启用 `pure-goal` 的预算/暂停状态机；`pure-goal` 插件仍被加载，模型仍可主动
  调用其工具。
- `/plan` 与 `mode=plan` 依赖已移除的 `pure-plan-mode` 扩展；选项仍存在但静默降级。
- `codemode` 要求宿主 ToolProgram action 30 / protocol v1；脚本在 QuickJS 中执行，可组合调用本次调用可见的工具。
- `_pure/themes` 为空，设置页外观项由桌面本地默认值承担。引擎选项覆盖思考等级、
  压缩保留量、provider 重试与 `subagents.maxRunning/maxTurns/timeoutSeconds`；
  内核没有 swarm 运行时，Swarm 分组不提供选项，页面按缺失分组整组隐藏。
- 未实现 `_pure/session/undo`/rewind（桌面 rewind 走 fork）与 `_pure/swarm/resume`
  （内核子代理是一次性内存子会话，完成后即释放；桌面不展示 swarm 工具卡）。
