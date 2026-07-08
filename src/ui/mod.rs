pub mod cache;
pub mod gallery;
pub mod state;
pub mod window;

pub const CONTEXT_GALLERY: &str = "gallery";
pub const CONTEXT_LIGHTBOX: &str = "lightbox";
pub const CONTEXT_GRID: &str = "grid";
pub const CONTEXT_GALLERY_UNFOCUSED: &str = "gallery && !Input";

pub mod actions {
    gpui::actions!(
        gallery,
        [
            Quit,
            Prev,
            Next,
            OpenLightbox,
            CloseLightbox,
            ZoomIn,
            ZoomOut,
            ZoomReset,
            PrevPage,
            NextPage,
            Bookmark
        ]
    );
}
