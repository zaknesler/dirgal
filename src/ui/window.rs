use crate::ui::{
    actions,
    gallery::Gallery,
    state::{AppState, SharedAppState},
};
use gpui::{App, AppContext as _, KeyBinding, TitlebarOptions, WindowOptions};

pub fn create_window(state: AppState) {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            gpui_component::theme::Theme::sync_system_appearance(None, cx);

            let shared = SharedAppState::new(state, cx);
            cx.set_global(shared);

            register_actions(cx);

            let options = WindowOptions {
                app_id: Some("dirgal".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("dirgal".into()),
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

fn register_actions(cx: &mut App) {
    macro_rules! bind_keys {
        ($context:expr, $(($key:expr, $action:expr)),* $(,)?) => {
            cx.bind_keys([$( KeyBinding::new($key, $action, $context) ),*]);
        };
    }

    cx.on_action(|_: &actions::Quit, cx| cx.quit());

    // Global
    bind_keys!(
        None,
        ("secondary-q", actions::Quit),
        ("ctrl-shift-w", actions::Quit)
    );

    // Gallery
    bind_keys!(
        Some(crate::ui::CONTEXT_GALLERY),
        ("ctrl-tab", actions::NextPage),
        ("ctrl-shift-tab", actions::PrevPage),
        ("escape", actions::CloseLightbox),
        ("secondary-k", actions::FocusSearch),
    );

    // Gallery (unfocused)
    bind_keys!(
        Some(crate::ui::CONTEXT_GALLERY_UNFOCUSED),
        ("left", actions::Prev),
        ("right", actions::Next),
        ("space", actions::OpenLightbox),
        ("g", actions::ToggleGrouped),
        ("b", actions::Bookmark::Current),
        ("o", actions::OpenInFinder::Current),
        ("=", actions::ZoomIn),
        ("-", actions::ZoomOut),
        ("0", actions::ZoomReset),
        ("c", actions::CollapseAll),
    );
}
