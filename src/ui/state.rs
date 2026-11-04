use gpui_kit::gpui::AppContext as _;

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: crate::core::config::AppConfig,
    pub scanner: crate::core::scan::ImageScanner,
}

impl gpui_kit::gpui::EventEmitter<()> for AppState {}

#[derive(Clone)]
pub struct SharedAppState(pub gpui_kit::gpui::Entity<AppState>);

impl gpui_kit::gpui::Global for SharedAppState {}

impl SharedAppState {
    pub fn new(initial_state: AppState, cx: &mut gpui_kit::gpui::App) -> Self {
        Self(cx.new(|_| initial_state))
    }

    pub fn from_app(cx: &mut gpui_kit::gpui::App) -> Self {
        cx.global::<Self>().clone()
    }

    pub fn entity(&self) -> &gpui_kit::gpui::Entity<AppState> {
        &self.0
    }
}
