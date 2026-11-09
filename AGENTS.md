# AGENTS.md

给在本仓库工作的编码代理的说明。先读本文件，再读 [README.md](README.md) 与 [docs/SPEC.md](docs/SPEC.md)（规格与验收映射的唯一权威）。

## 项目概览

Pure Agent 是纯 Rust 桌面 Agent：egui 桌面壳通过 stdio ACP v1（JSON-RPC）连接 `pure-backend.exe`，后者既是 ACP 服务（`rpc`）又是每会话的内核宿主（`--mode rpc`）。内核来自 pi-rust，功能一律由内核扩展与原生 skills 提供，不允许并行实现。

- 平台：Windows 优先（eframe/glow）。
- 工作区：单一根 workspace；内核库链接进 `pure-backend`，不单独产出 CLI。
- 发行物：`scripts/build.ps1` 汇总到 `dist/`（不入库）。

## 目录结构

```
crates/pure-desktop/              桌面模块
  app/                            pure-desktop-app → dist\pure-agent.exe（egui 壳）
  pure-client/ pure-core/         sans-IO 客户端栈
  pure-acp-wire/                  ACP 线协议类型
crates/pure-engine/               内核 + ACP
  pure-acp/                       ACP v1 服务、_pure/* 兼容层
  pure-backend/                   双角色入口：`rpc` 与 `--mode rpc`
  pure-cli/ pure-agent/ pure-ai/ pure-tools/ pure-runtime/
  pure-harness/ pure-durable/ pure-plugin-sdk/ pure-extensions/ ...
crates/pure-tui/                  终端 UI 渲染库
crates/pure-extensions-packages/  CDylib 扩展
  pure-goal/ pure-todo/ pure-subagents/ pure-codemode/
  pure-permission-gate/ pure-rtk/  （pure-rtk 内含三平台预编译 rtk 二进制）
  examples/                       plugin-stub、extension-hello
skills/                           随 dist 分发的 SKILL.md
scripts/build.ps1                 构建 + 汇总 dist（含必需产物校验）
scripts/acp_probe.py              端到端 ACP 探针（mock 模型）
docs/SPEC.md                      全局规格、兼容范围、验收映射
docs/specs/<change-id>/           每个新功能一份 spec（spec.md/tasks.md/checklist.md）
vendor/gpu-allocator/             crates.io patch，勿删
```

## 工具链与运行

- `rust-toolchain.toml` 固定 **1.95.0**（含 rustfmt/clippy）。`pure-desktop-app` 与 `pure-client` 要求 1.95 + edition 2024；其余 workspace crate 为 edition 2021。
- `.cargo/config.toml` 的 `RUST_FONTCONFIG_DLOPEN=1` 是桌面字体栈的运行前提，勿移除。
- 运行时状态在 `~/.pure-agent`（`PURE_HOME` 可覆盖）。测试与探针请指向临时 `PURE_HOME`，不要污染真实数据。
- 内存敏感：构建与测试统一用 `-j 1`；桌面并发渲染/探针套件必须 `--test-threads=1`。

```powershell
# 构建并汇总 dist（release 默认，可加 -Configuration debug）
powershell -ExecutionPolicy Bypass -File scripts/build.ps1

# 只编译指定 crate
cargo build -j 1 -p pure-desktop-app -p pure-backend

# 运行桌面
dist\pure-agent.exe
```

## 开发流程（spec-first）

新功能或用户可见行为变更，**先写 spec，再动代码**。每个功能新建一个 `docs/specs/<change-id>/` 目录，不复用、不覆盖历史。

```
docs/specs/<change-id>/     change-id 用 kebab-case，如 acp-retry-policy
  spec.md                   必需：目标、非目标、行为、接口/数据变更、影响面、验收标准
  tasks.md                  多阶段功能必需：任务拆解与勾选进度
  checklist.md              需实测验收时必需：逐条可复跑的验收项
```

- `spec.md` 至少写清：背景与目标、非目标、用户可见行为与边界情况、接口/协议/配置/数据变更、受影响的 crate 与发布物、可直接执行的验收命令。
- 流程：建目录并写 `spec.md` → 拆 `tasks.md` → 实现并勾选 → 按 `checklist.md` 逐条实测 → 同步文档 → 提交。不要“实现完再补 spec”。
- 行为或验收项变化时，同步更新 `docs/SPEC.md`（含 F1–F7 验收映射）与 `README.md`。
- spec 是决策记录，完成后保留；被后续方案推翻时，在文首标 `SUPERSEDED` 并链接新 change-id，不删原文。
- 例外：不改变行为的修复（typo、注释、纯内部重构、依赖维护）可不写 spec，但仍须通过提交门禁。

## 提交门禁

提交前必须全部通过；任何一项失败都不提交。

1. **审阅暂存内容**：`git status` + `git diff --cached`；按明确路径 `git add <file>`，不要盲目 `git add -A`。
2. **格式**：只格式化你改动的文件。
   ```powershell
   rustfmt --edition 2021 --check <changed.rs>   # desktop app 用 --edition 2024
   ```
   全仓存在既有格式差异，`cargo fmt --all` 会制造无关 diff，不要顺手全仓格式化。
3. **Lint**：`cargo clippy -j 1 -p <crate> --all-targets`，不得新增 warning（仓库有既有基线 warning，`-D warnings` 只对本来干净的 crate 使用）。
4. **测试（按改动范围）**
   ```powershell
   # 通用：改哪个 crate 测哪个
   cargo test -j 1 -p <crate> -- --test-threads=1

   # ACP / 协议 / _pure/* 兼容层改动：单测 + 端到端探针（非 0 退出码 = FAIL）
   cargo test -j 1 -p pure-acp
   # 探针默认读取 target\debug 下的 pure-backend.exe 与扩展 DLL，先构建
   cargo build -j 1 -p pure-backend -p pure-goal -p pure-todo -p pure-subagents `
     -p pure-codemode -p pure-permission-gate -p pure-rtk
   python scripts/acp_probe.py

   # 桌面与客户端栈改动
   cargo test -j 1 -p pure-desktop-app -p pure-client -p pure-core -- --test-threads=1

   # 发布前全量
   cargo test -j 1 --workspace -- --test-threads=1
   ```
5. **打包/产物变化**：跑 `scripts/build.ps1`；脚本会校验 `dist/` 必需文件，缺失即失败。
6. **文档与 spec**：新功能必须有 `docs/specs/<change-id>/spec.md`；有 `checklist.md` 时其验收项须全部实测通过；行为、协议、`_pure/*` 兼容范围或验收项变化时，同步更新 `README.md` 与 `docs/SPEC.md`。
7. **提交信息**：英文 Conventional Commits（`feat:` / `fix:` / `refactor:` / `chore:` / `docs:` / `test:`），祈使句，一次提交只做一件事。禁止 `--no-verify`。
8. **推送**：确认远端与分支后再推；`main` 只接受快进推送，禁止 force push。推送前复跑受影响的测试。

## 禁止入库

`target/`、`dist/`、`dist-next/`、`reference/`、`.agents/`、`.pure-agent/`、`__pycache__/`、`*.tmp`，以及运行日志、一次性取证文件。它们已在 `.gitignore` 中，不要用 `-f` 强推。

有意入库的大文件只有 `crates/pure-extensions-packages/pure-rtk/vendor/rtk/{win32-x64,darwin-x64,darwin-arm64}`：运行时按 `PURE_RTK_BIN` → `vendor/rtk/<platform>/` 查找，勿删除、勿压缩、勿改 LFS。

`reference/` 是上游对照基线，只读：不修改、不提交、不在其上做实现。

## 工程约束

- **依赖钉住**：`rpi-plugin-sdk = "=0.3.20"`（ABI-4，对齐 pure 0.21.0 宿主），不要替换成宿主内的本地 SDK；`gpu-allocator` 的 `[patch.crates-io]` 指向 `vendor/`，用于避开 windows 0.62 的 trait 冲突，不要移除。
- **双版本依赖**（thiserror 1/2、base64 0.22/0.23、sha2 0.10/0.11、similar 2/3、nix 0.29/0.31、pulldown-cmark 0.12/0.13）是刻意保留；统一版本属于独立后续工作。
- **不写并行实现**：goal/todo/subagents/codemode/permission-gate/rtk/skills 必须复用 `crates/pure-extensions-packages/*` 与内核原生 `SKILL.md`，不要另起实现。
- **ACP 兼容层**：`pure_acp::pure_compat::enable()` 只由 `pure-backend` 调用；关闭时必须保持上游原始线格式，pure-acp 上游测试要继续全绿。
- **unsafe**：workspace 声明 `unsafe_code = "forbid"`；FFI crate 不继承 lints 表，新增 `unsafe` 只能限定在该边界内，并在代码处说明原因。
- **GUI 测试**：桌面 UI 端到端会打开真实窗口，只允许在有人值守的交互桌面运行；不要放到后台或无人值守流水线。
- **Windows 优先**：路径、进程拉起、字体与 DLL 加载改动需在 Windows 上实测；跨平台结论不要只凭编译推断。

## 已知现状

- 仓库暂无 CI 配置，上述门禁由本地命令人工执行。
- `cargo fmt --all --check` 当前不通过（既有差异），不属于业务改动的修复范围；只校验你改动的文件。
- `skills/` 提供 `pure-guide` 与 `demo` 两个技能种子，`scripts/build.ps1` 会逐个校验其 `SKILL.md`。
- `.agents/document/spec/` 里的历史 spec 是本地草稿（不入库）；新功能 spec 一律放 `docs/specs/`，随代码提交。
