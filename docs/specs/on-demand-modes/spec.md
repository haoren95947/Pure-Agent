# On-demand goal / plan / swarm features

## 背景与目标

图3 的模式菜单里「目标 / 计划 / 集群」都不生效：

- 目标（`/goal`）在桌面可用，但 `pure-goal` 的 `goal` 工具常驻模型上下文。
- 计划（`mode=plan`）依赖的 `pure-plan-mode` 扩展此前被移除，选项静默降级。
- 集群（`swarm`）没有内核侧实现：没有扩展，也没有 `swarm` 配置选项。

用户要求：这三个是**功能**而不是**工具**——不得作为工具常驻系统上下文，只有
用户通过 slash 命令或图3 菜单点名时才加载。

目标：

1. `pure-goal` 保持工具注册（重载/命令测试依赖），但在 `SessionStart` 时从活动工具
   集合中移除 `goal`；`/goal` 命令仍工作，目标文本仍每轮注入。
2. 新增 `crates/pure-extensions-packages/pure-plan-mode`：
   - `/plan on|start|off|exit|status` 命令。
   - 打开时把 `exit_plan` 加入活动工具集；完成后移除。
   - `SessionStart` 时隐藏 `exit_plan`，未点名时绝不进入上下文。
   - `exit_plan` 在 `ToolCall` 事件里经宿主 `UiDialog`（`purpose=permission`，
     `question=exit_plan`）让用户批准 / 修改 / 拒绝。评审必须发生在事件处理器里：
     工具 worker 线程的 `UiDialog` 没有调用上下文，宿主会立即回 `cancelled`。
     批准确认后退出计划模式，修改保持开启；工具卸载推迟到下一个 `TurnEnd`
     （工具执行上下文不能替换活动工具集）。
   - 结果 `details:{kind:"plan",plan,outline,active,markdown}` 供引擎翻译为 ACP `plan`。
3. 新增 `crates/pure-extensions-packages/pure-swarm`：
   - `/swarm on|off|status|<task>` 命令；打开时加载 `swarm` 工具并追加集群提示，
     关闭时移除工具并追加复原提示。
   - `swarm` 工具接收 `description`、`prompt_template`（含 `{{item}}`）、2–8 个
     `items`，为每个 item 启动一个宿主托管子代理（`ChildAgent` action 24），
     轮询到全部结束，聚合 `completed/failed/cancelled` 与逐项报告。
   - `SessionStart` 时隐藏 `swarm` 工具。
4. ACP/引擎接线：
   - `session/new`、`_pure/config/get`、`session/set_config_option` 增加 `swarm`
     选项（`on|off`），与 `goal`、`mode` 并列。
   - `session/set_config_option` 的 `mode=plan` → `/plan start`，其余 → `/plan off`；
     `swarm` → `/swarm on|off`。命令参数走 `execute_extension_command_text`
     （与用户键入 `/name args` 同一形状），不能把参数再包一层 JSON。
   - 宿主 `GetActiveTools` 在空列表（“全部工具”哨兵）时返回实际工具集，
     插件才能只摘掉自己的工具。
   - 计划模式开启时 prompt 前注入只读规划指引，要求模型调用 `exit_plan`；
     `tool_execution_end` 的 `exit_plan`/`plan_mode_complete` 结果翻译为 ACP `plan`
     更新；`details.active == false` 时引擎把 `mode` 回落到 `default`。
   - 完全访问（`permissions.mode=full`）下权限对话框自动放行，但 `exit_plan`
     规划评审**必须**询问用户，不自动批准。
   - 权限应答把 `_meta.pure.planReview.revision` 作为 `answer.text` 交给插件，
     供「修改」分支读取反馈。
   - 子代理事件新增 `toolCallId` 与 `swarm{id,item,index,total}`，桌面据此把
     子代理归入集群卡片。
5. 构建：`scripts/build.ps1` 构建并打包 `pure_plan_mode.dll`、`pure_swarm.dll`，
   `pure_compat` 把它们列入内置插件（显示名 `plan-mode`、`swarm`）。

## 非目标

- goal 不启用 `pure-goal` 的预算/暂停状态机。
- swarm 不提供跨会话恢复（`_pure/swarm/resume` 仍不实现）。
- 不让模型在未开启功能时看到 `goal`/`exit_plan`/`swarm` 任一工具 schema。
- 不新增并行实现：plan/swarm 均以 cdylib 扩展提供。

## 用户可见行为

- 图3 菜单切换「计划」：进入只读规划；模型调查后弹出计划评审卡；批准后恢复正常工作，
  修改则保持计划模式并要求重写，拒绝则退出。
- 菜单切换「集群」：`swarm` 工具对模型可见，并收到集群提示；关闭后工具立即消失。
- `/plan`、`/swarm` 斜杠命令与菜单效果一致。
- 未开启「计划/集群」时，工具表与系统提示不含 `exit_plan`、`swarm`、`goal`。
- `/goal` 行为不变；目标文本仍注入每轮 prompt。

## 接口/数据变更

- 新 crate：`pure-plan-mode`、`pure-swarm`（workspace members）。
- `pure-acp`：`Session.kage_swarm`、`set_pure_swarm`、`augment_pure_config` 增加
  `swarm`；`pure_compat::swarm_option/note_swarm` 与插件清单更新；
  `translate_plan_event` 接受 `exit_plan`；`permission_answer` 透传 revision；
  `handle_ui_dialog` 对 `exit_plan` 跳过 full-access 自动批准。
- `pure-cli`：`child_agents::Job` 携带 `tool_call_id` 与 `swarm` 并写入
  `subagent_update`；`get_active_tools` 展开空哨兵。
- `pure-desktop-app`：计划评审选项按标签关键字匹配（`Reject and exit` 命中
  `reject`），使计划卡片的拒绝按钮可用。
- ACP 线协议只有兼容层范围内的新增选项与 `plan` 更新，关闭兼容层时保持上游原样。

## 影响面

`pure-extensions-packages`（两个新扩展、`pure-goal`）、`pure-acp`、`pure-cli`、
`pure-backend`（打包）、`scripts/build.ps1`、README 与 `docs/SPEC.md`。

## 验收

```powershell
cargo test -j 1 -p pure-plan-mode -p pure-swarm -p pure-goal -p pure-acp -p pure-cli -- --test-threads=1
cargo build -j 1 -p pure-backend -p pure-goal -p pure-todo -p pure-subagents -p pure-codemode -p pure-permission-gate -p pure-rtk -p pure-plan-mode -p pure-swarm
python scripts/acp_probe.py
powershell -ExecutionPolicy Bypass -File scripts/build.ps1
```

桌面实测：图3 分别切换目标/计划/集群；确认未开启时工具表不含对应工具。
