# Agent card name/detail overlap

## 背景与目标

右侧「智能体」面板每个运行中的子代理显示一行：名字（如 `worker`）与详情/任务摘要
同一行绘制。详情里的换行（子代理任务常是两段：`编写一个简单的 HTML 文件。\n\n任务：…`）
让整个 galley 变成多行，而行高按单行计算，绘制时又以 `(line_h - galley.y) / 2` 垂直居中，
于是第一行正文压到名字上。

目标：所有单行元信息（智能体名字、任务摘要、工具行摘要）在进入 `head::one_line` 时
折叠空白与换行，永远只绘制一行，并与名字保持原有基线。

## 非目标

- 不改变智能体面板的行高、字号、颜色或布局。
- 不改子代理任务文本本身；会话记录里仍保留原始换行。
- 不改动会话转录中的正文换行渲染。

## 用户可见行为

- 智能体卡片的名字与详情不再重叠，详情过长时按原有逻辑以省略号截断。
- 详情中的多段任务被折叠成 `段一 段二` 的单行摘要。
- 其它调用 `head::one_line` 的行（工具头、变更行）获得相同保证。

## 接口/实现变更

- `crates/pure-desktop/app/src/ui/transcript/head.rs`
  - 新增 `pub(crate) fn flat(text: &str) -> String`：`split_whitespace` 后以单空格重连。
  - `one_line` 入口先 `flat` 再排版；宽度测量与截断逻辑不变。
  - 新增单测 `flat_folds_a_multi_line_detail_into_one_row`（使用 Windows 路径
    `D:\test\page2.html`，验证反斜杠与换行都被保留/折叠）。

## 影响面

仅桌面渲染层；不涉及 ACP 线协议、内核、扩展或数据目录。

## 验收

```powershell
cargo test -j 1 -p pure-desktop-app -- --test-threads=1
```

桌面实测：让一个 `worker` 子代理执行含换行的任务，右侧智能体行名字与摘要互不覆盖。
