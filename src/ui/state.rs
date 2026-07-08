use gpui::AppContext as _;

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: crate::config::AppConfig,
    pub roots: Vec<std::path::PathBuf>,
    pub images: Vec<crate::image::ImageEntry>,
}

#[derive(Clone)]
pub struct SharedAppState(pub gpui::Entity<AppState>);

impl gpui::Global for SharedAppState {}

impl gpui::EventEmitter<()> for AppState {}

impl SharedAppState {
    pub fn new(initial_state: AppState, cx: &mut gpui::App) -> Self {
        Self(cx.new(|_| initial_state))
    }

    pub fn from_app(cx: &mut gpui::App) -> Self {
        cx.global::<Self>().clone()
    }

    pub fn entity(&self) -> &gpui::Entity<AppState> {
        &self.0
    }
}
