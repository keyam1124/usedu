use crate::output::bars::usage_bar;
use crate::output::format::{entry_name, sorted_entries};
use crate::output::ReportOptions;
use crate::scanner::{DirSummary, FileSummary};
use crate::util::path::display_path;
use crate::util::units::{format_bytes, format_compact_count, format_percent};
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, ContentArrangement, Table};

pub fn render_top_entries(root: &DirSummary, options: &ReportOptions) -> String {
    let mut table = base_table();
    table.set_header(vec!["#", "Path", "Used", "Share", "Files / Dirs", "Visual"]);

    for (idx, entry) in sorted_entries(&root.children, options.sort_key, options.dirs_only)
        .into_iter()
        .take(options.top)
        .enumerate()
    {
        table.add_row(vec![
            Cell::new((idx + 1).to_string()),
            Cell::new(entry_name(entry)),
            Cell::new(format_bytes(entry.used_bytes())),
            Cell::new(format_percent(entry.used_bytes(), root.used_bytes)),
            Cell::new(format!(
                "{} / {}",
                format_compact_count(entry.file_count()),
                format_compact_count(entry.dir_count())
            )),
            Cell::new(usage_bar(entry.used_bytes(), root.used_bytes, 20)),
        ]);
    }

    if root.children.is_empty() {
        table.add_row(vec![
            Cell::new("-"),
            Cell::new("(empty)"),
            Cell::new("0 B"),
            Cell::new("0.0%"),
            Cell::new("0 / 0"),
            Cell::new(usage_bar(0, 0, 20)),
        ]);
    }

    table.to_string()
}

pub fn render_top_files(files: &[FileSummary], root_used: u64, top: usize) -> String {
    let mut table = base_table();
    table.set_header(vec!["#", "Path", "Used", "Share", "Visual"]);

    for (idx, file) in files.iter().take(top).enumerate() {
        table.add_row(vec![
            Cell::new((idx + 1).to_string()),
            Cell::new(display_path(&file.path)),
            Cell::new(format_bytes(file.used_bytes)),
            Cell::new(format_percent(file.used_bytes, root_used)),
            Cell::new(usage_bar(file.used_bytes, root_used, 20)),
        ]);
    }

    if files.is_empty() {
        table.add_row(vec![
            Cell::new("-"),
            Cell::new("(none)"),
            Cell::new("0 B"),
            Cell::new("0.0%"),
            Cell::new(usage_bar(0, 0, 20)),
        ]);
    }

    table.to_string()
}

fn base_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table
}
