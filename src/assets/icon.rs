use std::sync::Arc;
use strum::{EnumIter, EnumString, IntoStaticStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum IconAsset {
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    Bookmark,
    BookmarkOff,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronsUpDown,
    ChevronUp,
    CircleX,
    Close,
    Copy,
    Ellipsis,
    EllipsisVertical,
    ExternalLink,
    Eye,
    EyeOff,
    File,
    Folder,
    FolderClosed,
    FolderOpen,
    Frame,
    GalleryVertical,
    GalleryVerticalEnd,
    Grid,
    Heart,
    HeartOff,
    Layers,
    LayoutDashboard,
    LayoutList,
    Loader,
    LoaderCircle,
    Maximize,
    Menu,
    Minimize,
    Minus,
    Plus,
    Refresh,
    Search,
    Settings,
    Settings2,
    SortAscending,
    SortDescending,
    Star,
    StarFill,
    StarOff,
    ThumbsDown,
    ThumbsUp,
    TriangleAlert,
}

impl IconAsset {
    pub fn path(&self) -> Arc<str> {
        let file: &'static str = self.into();
        format!("icons/{file}.svg").into()
    }
}

impl From<IconAsset> for gpui_component::Icon {
    fn from(value: IconAsset) -> Self {
        Self::default().path(value.path())
    }
}
