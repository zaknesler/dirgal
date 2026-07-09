use num_format::{Locale, ToFormattedString};

pub fn format_num(n: usize) -> String {
    n.to_formatted_string(&Locale::en)
}
