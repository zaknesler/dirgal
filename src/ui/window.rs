use std::path::PathBuf;

use crate::ui::{actions, gallery::Gallery};
use gpui::{App, AppContext as _, KeyBinding, SharedString, TitlebarOptions, WindowOptions};
use gpui_component::Root;

#[tracing::instrument(skip_all, fields(images = images.len()))]
pub fn create_window(roots: Vec<PathBuf>, images: Vec<crate::image::ImageEntry>) {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            gpui_component::theme::Theme::change(gpui_component::theme::ThemeMode::Dark, None, cx);

            cx.activate(true);

            cx.on_action(|_: &actions::Quit, cx| cx.quit());
            cx.bind_keys([
                KeyBinding::new("cmd-q", actions::Quit, None),
                KeyBinding::new("ctrl-q", actions::Quit, None),
                KeyBinding::new("left", actions::Prev, Some("Gallery")),
                KeyBinding::new("right", actions::Next, Some("Gallery")),
                KeyBinding::new("escape", actions::CloseLightbox, Some("Gallery")),
                KeyBinding::new("=", actions::ZoomIn, Some("Gallery")),
                KeyBinding::new("-", actions::ZoomOut, Some("Gallery")),
                KeyBinding::new("0", actions::ZoomReset, Some("Gallery")),
            ]);

            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Gallery")),
                    ..Default::default()
                }),
                ..Default::default()
            };

            cx.open_window(options, move |window, cx| {
                let view = Gallery::view(window, cx, roots, images);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        });
}
