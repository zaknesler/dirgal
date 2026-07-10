use num_format::{Locale, ToFormattedString};
use std::path::Path;

pub fn reveal_in_file_manager(path: &Path) {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .ok();
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(path)
        .spawn()
        .ok();
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn()
        .ok();
}

pub fn format_num(n: usize) -> String {
    n.to_formatted_string(&Locale::en)
}
