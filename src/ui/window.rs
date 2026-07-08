use std::path::PathBuf;

use crate::ui::{CONTEXT_GALLERY, actions, gallery::Gallery};
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
                KeyBinding::new("ctrl-w", actions::Quit, None),
                KeyBinding::new("ctrl-tab", actions::NextPage, Some(CONTEXT_GALLERY)),
                KeyBinding::new("ctrl-shift-tab", actions::PrevPage, Some(CONTEXT_GALLERY)),
                KeyBinding::new("left", actions::Prev, Some(CONTEXT_GALLERY)),
                KeyBinding::new("right", actions::Next, Some(CONTEXT_GALLERY)),
                KeyBinding::new("escape", actions::CloseLightbox, Some(CONTEXT_GALLERY)),
                KeyBinding::new("g", actions::OpenLightbox, Some(CONTEXT_GALLERY)),
                KeyBinding::new("b", actions::BookmarkActive, Some(CONTEXT_GALLERY)),
                KeyBinding::new("=", actions::ZoomIn, Some(CONTEXT_GALLERY)),
                KeyBinding::new("-", actions::ZoomOut, Some(CONTEXT_GALLERY)),
                KeyBinding::new("0", actions::ZoomReset, Some(CONTEXT_GALLERY)),
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
