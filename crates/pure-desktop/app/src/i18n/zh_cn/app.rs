//! Chinese strings for `app`.

pub(super) static ENTRIES: &[(&str, &str)] = &[
    ("Copied the session as Markdown", "已将会话复制为 Markdown"),
    ("{}: {}", "{}：{}"),
    ("needs your approval", "需要你的批准"),
    ("has a plan to review", "有方案待审阅"),
    ("turn failed", "本轮失败"),
    ("turn finished", "本轮已完成"),
    (
        "Interrupt the running turn before rewinding",
        "请先中断正在运行的一轮，再执行回退",
    ),
    (
        "/{} needs a session; open one first",
        "/{} 需要先打开一个会话",
    ),
    ("engine installed; restarting it", "引擎已安装，正在重启"),
    (
        "desktop installed at {}; restart pure to apply",
        "桌面端已安装到 {}，重启 pure 后生效",
    ),
];
