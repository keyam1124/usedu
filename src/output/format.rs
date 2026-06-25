use crate::scanner::{EntrySummary, SortKey};
use crate::util::path::display_os_str_human;

pub fn sorted_entries(
    entries: &[EntrySummary],
    sort_key: SortKey,
    dirs_only: bool,
) -> Vec<&EntrySummary> {
    let mut rows: Vec<&EntrySummary> = entries
        .iter()
        .filter(|entry| !dirs_only || entry.is_dir())
        .collect();

    rows.sort_by(|left, right| match sort_key {
        SortKey::Used => right
            .used_bytes()
            .cmp(&left.used_bytes())
            .then_with(|| entry_name(left).cmp(&entry_name(right))),
        SortKey::Name => entry_name(left)
            .cmp(&entry_name(right))
            .then_with(|| right.used_bytes().cmp(&left.used_bytes())),
        SortKey::Files => right
            .file_count()
            .cmp(&left.file_count())
            .then_with(|| right.used_bytes().cmp(&left.used_bytes())),
        SortKey::Dirs => right
            .dir_count()
            .cmp(&left.dir_count())
            .then_with(|| right.used_bytes().cmp(&left.used_bytes())),
    });

    rows
}

pub fn entry_name(entry: &EntrySummary) -> String {
    display_os_str_human(entry.name())
}
