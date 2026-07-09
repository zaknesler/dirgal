use crate::ui::{
    CONTEXT_GALLERY, CONTEXT_GALLERY_UNFOCUSED, actions,
    gallery::Gallery,
    state::{AppState, SharedAppState},
};
use gpui::{App, AppContext as _, KeyBinding, SharedString, TitlebarOptions, WindowOptions};

#[tracing::instrument(skip(state))]
pub fn create_window(state: AppState) {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            gpui_component::theme::Theme::change(gpui_component::theme::ThemeMode::Dark, None, cx);

            let shared = SharedAppState::new(state, cx);
            cx.set_global(shared);

            cx.on_action(|_: &actions::Quit, cx| cx.quit());

            cx.bind_keys([
                KeyBinding::new("cmd-q", actions::Quit, None),
                KeyBinding::new("ctrl-w", actions::Quit, None),
                KeyBinding::new("ctrl-tab", actions::NextPage, Some(CONTEXT_GALLERY)),
                KeyBinding::new("ctrl-shift-tab", actions::PrevPage, Some(CONTEXT_GALLERY)),
                KeyBinding::new("escape", actions::CloseLightbox, Some(CONTEXT_GALLERY)),
                KeyBinding::new("left", actions::Prev, Some(CONTEXT_GALLERY_UNFOCUSED)),
                KeyBinding::new("right", actions::Next, Some(CONTEXT_GALLERY_UNFOCUSED)),
                KeyBinding::new("g", actions::OpenLightbox, Some(CONTEXT_GALLERY_UNFOCUSED)),
                KeyBinding::new("b", actions::Bookmark, Some(CONTEXT_GALLERY_UNFOCUSED)),
                KeyBinding::new("=", actions::ZoomIn, Some(CONTEXT_GALLERY_UNFOCUSED)),
                KeyBinding::new("-", actions::ZoomOut, Some(CONTEXT_GALLERY_UNFOCUSED)),
                KeyBinding::new("0", actions::ZoomReset, Some(CONTEXT_GALLERY_UNFOCUSED)),
                KeyBinding::new("c", actions::CollapseAll, Some(CONTEXT_GALLERY_UNFOCUSED)),
            ]);

            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("dirgal")),
                    ..Default::default()
                }),
                ..Default::default()
            };

            cx.open_window(options, move |window, cx| {
                let view = Gallery::view(window, cx);
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("failed to open window");

            cx.activate(true);
        });
}
