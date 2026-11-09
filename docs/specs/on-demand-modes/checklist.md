# on-demand-modes — checklist

逐条实测，命令以仓库根为工作目录。

1. [x] `cargo test -j 1 -p pure-plan-mode -p pure-swarm -p pure-goal -- --test-threads=1`
2. [x] `cargo test -j 1 -p pure-acp -p pure-cli -- --test-threads=1`
3. [x] 构建探针前置产物：
       `cargo build -j 1 -p pure-backend -p pure-goal -p pure-todo -p pure-subagents`
       `-p pure-codemode -p pure-permission-gate -p pure-rtk -p pure-plan-mode -p pure-swarm`
4. [x] `python scripts/acp_probe.py` 退出码 0（74 项 PASS）。
5. [x] `powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Configuration debug -SkipBuild`
       退出码 0；`dist\extensions\pure_plan_mode.dll`、`dist\extensions\pure_swarm.dll`、
       `dist\skills\pure-guide\SKILL.md` 与 `dist\skills\demo\SKILL.md` 均存在。
6. [ ] 桌面（交互会话、有人值守）：图3 点「计划」→ 输入任务 → 出现计划评审卡；
       批准后模式回到默认，工具表不含 `exit_plan`。
7. [ ] 桌面：计划模式下点「修改」，反馈文本送达插件且计划模式保持开启。
8. [ ] 桌面：图3 点「集群」→ 模型可见 `swarm`；关闭后工具表立即不含 `swarm`。
9. [ ] 桌面：未开启计划/集群时，模型工具表不含 `goal`、`exit_plan`、`swarm`。
10. [ ] 完全访问模式（Permissions = Full access）下计划评审仍弹出询问。
