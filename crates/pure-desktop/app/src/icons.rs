//! The icon catalog: the Lucide SVGs the app draws, embedded and
//! tinted by the theme.
//!
//! One enum entry per icon, the catalog of the old `IconName` list.
//! The SVGs' `currentColor` strokes are re-colored white once at first
//! use, so egui's tint multiply paints the exact theme color. The
//! files vendor from the Lucide catalog (see `assets/icons/README.md`);
//! nothing loads over the network.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use egui::Widget;

/// The white the strokes are set to, so a tint multiplies to the exact
/// theme color.
const TINT_BASE: &str = "#FFFFFF";

/// The `bytes://` URI scheme prefix every icon loads under.
const URI_PREFIX: &str = "bytes://pure-icon/";

/// One icon the app draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(clippy::enum_variant_names)]
pub enum Icon {
    /// The Lucide `a-large-small` icon.
    ALargeSmall,
    /// The Lucide `archive` icon.
    Archive,
    /// The Lucide `arrow-down` icon.
    ArrowDown,
    /// The Lucide `arrow-left` icon.
    ArrowLeft,
    /// The Lucide `arrow-right` icon.
    ArrowRight,
    /// The Lucide `arrow-up` icon.
    ArrowUp,
    /// The Lucide `asterisk` icon.
    Asterisk,
    /// The Lucide `at-sign` icon.
    AtSign,
    /// The Lucide `ban` icon.
    Ban,
    /// The Lucide `battery` icon.
    Battery,
    /// The Lucide `battery-charging` icon.
    BatteryCharging,
    /// The Lucide `battery-full` icon.
    BatteryFull,
    /// The Lucide `battery-low` icon.
    BatteryLow,
    /// The Lucide `battery-medium` icon.
    BatteryMedium,
    /// The Lucide `battery-warning` icon.
    BatteryWarning,
    /// The Lucide `bell` icon.
    Bell,
    /// The Lucide `book-open` icon.
    BookOpen,
    /// The Lucide `bot` icon.
    Bot,
    /// The Lucide `building-2` icon.
    Building2,
    /// The Lucide `calendar` icon.
    Calendar,
    /// The Lucide `case-sensitive` icon.
    CaseSensitive,
    /// The Lucide `chart-pie` icon.
    ChartPie,
    /// The Lucide `check` icon.
    Check,
    /// The Lucide `chevron-down` icon.
    ChevronDown,
    /// The Lucide `chevron-left` icon.
    ChevronLeft,
    /// The Lucide `chevron-right` icon.
    ChevronRight,
    /// The Lucide `chevron-up` icon.
    ChevronUp,
    /// The Lucide `chevrons-up-down` icon.
    ChevronsUpDown,
    /// The Lucide `circle` icon.
    Circle,
    /// The Lucide `circle-alert` icon.
    CircleAlert,
    /// The Lucide `circle-check` icon.
    CircleCheck,
    /// The Lucide `circle-pause` icon.
    CirclePause,
    /// The Lucide `circle-slash` icon.
    CircleSlash,
    /// The Lucide `circle-user` icon.
    CircleUser,
    /// The Lucide `circle-x` icon.
    CircleX,
    /// The Lucide `close` icon.
    Close,
    /// The Lucide `command` icon.
    Command,
    /// The Lucide `copy` icon.
    Copy,
    /// The Lucide `corner-down-left` icon.
    CornerDownLeft,
    /// The Lucide `corner-down-right` icon.
    CornerDownRight,
    /// The Lucide `cpu` icon.
    Cpu,
    /// The Lucide `dash` icon.
    Dash,
    /// The Lucide `delete` icon.
    Delete,
    /// The Lucide `download` icon.
    Download,
    /// The Lucide `ellipsis` icon.
    Ellipsis,
    /// The Lucide `ellipsis-vertical` icon.
    EllipsisVertical,
    /// The Lucide `external-link` icon.
    ExternalLink,
    /// The Lucide `eye` icon.
    Eye,
    /// The Lucide `eye-off` icon.
    EyeOff,
    /// The Lucide `file` icon.
    File,
    /// The Lucide `file-diff` icon.
    FileDiff,
    /// The Lucide `file-plus` icon.
    FilePlus,
    /// The Lucide `file-text` icon.
    FileText,
    /// The Lucide `folder` icon.
    Folder,
    /// The Lucide `folder-closed` icon.
    FolderClosed,
    /// The Lucide `folder-lock` icon.
    FolderLock,
    /// The Lucide `folder-open` icon.
    FolderOpen,
    /// The Lucide `folder-up` icon.
    FolderUp,
    /// The Lucide `frame` icon.
    Frame,
    /// The Lucide `gallery-vertical-end` icon.
    GalleryVerticalEnd,
    /// The Lucide `git-branch` icon.
    GitBranch,
    /// The Lucide `git-fork` icon.
    GitFork,
    /// The Lucide `github` icon.
    Github,
    /// The Lucide `globe` icon.
    Globe,
    /// The Lucide `hand` icon.
    Hand,
    /// The Lucide `hard-drive` icon.
    HardDrive,
    /// The Lucide `heart` icon.
    Heart,
    /// The Lucide `heart-off` icon.
    HeartOff,
    /// The Lucide `house` icon.
    House,
    /// The Lucide `inbox` icon.
    Inbox,
    /// The Lucide `info` icon.
    Info,
    /// The Lucide `inspector` icon.
    Inspector,
    /// The Lucide `layers` icon.
    Layers,
    /// The Lucide `layout-dashboard` icon.
    LayoutDashboard,
    /// The Lucide `lightbulb` icon.
    Lightbulb,
    /// The Lucide `list` icon.
    List,
    /// The Lucide `list-todo` icon.
    ListTodo,
    /// The Lucide `list-tree` icon.
    ListTree,
    /// The Lucide `loader` icon.
    Loader,
    /// The Lucide `loader-circle` icon.
    LoaderCircle,
    /// The Lucide `map` icon.
    Map,
    /// The Lucide `maximize` icon.
    Maximize,
    /// The Lucide `memory-stick` icon.
    MemoryStick,
    /// The Lucide `menu` icon.
    Menu,
    /// The Lucide `message-square` icon.
    MessageSquare,
    /// The Lucide `minimize` icon.
    Minimize,
    /// The Lucide `minus` icon.
    Minus,
    /// The Lucide `moon` icon.
    Moon,
    /// The Lucide `network` icon.
    Network,
    /// The Lucide `palette` icon.
    Palette,
    /// The Lucide `panel-bottom` icon.
    PanelBottom,
    /// The Lucide `panel-bottom-open` icon.
    PanelBottomOpen,
    /// The Lucide `panel-left` icon.
    PanelLeft,
    /// The Lucide `panel-left-close` icon.
    PanelLeftClose,
    /// The Lucide `panel-left-open` icon.
    PanelLeftOpen,
    /// The Lucide `panel-right` icon.
    PanelRight,
    /// The Lucide `panel-right-close` icon.
    PanelRightClose,
    /// The Lucide `panel-right-open` icon.
    PanelRightOpen,
    /// The Lucide `paperclip` icon.
    Paperclip,
    /// The Lucide `pause` icon.
    Pause,
    /// The Lucide `pen-line` icon.
    PenLine,
    /// The Lucide `pencil` icon.
    Pencil,
    /// The Lucide `pin` icon.
    Pin,
    /// The Lucide `pin-off` icon.
    PinOff,
    /// The Lucide `play` icon.
    Play,
    /// The Lucide `plus` icon.
    Plus,
    /// The Lucide `redo` icon.
    Redo,
    /// The Lucide `redo-2` icon.
    Redo2,
    /// The Lucide `refresh-cw` icon.
    RefreshCw,
    /// The Lucide `replace` icon.
    Replace,
    /// The Lucide `resize-corner` icon.
    ResizeCorner,
    /// The Lucide `rotate-cw` icon.
    RotateCw,
    /// The Lucide `search` icon.
    Search,
    /// The Lucide `server` icon.
    Server,
    /// The Lucide `settings` icon.
    Settings,
    /// The Lucide `settings-2` icon.
    Settings2,
    /// The Lucide `shield-alert` icon.
    ShieldAlert,
    /// The Lucide `shield-check` icon.
    ShieldCheck,
    /// The Lucide `shield-question-mark` icon.
    ShieldQuestionMark,
    /// The Lucide `shield-x` icon.
    ShieldX,
    /// The Lucide `slash` icon.
    Slash,
    /// The Lucide `sort-ascending` icon.
    SortAscending,
    /// The Lucide `sort-descending` icon.
    SortDescending,
    /// The Lucide `square` icon.
    Square,
    /// The Lucide `square-pen` icon.
    SquarePen,
    /// The Lucide `square-terminal` icon.
    SquareTerminal,
    /// The Lucide `star` icon.
    Star,
    /// The Lucide `star-fill` icon.
    StarFill,
    /// The Lucide `star-off` icon.
    StarOff,
    /// The Lucide `sun` icon.
    Sun,
    /// The Lucide `target` icon.
    Target,
    /// The Lucide `terminal` icon.
    Terminal,
    /// The Lucide `thumbs-down` icon.
    ThumbsDown,
    /// The Lucide `thumbs-up` icon.
    ThumbsUp,
    /// The Lucide `trash` icon.
    Trash,
    /// The Lucide `triangle-alert` icon.
    TriangleAlert,
    /// The Lucide `undo` icon.
    Undo,
    /// The Lucide `undo-2` icon.
    Undo2,
    /// The Lucide `user` icon.
    User,
    /// The Lucide `users` icon.
    Users,
    /// The Lucide `waypoints` icon.
    Waypoints,
    /// The Lucide `window-close` icon.
    WindowClose,
    /// The Lucide `window-maximize` icon.
    WindowMaximize,
    /// The Lucide `window-minimize` icon.
    WindowMinimize,
    /// The Lucide `window-restore` icon.
    WindowRestore,
    /// The Lucide `x` icon.
    X,
    /// The Lucide `zap` icon.
    Zap,
}

impl Icon {
    /// Every icon, catalog order.
    pub const ALL: &'static [Icon] = &[
        Icon::ALargeSmall,
        Icon::Archive,
        Icon::ArrowDown,
        Icon::ArrowLeft,
        Icon::ArrowRight,
        Icon::ArrowUp,
        Icon::Asterisk,
        Icon::AtSign,
        Icon::Ban,
        Icon::Battery,
        Icon::BatteryCharging,
        Icon::BatteryFull,
        Icon::BatteryLow,
        Icon::BatteryMedium,
        Icon::BatteryWarning,
        Icon::Bell,
        Icon::BookOpen,
        Icon::Bot,
        Icon::Building2,
        Icon::Calendar,
        Icon::CaseSensitive,
        Icon::ChartPie,
        Icon::Check,
        Icon::ChevronDown,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::ChevronUp,
        Icon::ChevronsUpDown,
        Icon::Circle,
        Icon::CircleAlert,
        Icon::CircleCheck,
        Icon::CirclePause,
        Icon::CircleSlash,
        Icon::CircleUser,
        Icon::CircleX,
        Icon::Close,
        Icon::Command,
        Icon::Copy,
        Icon::CornerDownLeft,
        Icon::CornerDownRight,
        Icon::Cpu,
        Icon::Dash,
        Icon::Delete,
        Icon::Download,
        Icon::Ellipsis,
        Icon::EllipsisVertical,
        Icon::ExternalLink,
        Icon::Eye,
        Icon::EyeOff,
        Icon::File,
        Icon::FileDiff,
        Icon::FilePlus,
        Icon::FileText,
        Icon::Folder,
        Icon::FolderClosed,
        Icon::FolderLock,
        Icon::FolderOpen,
        Icon::FolderUp,
        Icon::Frame,
        Icon::GalleryVerticalEnd,
        Icon::GitBranch,
        Icon::GitFork,
        Icon::Github,
        Icon::Globe,
        Icon::Hand,
        Icon::HardDrive,
        Icon::Heart,
        Icon::HeartOff,
        Icon::House,
        Icon::Inbox,
        Icon::Info,
        Icon::Inspector,
        Icon::Layers,
        Icon::LayoutDashboard,
        Icon::Lightbulb,
        Icon::List,
        Icon::ListTodo,
        Icon::ListTree,
        Icon::Loader,
        Icon::LoaderCircle,
        Icon::Map,
        Icon::Maximize,
        Icon::MemoryStick,
        Icon::Menu,
        Icon::MessageSquare,
        Icon::Minimize,
        Icon::Minus,
        Icon::Moon,
        Icon::Network,
        Icon::Palette,
        Icon::PanelBottom,
        Icon::PanelBottomOpen,
        Icon::PanelLeft,
        Icon::PanelLeftClose,
        Icon::PanelLeftOpen,
        Icon::PanelRight,
        Icon::PanelRightClose,
        Icon::PanelRightOpen,
        Icon::Paperclip,
        Icon::Pause,
        Icon::PenLine,
        Icon::Pencil,
        Icon::Pin,
        Icon::PinOff,
        Icon::Play,
        Icon::Plus,
        Icon::Redo,
        Icon::Redo2,
        Icon::RefreshCw,
        Icon::Replace,
        Icon::ResizeCorner,
        Icon::RotateCw,
        Icon::Search,
        Icon::Server,
        Icon::Settings,
        Icon::Settings2,
        Icon::ShieldAlert,
        Icon::ShieldCheck,
        Icon::ShieldQuestionMark,
        Icon::ShieldX,
        Icon::Slash,
        Icon::SortAscending,
        Icon::SortDescending,
        Icon::Square,
        Icon::SquarePen,
        Icon::SquareTerminal,
        Icon::Star,
        Icon::StarFill,
        Icon::StarOff,
        Icon::Sun,
        Icon::Target,
        Icon::Terminal,
        Icon::ThumbsDown,
        Icon::ThumbsUp,
        Icon::Trash,
        Icon::TriangleAlert,
        Icon::Undo,
        Icon::Undo2,
        Icon::User,
        Icon::Users,
        Icon::Waypoints,
        Icon::WindowClose,
        Icon::WindowMaximize,
        Icon::WindowMinimize,
        Icon::WindowRestore,
        Icon::X,
        Icon::Zap,
    ];

    /// The file stem this icon loads from.
    #[must_use]
    pub fn file_stem(self) -> &'static str {
        match self {
            Icon::ALargeSmall => "a-large-small",
            Icon::Archive => "archive",
            Icon::ArrowDown => "arrow-down",
            Icon::ArrowLeft => "arrow-left",
            Icon::ArrowRight => "arrow-right",
            Icon::ArrowUp => "arrow-up",
            Icon::Asterisk => "asterisk",
            Icon::AtSign => "at-sign",
            Icon::Ban => "ban",
            Icon::Battery => "battery",
            Icon::BatteryCharging => "battery-charging",
            Icon::BatteryFull => "battery-full",
            Icon::BatteryLow => "battery-low",
            Icon::BatteryMedium => "battery-medium",
            Icon::BatteryWarning => "battery-warning",
            Icon::Bell => "bell",
            Icon::BookOpen => "book-open",
            Icon::Bot => "bot",
            Icon::Building2 => "building-2",
            Icon::Calendar => "calendar",
            Icon::CaseSensitive => "case-sensitive",
            Icon::ChartPie => "chart-pie",
            Icon::Check => "check",
            Icon::ChevronDown => "chevron-down",
            Icon::ChevronLeft => "chevron-left",
            Icon::ChevronRight => "chevron-right",
            Icon::ChevronUp => "chevron-up",
            Icon::ChevronsUpDown => "chevrons-up-down",
            Icon::Circle => "circle",
            Icon::CircleAlert => "circle-alert",
            Icon::CircleCheck => "circle-check",
            Icon::CirclePause => "circle-pause",
            Icon::CircleSlash => "circle-slash",
            Icon::CircleUser => "circle-user",
            Icon::CircleX => "circle-x",
            Icon::Close => "close",
            Icon::Command => "command",
            Icon::Copy => "copy",
            Icon::CornerDownLeft => "corner-down-left",
            Icon::CornerDownRight => "corner-down-right",
            Icon::Cpu => "cpu",
            Icon::Dash => "dash",
            Icon::Delete => "delete",
            Icon::Download => "download",
            Icon::Ellipsis => "ellipsis",
            Icon::EllipsisVertical => "ellipsis-vertical",
            Icon::ExternalLink => "external-link",
            Icon::Eye => "eye",
            Icon::EyeOff => "eye-off",
            Icon::File => "file",
            Icon::FileDiff => "file-diff",
            Icon::FilePlus => "file-plus",
            Icon::FileText => "file-text",
            Icon::Folder => "folder",
            Icon::FolderClosed => "folder-closed",
            Icon::FolderLock => "folder-lock",
            Icon::FolderOpen => "folder-open",
            Icon::FolderUp => "folder-up",
            Icon::Frame => "frame",
            Icon::GalleryVerticalEnd => "gallery-vertical-end",
            Icon::GitBranch => "git-branch",
            Icon::GitFork => "git-fork",
            Icon::Github => "github",
            Icon::Globe => "globe",
            Icon::Hand => "hand",
            Icon::HardDrive => "hard-drive",
            Icon::Heart => "heart",
            Icon::HeartOff => "heart-off",
            Icon::House => "house",
            Icon::Inbox => "inbox",
            Icon::Info => "info",
            Icon::Inspector => "inspector",
            Icon::Layers => "layers",
            Icon::LayoutDashboard => "layout-dashboard",
            Icon::Lightbulb => "lightbulb",
            Icon::List => "list",
            Icon::ListTodo => "list-todo",
            Icon::ListTree => "list-tree",
            Icon::Loader => "loader",
            Icon::LoaderCircle => "loader-circle",
            Icon::Map => "map",
            Icon::Maximize => "maximize",
            Icon::MemoryStick => "memory-stick",
            Icon::Menu => "menu",
            Icon::MessageSquare => "message-square",
            Icon::Minimize => "minimize",
            Icon::Minus => "minus",
            Icon::Moon => "moon",
            Icon::Network => "network",
            Icon::Palette => "palette",
            Icon::PanelBottom => "panel-bottom",
            Icon::PanelBottomOpen => "panel-bottom-open",
            Icon::PanelLeft => "panel-left",
            Icon::PanelLeftClose => "panel-left-close",
            Icon::PanelLeftOpen => "panel-left-open",
            Icon::PanelRight => "panel-right",
            Icon::PanelRightClose => "panel-right-close",
            Icon::PanelRightOpen => "panel-right-open",
            Icon::Paperclip => "paperclip",
            Icon::Pause => "pause",
            Icon::PenLine => "pen-line",
            Icon::Pencil => "pencil",
            Icon::Pin => "pin",
            Icon::PinOff => "pin-off",
            Icon::Play => "play",
            Icon::Plus => "plus",
            Icon::Redo => "redo",
            Icon::Redo2 => "redo-2",
            Icon::RefreshCw => "refresh-cw",
            Icon::Replace => "replace",
            Icon::ResizeCorner => "resize-corner",
            Icon::RotateCw => "rotate-cw",
            Icon::Search => "search",
            Icon::Server => "server",
            Icon::Settings => "settings",
            Icon::Settings2 => "settings-2",
            Icon::ShieldAlert => "shield-alert",
            Icon::ShieldCheck => "shield-check",
            Icon::ShieldQuestionMark => "shield-question-mark",
            Icon::ShieldX => "shield-x",
            Icon::Slash => "slash",
            Icon::SortAscending => "sort-ascending",
            Icon::SortDescending => "sort-descending",
            Icon::Square => "square",
            Icon::SquarePen => "square-pen",
            Icon::SquareTerminal => "square-terminal",
            Icon::Star => "star",
            Icon::StarFill => "star-fill",
            Icon::StarOff => "star-off",
            Icon::Sun => "sun",
            Icon::Target => "target",
            Icon::Terminal => "terminal",
            Icon::ThumbsDown => "thumbs-down",
            Icon::ThumbsUp => "thumbs-up",
            Icon::Trash => "trash",
            Icon::TriangleAlert => "triangle-alert",
            Icon::Undo => "undo",
            Icon::Undo2 => "undo-2",
            Icon::User => "user",
            Icon::Users => "users",
            Icon::Waypoints => "waypoints",
            Icon::WindowClose => "window-close",
            Icon::WindowMaximize => "window-maximize",
            Icon::WindowMinimize => "window-minimize",
            Icon::WindowRestore => "window-restore",
            Icon::X => "x",
            Icon::Zap => "zap",
        }
    }

    /// The `bytes://` URI this icon loads under, with the `.svg`
    /// extension the loaders key on.
    #[must_use]
    pub fn uri(self) -> Cow<'static, str> {
        Cow::Owned(format!("{URI_PREFIX}{}.svg", self.file_stem()))
    }

    /// The icon's SVG bytes, strokes set to white for tinting.
    #[must_use]
    pub fn bytes(self) -> Arc<[u8]> {
        static TINTED: LazyLock<HashMap<Icon, Arc<[u8]>>> = LazyLock::new(|| {
            Icon::ALL
                .iter()
                .map(|icon| (*icon, tinted(icon.raw())))
                .collect()
        });
        Arc::clone(&TINTED[&self])
    }

    /// The embedded file, untouched.
    fn raw(self) -> &'static [u8] {
        match self {
            Icon::ALargeSmall => include_bytes!("../assets/icons/a-large-small.svg"),
            Icon::Archive => include_bytes!("../assets/icons/archive.svg"),
            Icon::ArrowDown => include_bytes!("../assets/icons/arrow-down.svg"),
            Icon::ArrowLeft => include_bytes!("../assets/icons/arrow-left.svg"),
            Icon::ArrowRight => include_bytes!("../assets/icons/arrow-right.svg"),
            Icon::ArrowUp => include_bytes!("../assets/icons/arrow-up.svg"),
            Icon::Asterisk => include_bytes!("../assets/icons/asterisk.svg"),
            Icon::AtSign => include_bytes!("../assets/icons/at-sign.svg"),
            Icon::Ban => include_bytes!("../assets/icons/ban.svg"),
            Icon::Battery => include_bytes!("../assets/icons/battery.svg"),
            Icon::BatteryCharging => include_bytes!("../assets/icons/battery-charging.svg"),
            Icon::BatteryFull => include_bytes!("../assets/icons/battery-full.svg"),
            Icon::BatteryLow => include_bytes!("../assets/icons/battery-low.svg"),
            Icon::BatteryMedium => include_bytes!("../assets/icons/battery-medium.svg"),
            Icon::BatteryWarning => include_bytes!("../assets/icons/battery-warning.svg"),
            Icon::Bell => include_bytes!("../assets/icons/bell.svg"),
            Icon::BookOpen => include_bytes!("../assets/icons/book-open.svg"),
            Icon::Bot => include_bytes!("../assets/icons/bot.svg"),
            Icon::Building2 => include_bytes!("../assets/icons/building-2.svg"),
            Icon::Calendar => include_bytes!("../assets/icons/calendar.svg"),
            Icon::CaseSensitive => include_bytes!("../assets/icons/case-sensitive.svg"),
            Icon::ChartPie => include_bytes!("../assets/icons/chart-pie.svg"),
            Icon::Check => include_bytes!("../assets/icons/check.svg"),
            Icon::ChevronDown => include_bytes!("../assets/icons/chevron-down.svg"),
            Icon::ChevronLeft => include_bytes!("../assets/icons/chevron-left.svg"),
            Icon::ChevronRight => include_bytes!("../assets/icons/chevron-right.svg"),
            Icon::ChevronUp => include_bytes!("../assets/icons/chevron-up.svg"),
            Icon::ChevronsUpDown => include_bytes!("../assets/icons/chevrons-up-down.svg"),
            Icon::Circle => include_bytes!("../assets/icons/circle.svg"),
            Icon::CircleAlert => include_bytes!("../assets/icons/circle-alert.svg"),
            Icon::CircleCheck => include_bytes!("../assets/icons/circle-check.svg"),
            Icon::CirclePause => include_bytes!("../assets/icons/circle-pause.svg"),
            Icon::CircleSlash => include_bytes!("../assets/icons/circle-slash.svg"),
            Icon::CircleUser => include_bytes!("../assets/icons/circle-user.svg"),
            Icon::CircleX => include_bytes!("../assets/icons/circle-x.svg"),
            Icon::Close => include_bytes!("../assets/icons/close.svg"),
            Icon::Command => include_bytes!("../assets/icons/command.svg"),
            Icon::Copy => include_bytes!("../assets/icons/copy.svg"),
            Icon::CornerDownLeft => include_bytes!("../assets/icons/corner-down-left.svg"),
            Icon::CornerDownRight => include_bytes!("../assets/icons/corner-down-right.svg"),
            Icon::Cpu => include_bytes!("../assets/icons/cpu.svg"),
            Icon::Dash => include_bytes!("../assets/icons/dash.svg"),
            Icon::Delete => include_bytes!("../assets/icons/delete.svg"),
            Icon::Download => include_bytes!("../assets/icons/download.svg"),
            Icon::Ellipsis => include_bytes!("../assets/icons/ellipsis.svg"),
            Icon::EllipsisVertical => include_bytes!("../assets/icons/ellipsis-vertical.svg"),
            Icon::ExternalLink => include_bytes!("../assets/icons/external-link.svg"),
            Icon::Eye => include_bytes!("../assets/icons/eye.svg"),
            Icon::EyeOff => include_bytes!("../assets/icons/eye-off.svg"),
            Icon::File => include_bytes!("../assets/icons/file.svg"),
            Icon::FileDiff => include_bytes!("../assets/icons/file-diff.svg"),
            Icon::FilePlus => include_bytes!("../assets/icons/file-plus.svg"),
            Icon::FileText => include_bytes!("../assets/icons/file-text.svg"),
            Icon::Folder => include_bytes!("../assets/icons/folder.svg"),
            Icon::FolderClosed => include_bytes!("../assets/icons/folder-closed.svg"),
            Icon::FolderLock => include_bytes!("../assets/icons/folder-lock.svg"),
            Icon::FolderOpen => include_bytes!("../assets/icons/folder-open.svg"),
            Icon::FolderUp => include_bytes!("../assets/icons/folder-up.svg"),
            Icon::Frame => include_bytes!("../assets/icons/frame.svg"),
            Icon::GalleryVerticalEnd => include_bytes!("../assets/icons/gallery-vertical-end.svg"),
            Icon::GitBranch => include_bytes!("../assets/icons/git-branch.svg"),
            Icon::GitFork => include_bytes!("../assets/icons/git-fork.svg"),
            Icon::Github => include_bytes!("../assets/icons/github.svg"),
            Icon::Globe => include_bytes!("../assets/icons/globe.svg"),
            Icon::Hand => include_bytes!("../assets/icons/hand.svg"),
            Icon::HardDrive => include_bytes!("../assets/icons/hard-drive.svg"),
            Icon::Heart => include_bytes!("../assets/icons/heart.svg"),
            Icon::HeartOff => include_bytes!("../assets/icons/heart-off.svg"),
            Icon::House => include_bytes!("../assets/icons/house.svg"),
            Icon::Inbox => include_bytes!("../assets/icons/inbox.svg"),
            Icon::Info => include_bytes!("../assets/icons/info.svg"),
            Icon::Inspector => include_bytes!("../assets/icons/inspector.svg"),
            Icon::Layers => include_bytes!("../assets/icons/layers.svg"),
            Icon::LayoutDashboard => include_bytes!("../assets/icons/layout-dashboard.svg"),
            Icon::Lightbulb => include_bytes!("../assets/icons/lightbulb.svg"),
            Icon::List => include_bytes!("../assets/icons/list.svg"),
            Icon::ListTodo => include_bytes!("../assets/icons/list-todo.svg"),
            Icon::ListTree => include_bytes!("../assets/icons/list-tree.svg"),
            Icon::Loader => include_bytes!("../assets/icons/loader.svg"),
            Icon::LoaderCircle => include_bytes!("../assets/icons/loader-circle.svg"),
            Icon::Map => include_bytes!("../assets/icons/map.svg"),
            Icon::Maximize => include_bytes!("../assets/icons/maximize.svg"),
            Icon::MemoryStick => include_bytes!("../assets/icons/memory-stick.svg"),
            Icon::Menu => include_bytes!("../assets/icons/menu.svg"),
            Icon::MessageSquare => include_bytes!("../assets/icons/message-square.svg"),
            Icon::Minimize => include_bytes!("../assets/icons/minimize.svg"),
            Icon::Minus => include_bytes!("../assets/icons/minus.svg"),
            Icon::Moon => include_bytes!("../assets/icons/moon.svg"),
            Icon::Network => include_bytes!("../assets/icons/network.svg"),
            Icon::Palette => include_bytes!("../assets/icons/palette.svg"),
            Icon::PanelBottom => include_bytes!("../assets/icons/panel-bottom.svg"),
            Icon::PanelBottomOpen => include_bytes!("../assets/icons/panel-bottom-open.svg"),
            Icon::PanelLeft => include_bytes!("../assets/icons/panel-left.svg"),
            Icon::PanelLeftClose => include_bytes!("../assets/icons/panel-left-close.svg"),
            Icon::PanelLeftOpen => include_bytes!("../assets/icons/panel-left-open.svg"),
            Icon::PanelRight => include_bytes!("../assets/icons/panel-right.svg"),
            Icon::PanelRightClose => include_bytes!("../assets/icons/panel-right-close.svg"),
            Icon::PanelRightOpen => include_bytes!("../assets/icons/panel-right-open.svg"),
            Icon::Paperclip => include_bytes!("../assets/icons/paperclip.svg"),
            Icon::Pause => include_bytes!("../assets/icons/pause.svg"),
            Icon::PenLine => include_bytes!("../assets/icons/pen-line.svg"),
            Icon::Pencil => include_bytes!("../assets/icons/pencil.svg"),
            Icon::Pin => include_bytes!("../assets/icons/pin.svg"),
            Icon::PinOff => include_bytes!("../assets/icons/pin-off.svg"),
            Icon::Play => include_bytes!("../assets/icons/play.svg"),
            Icon::Plus => include_bytes!("../assets/icons/plus.svg"),
            Icon::Redo => include_bytes!("../assets/icons/redo.svg"),
            Icon::Redo2 => include_bytes!("../assets/icons/redo-2.svg"),
            Icon::RefreshCw => include_bytes!("../assets/icons/refresh-cw.svg"),
            Icon::Replace => include_bytes!("../assets/icons/replace.svg"),
            Icon::ResizeCorner => include_bytes!("../assets/icons/resize-corner.svg"),
            Icon::RotateCw => include_bytes!("../assets/icons/rotate-cw.svg"),
            Icon::Search => include_bytes!("../assets/icons/search.svg"),
            Icon::Server => include_bytes!("../assets/icons/server.svg"),
            Icon::Settings => include_bytes!("../assets/icons/settings.svg"),
            Icon::Settings2 => include_bytes!("../assets/icons/settings-2.svg"),
            Icon::ShieldAlert => include_bytes!("../assets/icons/shield-alert.svg"),
            Icon::ShieldCheck => include_bytes!("../assets/icons/shield-check.svg"),
            Icon::ShieldQuestionMark => include_bytes!("../assets/icons/shield-question-mark.svg"),
            Icon::ShieldX => include_bytes!("../assets/icons/shield-x.svg"),
            Icon::Slash => include_bytes!("../assets/icons/slash.svg"),
            Icon::SortAscending => include_bytes!("../assets/icons/sort-ascending.svg"),
            Icon::SortDescending => include_bytes!("../assets/icons/sort-descending.svg"),
            Icon::Square => include_bytes!("../assets/icons/square.svg"),
            Icon::SquarePen => include_bytes!("../assets/icons/square-pen.svg"),
            Icon::SquareTerminal => include_bytes!("../assets/icons/square-terminal.svg"),
            Icon::Star => include_bytes!("../assets/icons/star.svg"),
            Icon::StarFill => include_bytes!("../assets/icons/star-fill.svg"),
            Icon::StarOff => include_bytes!("../assets/icons/star-off.svg"),
            Icon::Sun => include_bytes!("../assets/icons/sun.svg"),
            Icon::Target => include_bytes!("../assets/icons/target.svg"),
            Icon::Terminal => include_bytes!("../assets/icons/terminal.svg"),
            Icon::ThumbsDown => include_bytes!("../assets/icons/thumbs-down.svg"),
            Icon::ThumbsUp => include_bytes!("../assets/icons/thumbs-up.svg"),
            Icon::Trash => include_bytes!("../assets/icons/trash.svg"),
            Icon::TriangleAlert => include_bytes!("../assets/icons/triangle-alert.svg"),
            Icon::Undo => include_bytes!("../assets/icons/undo.svg"),
            Icon::Undo2 => include_bytes!("../assets/icons/undo-2.svg"),
            Icon::User => include_bytes!("../assets/icons/user.svg"),
            Icon::Users => include_bytes!("../assets/icons/users.svg"),
            Icon::Waypoints => include_bytes!("../assets/icons/waypoints.svg"),
            Icon::WindowClose => include_bytes!("../assets/icons/window-close.svg"),
            Icon::WindowMaximize => include_bytes!("../assets/icons/window-maximize.svg"),
            Icon::WindowMinimize => include_bytes!("../assets/icons/window-minimize.svg"),
            Icon::WindowRestore => include_bytes!("../assets/icons/window-restore.svg"),
            Icon::X => include_bytes!("../assets/icons/x.svg"),
            Icon::Zap => include_bytes!("../assets/icons/zap.svg"),
        }
    }
}

/// Replaces `currentColor` with the white tint base, so egui's tint
/// multiply paints the theme color instead of black.
fn tinted(bytes: &'static [u8]) -> Arc<[u8]> {
    let needle = b"currentColor";
    if !bytes.windows(needle.len()).any(|w| w == needle) {
        return Arc::from(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some(at) = rest.windows(needle.len()).position(|w| w == needle) {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(TINT_BASE.as_bytes());
        rest = &rest[at + needle.len()..];
    }
    out.extend_from_slice(rest);
    Arc::from(out)
}

/// Registers the image loaders the icons need. Call once at startup,
/// before the first frame that draws one.
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

/// Draws one icon at `size`, tinted `color`.
pub fn icon(ui: &mut egui::Ui, icon: Icon, size: f32, color: egui::Color32) -> egui::Response {
    themed_image(icon, size, color).ui(ui)
}

/// Draws one icon at `size` in the active palette's ink.
pub fn themed(ui: &mut egui::Ui, icon: Icon, size: f32) -> egui::Response {
    let palette = crate::theme::Palette::active_of(ui.ctx());
    themed_image(icon, size, palette.ink).ui(ui)
}

/// Painting an image on the pixel grid, as the reference rounds every
/// box it lays out, so an icon on a fractional position stays crisp
/// and lands where the reference draws it.
pub(crate) trait PaintSnapped {
    /// Paints the image into `rect` rounded to whole pixels.
    fn paint_snapped(self, ui: &egui::Ui, rect: egui::Rect);
}

impl PaintSnapped for egui::Image<'_> {
    fn paint_snapped(self, ui: &egui::Ui, rect: egui::Rect) {
        use egui::emath::GuiRounding as _;
        self.paint_at(ui, rect.round_to_pixels(ui.pixels_per_point()));
    }
}

/// The image widget for one icon, for buttons that take an image.
pub(crate) fn themed_image(icon: Icon, size: f32, color: egui::Color32) -> egui::Image<'static> {
    egui::Image::new(egui::ImageSource::Bytes {
        uri: icon.uri(),
        bytes: icon.bytes().into(),
    })
    .tint(color)
    .max_size(egui::Vec2::splat(size))
}

#[cfg(test)]
mod tests {
    use super::Icon;

    #[test]
    fn every_icon_has_a_unique_uri_and_svg_bytes() {
        let mut uris = std::collections::BTreeSet::new();
        for icon in Icon::ALL {
            let bytes = icon.raw();
            let svg = b"<svg";
            assert!(
                bytes.windows(svg.len()).any(|w| w == svg),
                "{} does not look like an SVG",
                icon.file_stem()
            );
            assert!(uris.insert(icon.uri()), "duplicate uri: {}", icon.uri());
        }
        assert_eq!(uris.len(), Icon::ALL.len());
    }

    #[test]
    fn tinting_replaces_current_color() {
        for icon in Icon::ALL {
            let bytes = icon.bytes();
            let text = String::from_utf8_lossy(&bytes);
            assert!(
                !text.contains("currentColor"),
                "{} kept currentColor",
                icon.file_stem()
            );
        }
    }
}
