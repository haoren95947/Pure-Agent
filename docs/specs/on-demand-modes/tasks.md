# on-demand-modes — tasks

- [x] `pure-goal`：`SessionStart` 隐藏 `goal` 工具（保留注册与 `/goal` 命令），
      重载夹具接受第二个事件标签。
- [x] 新建 `pure-plan-mode`：`/plan` 命令、`exit_plan` 工具、评审批处理、
      `SessionStart` 隐藏工具、单测。
- [x] 新建 `pure-swarm`：`/swarm` 命令、`swarm` 工具、ChildAgent 批处理与聚合、
      `SessionStart` 隐藏工具、单测。
- [x] workspace 注册两个新 crate。
- [x] `pure-acp`：`swarm` 配置选项、`set_pure_swarm`、`pure_compat` 选项与插件清单、
      plan 事件翻译与模式回落、revision 透传、full-access 下计划评审仍询问。
- [x] `pure-cli`：子代理事件的 `toolCallId`/`swarm` 字段。
- [x] `scripts/build.ps1`：构建、拷贝与产物校验加入两个 DLL。
- [x] 全部相关 crate 的单测与 ACP 探针通过（探针 74 项 PASS，退出码 0）。
- [x] `scripts/build.ps1` 构建、拷贝并校验两个 DLL；`skills\pure-guide\SKILL.md`
      与 `skills\demo\SKILL.md` 补齐后 `-Configuration debug -SkipBuild` 产物校验通过。
- [x] README 与 `docs/SPEC.md` 同步。
