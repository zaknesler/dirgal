use num_format::{Locale, ToFormattedString};
use std::path::Path;

pub fn file_manager_label() -> &'static str {
    #[cfg(target_os = "macos")]
    return "Finder";

    #[cfg(target_os = "windows")]
    return "Explorer";

    #[cfg(target_os = "linux")]
    return "Files";

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return "File Manager";
}

/// Open the given path in the file explorer for the current OS
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

/// Formats a number as a string with commas for readability
pub fn format_num(n: usize) -> String {
    n.to_formatted_string(&Locale::en)
}

/// Parses a search query into required terms (`a|b` is OR) and excluded `!` keywords
pub fn parse_search(query: &str) -> (Vec<Vec<&str>>, Vec<&str>) {
    let mut required = Vec::new();
    let mut excluded = Vec::new();

    for word in query.split_whitespace() {
        let (negated, word) = match word.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, word),
        };
        let mut alternatives: Vec<&str> = word.split('|').filter(|s| !s.is_empty()).collect();
        alternatives.sort_unstable();
        alternatives.dedup();

        if negated {
            excluded.extend(alternatives);
        } else if !alternatives.is_empty() {
            required.push(alternatives);
        }
    }

    required.sort_unstable();
    required.dedup();

    excluded.sort_unstable();
    excluded.dedup();

    (required, excluded)
}

/// Checks text against the output of `parse_search`
pub fn matches_search(text: &str, required: &[Vec<&str>], excluded: &[&str]) -> bool {
    required
        .iter()
        .all(|alternatives| alternatives.iter().any(|k| text.contains(k)))
        && !excluded.iter().any(|k| text.contains(k))
}
