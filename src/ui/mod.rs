pub mod gallery;
pub mod state;
pub mod window;

pub const CONTEXT_GALLERY: &str = "gallery";
pub const CONTEXT_LIGHTBOX: &str = "lightbox";

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
            BookmarkActive
        ]
    );
}
