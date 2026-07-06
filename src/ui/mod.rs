pub mod gallery;
pub mod window;

pub mod actions {
    gpui::actions!(
        gallery,
        [Quit, Prev, Next, CloseLightbox, ZoomIn, ZoomOut, ZoomReset]
    );
}
