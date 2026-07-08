#[derive(Debug, Clone)]
pub struct AppState {
    pub config: crate::config::AppConfig,
    pub roots: Vec<std::path::PathBuf>,
    pub images: Vec<crate::image::ImageEntry>,
}

impl gpui::Global for AppState {}

impl AppState {
    pub fn from_app(cx: &mut gpui::App) -> Self {
        cx.global::<Self>().clone()
    }
}
