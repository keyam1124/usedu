pub fn usage_bar(part: u64, total: u64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if total == 0 || part == 0 {
        return " ".repeat(width);
    }

    let filled = ((part as f64 / total as f64) * width as f64).round() as usize;
    let filled = filled.clamp(1, width);
    format!("{}{}", "█".repeat(filled), " ".repeat(width - filled))
}
