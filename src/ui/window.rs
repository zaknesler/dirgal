use crate::ui::{
    gallery::Gallery,
    state::{AppState, SharedAppState},
};
use gpui::{App, AppContext as _, TitlebarOptions, WindowOptions};
use gpui_component::{Theme, ThemeMode};

pub fn create_window(state: AppState) {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);

            super::actions::register_actions(cx);

            let roots_str = state
                .scanner
                .roots
                .iter()
                .map(|r| r.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" · ");
            let title = format!("dirgal - {}", roots_str);

            let options = WindowOptions {
                app_id: Some("dirgal".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let shared = SharedAppState::new(state, cx);
            cx.set_global(shared);

            cx.open_window(options, move |window, cx| {
                let view = Gallery::view(window, cx);
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("failed to open window");

            cx.activate(true);
        });
}
