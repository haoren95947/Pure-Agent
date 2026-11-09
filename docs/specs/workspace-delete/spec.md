# Delete a workspace's conversations

## 背景与目标

左侧工作区/会话分组（例如 `D:\test`、`D:\Work\Code\Rust\kiki-agent`）只有展开/折叠，
没有删除入口，用户无法清理某个工作区下的会话。

目标：每个工作区分组行提供悬停可见的删除按钮，确认后删除该分组下的全部会话，
分组随之为空并从列表消失。

## 非目标

- 不删除工作区目录本身，也不改动磁盘上的项目文件。
- 不提供单会话批量勾选或回收站/撤销。
- 不删除其它工作区、已归档会话或正在运行的会话之外的状态。

## 用户可见行为

- 分组行右侧（“新建会话”按钮左侧）保留一条 22px 的垃圾桶按钮位；按钮常驻，
  悬停在按钮上时用危险色，提示文案「删除该工作区的全部会话」。
- 点击后弹出确认框：`Delete N conversations in <dir>? Their transcripts are removed
  from disk, and this cannot be undone.`，取消 / 删除两个按钮，危险色。
- 确认后逐个发送 `session/delete`；成功返回后会话从列表与目录缓存移除；若当前会话
  在被删集合内则回到欢迎页；右上角提示「已删除 N 个会话（<dir>）」。
- 分组内会话全部删除后，该工作区分组不再出现。
- 中文界面下按钮与文案走 `zh_cn` 词条。

## 接口/数据变更

- 桌面 `SidebarEvent::DeleteWorkspace { dir, sessions }`。
- `Dialogs` 新增 `DialogKind::DeleteWorkspace { dir, sessions }` 与对应
  `DialogEvent::DeleteWorkspace`；`DialogKind` 因携带 `Vec<String>` 由 `Copy` 改为
  `Clone`，`open()` 返回克隆（对话框每帧重绘需要值语义）。
- `pure-client`：`Client::delete_session` 发送标准 `session/delete`（`SessionRequest`），
  新增 `Pending::Delete { session_id }`；成功应答移除 `state.sessions` 与
  `state.directory` 中对应项并发出 `Change::Session`。
- `Store::delete_sessions(&[String]) -> usize`：必要时回到欢迎页、调用客户端、清理计时。
- 后端沿用已有 `session/delete`（ACP 标准方法，幂等），无协议变更。

## 影响面

`pure-desktop-app`（sidebar/dialog/launch/i18n/store）与 `pure-client`；后端与扩展不变。

## 验收

```powershell
cargo test -j 1 -p pure-desktop-app -p pure-client -- --test-threads=1
```

桌面实测：悬停分组行出现删除按钮 → 确认框计数与目录正确 → 删除后分组与会话消失，
活动会话被删时回到欢迎页。
