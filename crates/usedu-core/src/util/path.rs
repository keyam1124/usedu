use std::ffi::OsStr;
use std::path::Path;

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn display_path_human(path: &Path) -> String {
    escape_untrusted_text(&path.to_string_lossy())
}

pub fn display_os_str_human(value: &OsStr) -> String {
    escape_untrusted_text(&value.to_string_lossy())
}

pub fn escape_untrusted_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{1b}' => escaped.push_str("\\x1b"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{{{:x}}}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}
