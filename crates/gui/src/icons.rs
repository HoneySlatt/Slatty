//! Lucide icons (ISC license, see assets/icons/LICENSE), tinted with the given colour.

use iced::widget::{Svg, svg};
use iced::{Color, Theme};

#[derive(Debug, Clone, Copy)]
pub enum Icon {
    ArrowRight,
    Calendar,
    ChevronLeft,
    CircleCheck,
    Clock,
    Cloud,
    Download,
    EllipsisVertical,
    FolderOpen,
    HardDrive,
    Heart,
    HeartFilled,
    LayoutGrid,
    ListFilter,
    Play,
    RefreshCw,
    Search,
    Settings,
    SlidersHorizontal,
    Stop,
    TriangleAlert,
    Trophy,
    X,
}

impl Icon {
    fn bytes(self) -> &'static [u8] {
        match self {
            Icon::ArrowRight => include_bytes!("../assets/icons/arrow-right.svg"),
            Icon::Calendar => include_bytes!("../assets/icons/calendar.svg"),
            Icon::ChevronLeft => include_bytes!("../assets/icons/chevron-left.svg"),
            Icon::CircleCheck => include_bytes!("../assets/icons/circle-check.svg"),
            Icon::Clock => include_bytes!("../assets/icons/clock.svg"),
            Icon::Cloud => include_bytes!("../assets/icons/cloud.svg"),
            Icon::Download => include_bytes!("../assets/icons/download.svg"),
            Icon::EllipsisVertical => include_bytes!("../assets/icons/ellipsis-vertical.svg"),
            Icon::FolderOpen => include_bytes!("../assets/icons/folder-open.svg"),
            Icon::HardDrive => include_bytes!("../assets/icons/hard-drive.svg"),
            Icon::Heart => include_bytes!("../assets/icons/heart.svg"),
            Icon::HeartFilled => include_bytes!("../assets/icons/heart-filled.svg"),
            Icon::LayoutGrid => include_bytes!("../assets/icons/layout-grid.svg"),
            Icon::ListFilter => include_bytes!("../assets/icons/list-filter.svg"),
            Icon::Play => include_bytes!("../assets/icons/play-filled.svg"),
            Icon::RefreshCw => include_bytes!("../assets/icons/refresh-cw.svg"),
            Icon::Search => include_bytes!("../assets/icons/search.svg"),
            Icon::Settings => include_bytes!("../assets/icons/settings.svg"),
            Icon::SlidersHorizontal => include_bytes!("../assets/icons/sliders-horizontal.svg"),
            Icon::Stop => include_bytes!("../assets/icons/square-filled.svg"),
            Icon::TriangleAlert => include_bytes!("../assets/icons/triangle-alert.svg"),
            Icon::Trophy => include_bytes!("../assets/icons/trophy.svg"),
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
