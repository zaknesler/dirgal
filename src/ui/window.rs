use std::path::PathBuf;

use crate::ui::gallery::{CloseLightbox, Gallery, Next, Prev, Quit, ZoomIn, ZoomOut, ZoomReset};
use gpui::{
    App, AppContext as _, Bounds, KeyBinding, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use gpui_component::Root;

#[tracing::instrument(skip_all, fields(images = images.len()))]
pub fn create_window(roots: Vec<PathBuf>, images: Vec<crate::image::ImageEntry>) {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            gpui_component::theme::Theme::change(gpui_component::theme::ThemeMode::Dark, None, cx);
            cx.activate(true);

            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("ctrl-q", Quit, None),
                KeyBinding::new("left", Prev, Some("Gallery")),
                KeyBinding::new("right", Next, Some("Gallery")),
                KeyBinding::new("escape", CloseLightbox, Some("Gallery")),
                KeyBinding::new("=", ZoomIn, Some("Gallery")),
                KeyBinding::new("-", ZoomOut, Some("Gallery")),
                KeyBinding::new("0", ZoomReset, Some("Gallery")),
            ]);

            let bounds = Bounds::centered(None, size(px(1920.), px(1080.)), cx);
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Gallery")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };

            cx.open_window(options, move |window, cx| {
                let view = Gallery::view(window, cx, roots, images);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        });
}
