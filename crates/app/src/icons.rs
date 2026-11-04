//! The icons the interface draws (Lucide's), by name: the state says which one a button shows.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    ArrowDown,
    ArrowRight,
    ArrowRightLeft,
    ArrowUp,
    Bell,
    Calendar,
    Check,
    ChevronLeft,
    CircleCheck,
    Clock,
    Cloud,
    Download,
    EllipsisVertical,
    FileCheck,
    Folder,
    Gamepad,
    FolderOpen,
    Globe,
    GripVertical,
    HardDrive,
    Heart,
    HeartFilled,
    Info,
    LayoutGrid,
    ListFilter,
    Package,
    Palette,
    PanelLeftClose,
    PanelLeftOpen,
    Pause,
    Pencil,
    Play,
    Plus,
    Puzzle,
    RefreshCw,
    Search,
    Settings,
    Shield,
    SlidersHorizontal,
    Stop,
    Trash,
    TriangleAlert,
    Trophy,
    User,
    Wrench,
    X,
}

/// An icon's file (Lucide's, ISC license, see crates/gui/assets/icons/LICENSE): its name, and its
/// SVG built into the binary.
macro_rules! lucide {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!("../../gui/assets/icons/", $name, ".svg")),
        )
    };
}

impl Icon {
    /// Every icon, for an interface that finds one by its file's name.
    pub const ALL: [Icon; 46] = [
        Icon::ArrowDown,
        Icon::ArrowRight,
        Icon::ArrowRightLeft,
        Icon::ArrowUp,
        Icon::Bell,
        Icon::Calendar,
        Icon::Check,
        Icon::ChevronLeft,
        Icon::CircleCheck,
        Icon::Clock,
        Icon::Cloud,
        Icon::Download,
        Icon::EllipsisVertical,
        Icon::FileCheck,
        Icon::Folder,
        Icon::Gamepad,
        Icon::FolderOpen,
        Icon::Globe,
        Icon::GripVertical,
        Icon::HardDrive,
        Icon::Heart,
        Icon::HeartFilled,
        Icon::Info,
        Icon::LayoutGrid,
        Icon::ListFilter,
        Icon::Package,
        Icon::Palette,
        Icon::PanelLeftClose,
        Icon::PanelLeftOpen,
        Icon::Pause,
        Icon::Pencil,
        Icon::Play,
        Icon::Plus,
        Icon::Puzzle,
        Icon::RefreshCw,
        Icon::Search,
        Icon::Settings,
        Icon::Shield,
        Icon::SlidersHorizontal,
        Icon::Stop,
        Icon::Trash,
        Icon::TriangleAlert,
        Icon::Trophy,
        Icon::User,
        Icon::Wrench,
        Icon::X,
    ];

    /// The name of the icon's file, without its extension.
    pub fn name(self) -> &'static str {
        self.file().0
    }

    /// The icon's SVG.
    pub fn svg(self) -> &'static [u8] {
        self.file().1
    }

    fn file(self) -> (&'static str, &'static [u8]) {
        match self {
            Icon::ArrowDown => lucide!("arrow-down"),
            Icon::ArrowRight => lucide!("arrow-right"),
            Icon::ArrowRightLeft => lucide!("arrow-right-left"),
            Icon::ArrowUp => lucide!("arrow-up"),
            Icon::Bell => lucide!("bell"),
            Icon::Calendar => lucide!("calendar"),
            Icon::Check => lucide!("check"),
            Icon::ChevronLeft => lucide!("chevron-left"),
            Icon::CircleCheck => lucide!("circle-check"),
            Icon::Clock => lucide!("clock"),
            Icon::Cloud => lucide!("cloud"),
            Icon::Download => lucide!("download"),
            Icon::EllipsisVertical => lucide!("ellipsis-vertical"),
            Icon::FileCheck => lucide!("file-check"),
            Icon::Folder => lucide!("folder"),
            Icon::Gamepad => lucide!("gamepad-2"),
            Icon::FolderOpen => lucide!("folder-open"),
            Icon::Globe => lucide!("globe"),
            Icon::GripVertical => lucide!("grip-vertical"),
            Icon::HardDrive => lucide!("hard-drive"),
            Icon::Heart => lucide!("heart"),
            Icon::HeartFilled => lucide!("heart-filled"),
            Icon::Info => lucide!("info"),
            Icon::LayoutGrid => lucide!("layout-grid"),
            Icon::ListFilter => lucide!("list-filter"),
            Icon::Package => lucide!("package"),
            Icon::PanelLeftClose => lucide!("panel-left-close"),
            Icon::PanelLeftOpen => lucide!("panel-left-open"),
            Icon::Palette => lucide!("palette"),
            Icon::Pause => lucide!("pause-filled"),
            Icon::Pencil => lucide!("pencil"),
            Icon::Play => lucide!("play-filled"),
            Icon::Plus => lucide!("plus"),
            Icon::Puzzle => lucide!("puzzle"),
            Icon::RefreshCw => lucide!("refresh-cw"),
            Icon::Search => lucide!("search"),
            Icon::Settings => lucide!("settings"),
            Icon::Shield => lucide!("shield"),
            Icon::SlidersHorizontal => lucide!("sliders-horizontal"),
            Icon::Stop => lucide!("square-filled"),
            Icon::Trash => lucide!("trash-2"),
            Icon::TriangleAlert => lucide!("triangle-alert"),
            Icon::Trophy => lucide!("trophy"),
            Icon::User => lucide!("user"),
            Icon::Wrench => lucide!("wrench"),
            Icon::X => lucide!("x"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ALL` names each icon once, and every file shipped is some icon's: an icon left out of it
    /// leaves a file over.
    #[test]
    fn every_icon_is_listed_once_with_its_file() {
        let mut names: Vec<&str> = Icon::ALL.iter().map(|i| i.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Icon::ALL.len());
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../gui/assets/icons");
        let mut files: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .filter_map(|f| {
                f.ok()?
                    .file_name()
                    .to_str()?
                    .strip_suffix(".svg")
                    .map(String::from)
            })
            .collect();
        files.sort_unstable();
        assert_eq!(files, names);
    }
}
