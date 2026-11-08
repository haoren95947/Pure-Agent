# Pure Agent

纯 Rust 桌面 Agent：**桌面壳使用源自 kage-desktop 的 egui 客户端，Agent 内核使用源自 rpi-acp + rpi 的实现**。

```
┌─ dist\pure-agent.exe ─────────────────────────────────────┐
│ pure-desktop-app（crates/pure-desktop/app，产品化定制）      │
│ 会话 / 聊天 / 工具卡 / 计划面板 / 设置 / 模型选择器          │
└──────────────────────┬────────────────────────────────────┘
                       │ ACP v1 over stdio（JSON-RPC）
┌──────────────────────▼────────────────────────────────────┐
│ dist\pure-backend.exe  rpc                                 │
│ pure-acp：ACP 会话/流式/工具/权限/配置 + `_pure/*` 扩展      │
└──────────────────────┬────────────────────────────────────┘
                       │ pure RPC（每会话一个子进程，仍是同一个 exe）
┌──────────────────────▼────────────────────────────────────┐
│ dist\pure-backend.exe --mode rpc + extensions\*.dll        │
│ 内核宿主：模型/工具/skills；pure-goal · pure-todo ·         │
│ pure-subagents · pure-codemode · pure-permission-gate      │
│ · pure-rtk                                                 │
└───────────────────────────────────────────────────────────┘
```

`pure-backend.exe` 是双角色单文件：桌面以 `rpc` 启动它跑 ACP 服务；pure-acp 需要
宿主时，把同一个 exe 以 `--mode rpc` 拉起为每会话子进程（`RPI_BIN` 默认指向自身）。

- 桌面不是重写：`crates/pure-desktop/app` 的 `pure-desktop-app` 只做产品化定制（引擎文件名、
  窗口标题、保存后关闭表单、会话菜单、Plugins 页、品牌）。
- ACP 内核不是重写：`crates/pure-engine/pure-acp` 是 rpi 的 ACP 适配器，
  由 `pure-backend` 仅做环境初始化、目录创建、默认配置播种后调用 `pure_acp::protocol::run`。
- goal/todo/subagents/codemode/permission-gate/rtk/skills 全部来自内核扩展与原生 skills，没有并行实现。

## 构建

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
# 调试构建：
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Configuration debug
```

产物在 `dist\`：

```
pure-agent.exe
pure-backend.exe
extensions\pure_goal.dll
extensions\pure_todo.dll
extensions\pure_subagents.dll
extensions\pure_codemode.dll
extensions\pure_permission_gate.dll
extensions\pure_rtk.dll
extensions\vendor\rtk\{win32-x64,darwin-x64,darwin-arm64}\rtk[.exe]
skills\pure-guide\SKILL.md
skills\demo\SKILL.md
README.md
THIRD_PARTY_NOTICES.md
```

直接运行 `dist\pure-agent.exe`。构建只涉及一个 workspace（内核库作为库被链接进
`pure-backend`，不单独产出 CLI）：

```powershell
cargo build --release -p pure-desktop-app -p pure-backend -p pure-goal -p pure-todo `
  -p pure-subagents -p pure-codemode -p pure-permission-gate -p pure-rtk
```

## 数据目录

所有状态统一放在 `~/.pure-agent`（首次启动自动创建）：

```
~/.pure-agent\
  desktop.json             # 桌面偏好
  agent\settings.json      # 默认思考等级、skills 目录
  agent\models.json        # provider / model 配置
  agent\auth.json          # API key（明文，设置页会提示）
  agent\skills\            # 从 dist\skills 播种
  acp-sessions\            # ACP 会话 JSONL
  logs\backend.log         # 后端日志
  logos\                   # provider 图标缓存
```

`PURE_HOME` 可覆盖根目录；`RPI_BIN` 可指定外部宿主做调试（默认是
`pure-backend.exe` 自身），`RPI_EXTENSIONS_DIR` 可覆盖扩展路径。
模型配置直接使用内核的 `models.json` / `auth.json`，可在设置页新增 provider、
保存 key、Test 连通性，重启后由 `_pure/config/get` 原样回读。

## 桌面功能与内核对应

| 功能 | 桌面入口 | 内核实现 |
|------|----------|----------|
| 模型选择 | 会话配置 `model` 选项（`provider/model`） | `session/set_config_option` → 内核 `set_model` |
| 思考等级 | `thinking` 选项 | 内核 `set_thinking_level` |
| `/goal` | goal 胶囊 + `goal` 文本选项 | 会话级 goal 注入每轮 prompt；`pure-goal` 工具仍可用 |
| `/compact` | 斜杠菜单 / `_pure/session/compact` | 内核原生 compact |
| Todo | 计划/待办面板 | `pure-todo` 的 `todo` 工具结果 → ACP `plan` 更新 |
| Subagent | 子代理工具卡 | `pure-subagents` 的 `delegate_task` |
| codemode | 工具卡 | `pure-codemode` 的 `codemode` 工具：一轮内用 JS 组合调用现有工具 |
| Skills | `/skill:<name>` | `~/.pure-agent/agent/skills`、`.pure-agent/skills` 下的 `SKILL.md` |
| 设置 | 设置页 | `_pure/config/get|set|test`、`_pure/auth/set`、`_pure/models/list` |
| 文件面板 | 工作区浏览器 | `_pure/folders`、`_pure/fs` |
| 会话操作 | 侧栏行菜单 | 重命名、置顶/归档（本地）、`_pure/session/fork|export|compact` |
| 媒体保存 | 图片/音频卡片 | `_pure/image/save`（按 sha256 从会话历史取字节，落 `generated/`） |
| MCP | 设置页 + 状态点 | `agent/mcp.json` 注入每个新会话，`_pure/mcp_status` 逐服务器回报 |
| 权限规则 | 设置页工具规则 | `agent/permissions.json` 由 `pure-permission-gate` 执行（deny > allow > default；`confine_paths` 限制文件工具） |
| 访问模式 | Permissions 页 Default / Full access | `permissions.mode`：Default 走规则与确认；Full access 关闭权限门并对运行中会话自动批准 |
| 扩展开关 | Plugins 页逐项开关 | `agent/plugins.json` 的 `enabled`；被关闭的 DLL 不再由新会话加载 |
| 界面语言 | 跟随系统（General 页不再提供切换） | 桌面本地 i18n（`crate::i18n`），英语为回退 |
| 引擎选项 | Agent & Sessions 页 | `_pure/options/list|set` → `agent/settings.json` |

## 兼容范围与差异

- `_pure/turn` 作为 ACP `session/update` 发送（`phase: start|end`），由内核的
  `turn_start` / `turn_end` 事件翻译而来；compact 进度翻译为 `_pure/compaction`。
- provider `kind` 仅支持 `openai`（→ `openai-completions`）与 `anthropic`
  （→ `anthropic-messages`）；Gemini 等其它协议在 `_pure/config/set` 明确拒绝。
- `_pure/session/fork` 基于原生 JSONL 复制（支持 `before` 提示点与 `at` 条目），
  `_pure/session/export` 输出 Markdown；`session/list` 回读重命名标题与 fork 父会话。
- `_pure/themes` 仍返回空集合，桌面降级为内置外观；`_pure/options/list|set`
  提供思考等级、压缩保留量、provider 重试与子代理并发/轮次/超时共六项，写入内核 `settings.json`。
  Agent & Sessions 页只渲染引擎真实提供的分组，缺失分组（如内核没有的 Swarm）整组隐藏。
- MCP 配置来自设置页写入的 `agent/mcp.json`；客户端显式传入非空 `mcpServers`
  时以客户端为准。任一服务器启动失败不会阻塞会话，状态回报为 `failed`。
- 权限规则由 `pure-permission-gate` 在每次工具调用时读取；`confine_paths`
  限制文件工具在工作区内。未命中规则时 bash/powershell/write/edit 仍按内置策略询问。
- 访问模式写入 `agent/permissions.json` 的 `mode`：`default` 走上述规则；
  `full` 让新会话带 `--bypass` 启动，且 ACP 层对运行中会话的权限对话框直接
  选择 allow 选项，不再打扰用户。
  会话内的模式菜单与 Permissions 页都会列出 Default / Full access。
- 插件开关写入 `agent/plugins.json`（`{name: {enabled: false}}`）；宿主启动时
  以 `PURE_DISABLED_EXTENSIONS` 传入被关闭的 DLL 名，加载器直接跳过，因此
  关闭的扩展不注册工具、命令或对话框。对已启动的会话不热卸载，下一次新建
  会话生效（设置页有说明）。
- 界面语言是桌面端本地偏好（`desktop.json` 的 `language`），默认跟随系统；
  英语为源语言，中文缺失条目自动回退英文。General 页已去掉语言切换控件，
  仅原生可执行文件跟随 Windows 界面语言，测试与工具默认英文。
- goal 语义：会话级文本目标，注入每一轮，并写入 `goals.json` 以便重启后回读；
  不启用 `pure-goal` 的预算/暂停状态机。
- `/plan` 与 `mode=plan` 依赖已移除的 `pure-plan-mode` 扩展；选项仍存在，但不会进入只读规划。
- `codemode` 依赖内核 ToolProgram action 30 / protocol v1 与 `x-pure-execution-mode`、
  `x-pure-result-envelope` 注册关键字；代码在 QuickJS 中运行，可调用本次调用可见的工具。
- 兼容增强通过 `pure_acp::pure_compat::enable()` 打开（仅 `pure-backend` 调用）；
  关闭时 `pure-acp` 保持上游原始线格式。

## 测试与验证

```powershell
# 1) 根 workspace 全量单测（桌面壳 + 内核库；GPU 探针需单线程避免渲染竞争）
cargo test -j 4 --workspace -- --test-threads=1

# 2) ACP 适配器单测（含兼容关闭时保持原行为）
cargo test -p pure-acp

# 3) 端到端 ACP 探针：本地 mock 模型，覆盖聊天/工具/计划/子代理/compact/配置/重启持久化/插件开关/完全访问
python scripts/acp_probe.py
```

探针最后输出 PASS/FAIL 汇总并以退出码表示结果。

## 目录

```
crates/pure-desktop/       桌面模块：app（egui 壳，包 pure-desktop-app）+
                           pure-client/pure-core/pure-acp-wire 客户端栈
                           定制：引擎名/标题/产物名、保存完成关闭表单、侧栏行菜单、
                           Plugins 页展示内核扩展、移除 Vim 模式与能力提示条
crates/pure-engine/        CLI 内核（pure-cli 作为库链接进 pure-backend）+
                           ACP 栈（pure-acp/pure-extcore/pure-mcp-core 等）
  pure-backend             双角色入口：`rpc` 跑 ACP，`--mode rpc` 跑内核宿主
  pure-acp                 ACP v1 服务、`_pure/*` 兼容
crates/pure-tui/           终端 UI 渲染库（单 crate）
crates/pure-extensions-packages/
                           goal/todo/subagents/codemode/permission-gate/rtk 扩展
  examples/                plugin-stub、extension-hello 示例插件
skills/                    随 dist 分发的技能
scripts/build.ps1          一键构建 + 汇总 dist
scripts/acp_probe.py       ACP 端到端验证脚本
docs/SPEC.md               架构与验收规格
```

## 许可

MIT。第三方声明见 `THIRD_PARTY_NOTICES.md`（kage MIT、pi-rust MIT、字体 OFL、Lucide ISC）。
