pub mod bars;
pub mod format;
pub mod json;
pub mod table;
pub mod tree;

use crate::scanner::{ScanResult, SortKey};
use crate::util::path::display_path;
use crate::util::timing::format_duration;
use crate::util::units::{format_bytes, format_count};

#[derive(Debug, Clone)]
pub struct ReportOptions {
    pub depth: usize,
    pub top: usize,
    pub include_files: bool,
    pub summarize: bool,
    pub dirs_only: bool,
    pub sort_key: SortKey,
    pub show_errors: bool,
}

pub fn render_report(scan: &ScanResult, options: &ReportOptions) -> String {
    let mut out = String::new();
    out.push_str(&format!("Target:      {}\n", display_path(&scan.root.path)));
    out.push_str(&format!(
        "Used:        {}\n",
        format_bytes(scan.root.used_bytes)
    ));
    out.push_str(&format!(
        "Files:       {}\n",
        format_count(scan.root.file_count)
    ));
    out.push_str(&format!(
        "Dirs:        {}\n",
        format_count(scan.root.dir_count)
    ));
    out.push_str(&format!(
        "Errors:      {}\n",
        format_count(scan.metrics.errors_seen)
    ));
    out.push_str(&format!(
        "Elapsed:     {}\n",
        format_duration(scan.metrics.elapsed)
    ));
    out.push_str(&format!(
        "Throughput:  {} entries/s\n",
        format_count(scan.metrics.entries_per_second())
    ));

    if !options.summarize {
        out.push('\n');
        out.push_str("Top entries\n");
        out.push_str(&table::render_top_entries(&scan.root, options));

        if options.include_files {
            out.push_str("\n\nTop files\n");
            out.push_str(&table::render_top_files(
                &scan.top_files,
                scan.root.used_bytes,
                options.top,
            ));
        }

        out.push_str("\n\nTree\n");
        out.push_str(&tree::render_tree(&scan.root, options));
    }

    if scan.metrics.errors_seen > 0 {
        out.push_str("\n\n");
        if options.show_errors {
            out.push_str("Errors\n");
            for error in &scan.root.errors {
                out.push_str(&format!(
                    "- {}: {} ({})\n",
                    display_path(&error.path),
                    error.message,
                    error.kind
                ));
            }
        } else {
            out.push_str("Some entries could not be read. Re-run with --errors to view details.\n");
            out.push_str(
                "For protected macOS locations, grant Full Disk Access to the terminal app if needed.\n",
            );
        }
    }

    out
}
