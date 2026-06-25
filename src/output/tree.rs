use crate::output::bars::usage_bar;
use crate::output::format::sorted_entries;
use crate::output::ReportOptions;
use crate::scanner::{DirSummary, EntrySummary, SortKey};
use crate::util::path::{display_os_str_human, display_path_human};
use crate::util::units::{format_bytes, format_percent};

pub fn render_tree(root: &DirSummary, options: &ReportOptions) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {}\n",
        display_path_human(&root.path),
        format_bytes(root.used_bytes)
    ));

    if options.depth == 0 {
        return out;
    }

    render_children(
        &mut out,
        root,
        root.used_bytes,
        options.depth,
        "",
        options.sort_key,
        options.include_files,
    );
    out
}

fn render_children(
    out: &mut String,
    dir: &DirSummary,
    total: u64,
    depth: usize,
    prefix: &str,
    sort_key: SortKey,
    include_files: bool,
) {
    if depth == 0 {
        return;
    }

    let rows: Vec<&EntrySummary> = sorted_entries(&dir.children, sort_key, false)
        .into_iter()
        .filter(|entry| include_files || entry.is_dir())
        .collect();

    for (idx, entry) in rows.iter().enumerate() {
        let last = idx == rows.len() - 1;
        let connector = if last { "└──" } else { "├──" };
        out.push_str(&format!(
            "{}{} {} {}  {}  {}\n",
            prefix,
            connector,
            display_os_str_human(entry.name()),
            format_bytes(entry.used_bytes()),
            format_percent(entry.used_bytes(), total),
            usage_bar(entry.used_bytes(), total, 16).trim_end()
        ));

        if let Some(child_dir) = entry.as_dir() {
            let child_prefix = format!("{}{}", prefix, if last { "    " } else { "│   " });
            render_children(
                out,
                child_dir,
                total,
                depth - 1,
                &child_prefix,
                sort_key,
                include_files,
            );
        }
    }
}
