use crate::ui::{
    gallery::Gallery,
    state::{AppState, SharedAppState},
};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::gpui::{App, AppContext as _, TitlebarOptions, WindowOptions, px, size};

pub fn create_window(state: AppState) {
    gpui_kit::application()
        .with_assets(crate::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
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
                window_min_size: Some(size(px(800.), px(400.))),
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
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            })
            .expect("failed to open window");

            cx.activate(true);
        });
}
