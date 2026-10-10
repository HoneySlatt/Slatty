//! Lucide icons (ISC license, see assets/icons/LICENSE), tinted with the given colour.

use iced::widget::{Svg, svg};
use iced::{Color, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    ArrowDown,
    ArrowRight,
    ArrowRightLeft,
    ArrowUp,
    Calendar,
    ChevronLeft,
    ChevronRight,
    CircleCheck,
    Clock,
    Cloud,
    Download,
    EllipsisVertical,
    FileCheck,
    Folder,
    FolderOpen,
    Globe,
    GripVertical,
    HardDrive,
    Heart,
    HeartFilled,
    LayoutGrid,
    ListFilter,
    Package,
    Pause,
    Pencil,
    Play,
    Puzzle,
    RefreshCw,
    Search,
    Settings,
    SlidersHorizontal,
    Stop,
    Trash,
    TriangleAlert,
    Trophy,
    Wrench,
    X,
}

impl Icon {
    fn bytes(self) -> &'static [u8] {
        match self {
            Icon::ArrowDown => include_bytes!("../assets/icons/arrow-down.svg"),
            Icon::ArrowRight => include_bytes!("../assets/icons/arrow-right.svg"),
            Icon::ArrowRightLeft => include_bytes!("../assets/icons/arrow-right-left.svg"),
            Icon::ArrowUp => include_bytes!("../assets/icons/arrow-up.svg"),
            Icon::Calendar => include_bytes!("../assets/icons/calendar.svg"),
            Icon::ChevronLeft => include_bytes!("../assets/icons/chevron-left.svg"),
            Icon::ChevronRight => include_bytes!("../assets/icons/chevron-right.svg"),
            Icon::CircleCheck => include_bytes!("../assets/icons/circle-check.svg"),
            Icon::Clock => include_bytes!("../assets/icons/clock.svg"),
            Icon::Cloud => include_bytes!("../assets/icons/cloud.svg"),
            Icon::Download => include_bytes!("../assets/icons/download.svg"),
            Icon::EllipsisVertical => include_bytes!("../assets/icons/ellipsis-vertical.svg"),
            Icon::FileCheck => include_bytes!("../assets/icons/file-check.svg"),
            Icon::Folder => include_bytes!("../assets/icons/folder.svg"),
            Icon::FolderOpen => include_bytes!("../assets/icons/folder-open.svg"),
            Icon::Globe => include_bytes!("../assets/icons/globe.svg"),
            Icon::GripVertical => include_bytes!("../assets/icons/grip-vertical.svg"),
            Icon::HardDrive => include_bytes!("../assets/icons/hard-drive.svg"),
            Icon::Heart => include_bytes!("../assets/icons/heart.svg"),
            Icon::HeartFilled => include_bytes!("../assets/icons/heart-filled.svg"),
            Icon::LayoutGrid => include_bytes!("../assets/icons/layout-grid.svg"),
            Icon::ListFilter => include_bytes!("../assets/icons/list-filter.svg"),
            Icon::Package => include_bytes!("../assets/icons/package.svg"),
            Icon::Pause => include_bytes!("../assets/icons/pause-filled.svg"),
            Icon::Pencil => include_bytes!("../assets/icons/pencil.svg"),
            Icon::Play => include_bytes!("../assets/icons/play-filled.svg"),
            Icon::Puzzle => include_bytes!("../assets/icons/puzzle.svg"),
            Icon::RefreshCw => include_bytes!("../assets/icons/refresh-cw.svg"),
            Icon::Search => include_bytes!("../assets/icons/search.svg"),
            Icon::Settings => include_bytes!("../assets/icons/settings.svg"),
            Icon::SlidersHorizontal => include_bytes!("../assets/icons/sliders-horizontal.svg"),
            Icon::Stop => include_bytes!("../assets/icons/square-filled.svg"),
            Icon::Trash => include_bytes!("../assets/icons/trash-2.svg"),
            Icon::TriangleAlert => include_bytes!("../assets/icons/triangle-alert.svg"),
            Icon::Trophy => include_bytes!("../assets/icons/trophy.svg"),
            Icon::Wrench => include_bytes!("../assets/icons/wrench.svg"),
            Icon::X => include_bytes!("../assets/icons/x.svg"),
        }
    }
}

pub fn icon<'a>(which: Icon, size: f32, color: Color) -> Svg<'a, Theme> {
    svg(svg::Handle::from_memory(which.bytes()))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
}
