use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use usedu_core::scanner::{
    DirSummary, EntryCounts as ScannerCounts, EntrySummary, FileSummary, ScanErrorRecord,
    ScanResult, SortKey,
};
use usedu_core::util::path::display_path;

pub const SCAN_SCHEMA_VERSION: &str = "usedu.scan.v2";
pub const DIFF_SCHEMA_VERSION: &str = "usedu.diff.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeMode {
    Report,
    Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolSort {
    Used,
    Name,
    Files,
    Dirs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanEnvelope {
    pub schema_version: String,
    pub scan_id: String,
    pub status: ScanStatusDto,
    pub semantics: SemanticsDto,
    pub effective_options: EffectiveOptionsDto,
    pub root: EntryDto,
    pub entries: Vec<EntryDto>,
    pub top_files: Vec<TopFileDto>,
    pub issue_summary: IssueSummaryDto,
    pub issues: Vec<IssueDto>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatusDto {
    pub state: ScanStateDto,
    pub partial_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScanStateDto {
    Complete,
    Partial,
    Cancelled,
    LimitReached,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SemanticsDto {
    pub size_metric: String,
    pub accounting_source: String,
    pub accuracy: String,
    pub hard_link_policy: String,
    pub filesystem_boundary_policy: String,
    pub symlink_policy: String,
    pub directory_own_bytes_included: bool,
    pub reclaimable_bytes_known: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveOptionsDto {
    pub mode: String,
    pub depth: usize,
    pub top: usize,
    pub include_files: bool,
    pub summarize: bool,
    pub dirs_only: bool,
    pub sort: ProtocolSort,
    pub show_errors: bool,
    pub fast: bool,
    pub cross_file_systems: bool,
    pub jobs: Option<usize>,
    pub max_output_entries: Option<usize>,
    pub redact_paths: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDto {
    pub entry_id: String,
    pub parent_entry_id: Option<String>,
    pub kind: EntryKindDto,
    pub display_name: String,
    pub display_path: String,
    pub path_ref: PathRefDto,
    pub used_bytes: u64,
    pub own_bytes: u64,
    pub unique_bytes: Option<u64>,
    pub shared_bytes: Option<u64>,
    pub counts: EntryCountsDto,
    pub complete: bool,
    pub issue_count_below: u64,
    pub skipped_count_below: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EntryKindDto {
    Directory,
    RegularFile,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryCountsDto {
    pub regular_files: u64,
    pub directories: u64,
    pub symlinks: u64,
    pub other: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct PathRefDto {
    pub encoding: String,
    pub bytes_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopFileDto {
    pub entry_id: String,
    pub display_name: String,
    pub display_path: String,
    pub path_ref: PathRefDto,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueSummaryDto {
    pub total: u64,
    pub errors: u64,
    pub warnings: u64,
    pub skipped: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub code: String,
    pub severity: String,
    pub entry_id: Option<String>,
    pub display_path: String,
    pub path_ref: PathRefDto,
    pub raw_os_error: Option<i32>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct EnvelopeOptions {
    pub mode: EnvelopeMode,
    pub depth: usize,
    pub top: usize,
    pub include_files: bool,
    pub summarize: bool,
    pub dirs_only: bool,
    pub sort_key: SortKey,
    pub show_errors: bool,
    pub fast: bool,
    pub cross_file_systems: bool,
    pub jobs: Option<usize>,
    pub max_output_entries: Option<usize>,
    pub redact_paths: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEnvelope {
    pub schema_version: String,
    pub status: DiffStatusDto,
    pub before_scan_id: String,
    pub after_scan_id: String,
    pub summary: DiffSummaryDto,
    pub changes: Vec<DiffEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiffStatusDto {
    pub exact: bool,
    pub uncertain_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummaryDto {
    pub added: u64,
    pub removed: u64,
    pub grown: u64,
    pub shrunk: u64,
    pub unchanged: u64,
    pub uncertain: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntryDto {
    pub change: String,
    pub before: Option<DiffEntrySideDto>,
    pub after: Option<DiffEntrySideDto>,
    pub used_bytes_delta: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntrySideDto {
    pub entry_id: String,
    pub kind: EntryKindDto,
    pub display_path: String,
    pub path_ref: PathRefDto,
    pub used_bytes: u64,
}

pub fn build_scan_envelope(scan: &ScanResult, options: &EnvelopeOptions) -> ScanEnvelope {
    let scan_id = scan_id(scan);
    let root = dir_entry(&scan.root, None, options);
    let mut entries = Vec::new();
    let mut next_cursor = None;
    let mut limit_reached = false;

    if !options.summarize {
        let before_limit = entries.len();
        collect_entries(
            &scan.root,
            &root.entry_id,
            options,
            options.depth,
            &mut entries,
        );
        if options.mode == EnvelopeMode::Report
            && options.top > 0
            && selected_children(&scan.root.children, options).len() > options.top
        {
            next_cursor = Some(cursor_for_offset(options.top));
        }
        if let Some(max_entries) = options.max_output_entries {
            if entries.len() > max_entries {
                entries.truncate(max_entries);
                next_cursor = Some(cursor_for_offset(before_limit.saturating_add(max_entries)));
                limit_reached = true;
            }
        }
    }

    let issues = if options.show_errors {
        scan.root
            .errors
            .iter()
            .map(|issue| issue_dto(issue, options))
            .collect()
    } else {
        Vec::new()
    };
    let skipped = scan
        .root
        .errors
        .iter()
        .filter(|error| is_skip_issue(error))
        .count() as u64;
    let issue_total = scan.root.errors.len() as u64;

    ScanEnvelope {
        schema_version: SCAN_SCHEMA_VERSION.to_string(),
        scan_id,
        status: ScanStatusDto {
            state: if limit_reached {
                ScanStateDto::LimitReached
            } else if issue_total == 0 {
                ScanStateDto::Complete
            } else {
                ScanStateDto::Partial
            },
            partial_reasons: partial_reasons(issue_total, limit_reached),
        },
        semantics: semantics(options),
        effective_options: effective_options(options),
        root,
        entries,
        top_files: if options.include_files {
            scan.top_files
                .iter()
                .take(options.top)
                .map(|file| top_file_dto(file, options))
                .collect()
        } else {
            Vec::new()
        },
        issue_summary: IssueSummaryDto {
            total: issue_total,
            errors: issue_total.saturating_sub(skipped),
            warnings: skipped,
            skipped,
        },
        issues,
        next_cursor,
    }
}

fn partial_reasons(issue_total: u64, limit_reached: bool) -> Vec<String> {
    let mut reasons = Vec::new();
    if issue_total > 0 {
        reasons.push("issuesRecorded".to_string());
    }
    if limit_reached {
        reasons.push("maxOutputEntries".to_string());
    }
    reasons
}

pub fn render_json_v2(scan: &ScanResult, options: &EnvelopeOptions) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(&build_scan_envelope(
        scan, options,
    ))?)
}

pub fn render_ndjson(scan: &ScanResult, options: &EnvelopeOptions) -> anyhow::Result<String> {
    let envelope = build_scan_envelope(scan, options);
    let mut lines = Vec::new();
    lines.push(serde_json::to_string(&json!({
        "schemaVersion": SCAN_SCHEMA_VERSION,
        "event": "scanStarted",
        "scanId": envelope.scan_id,
        "root": envelope.root,
        "semantics": envelope.semantics,
        "effectiveOptions": envelope.effective_options,
    }))?);
    for entry in envelope.entries {
        lines.push(serde_json::to_string(&json!({
            "schemaVersion": SCAN_SCHEMA_VERSION,
            "event": "entry",
            "scanId": envelope.scan_id,
            "entry": entry,
        }))?);
    }
    for issue in envelope.issues {
        lines.push(serde_json::to_string(&json!({
            "schemaVersion": SCAN_SCHEMA_VERSION,
            "event": "issue",
            "scanId": envelope.scan_id,
            "issue": issue,
        }))?);
    }
    lines.push(serde_json::to_string(&json!({
        "schemaVersion": SCAN_SCHEMA_VERSION,
        "event": "scanCompleted",
        "scanId": envelope.scan_id,
        "status": envelope.status,
        "issueSummary": envelope.issue_summary,
        "nextCursor": envelope.next_cursor,
    }))?);
    Ok(lines.join("\n"))
}

pub fn json_v2_schema() -> &'static str {
    include_str!("../../../schemas/usedu-json-v2.schema.json")
}

pub fn diff_snapshots(before: &ScanEnvelope, after: &ScanEnvelope) -> DiffEnvelope {
    let mut reasons = Vec::new();
    if before.status.state != ScanStateDto::Complete || after.status.state != ScanStateDto::Complete
    {
        reasons.push("partialInput".to_string());
    }
    if before.semantics != after.semantics {
        reasons.push("differentSemantics".to_string());
    }

    let exact = reasons.is_empty();
    let before_entries = comparable_entries(before);
    let after_entries = comparable_entries(after);
    let mut changes = Vec::new();
    let mut summary = DiffSummaryDto {
        added: 0,
        removed: 0,
        grown: 0,
        shrunk: 0,
        unchanged: 0,
        uncertain: 0,
    };

    for (path_ref, before_entry) in &before_entries {
        match after_entries.get(path_ref) {
            Some(after_entry) => {
                let delta = after_entry.used_bytes as i128 - before_entry.used_bytes as i128;
                let change = if !exact {
                    summary.uncertain += 1;
                    "uncertain"
                } else if delta > 0 {
                    summary.grown += 1;
                    "grown"
                } else if delta < 0 {
                    summary.shrunk += 1;
                    "shrunk"
                } else {
                    summary.unchanged += 1;
                    "unchanged"
                };
                changes.push(DiffEntryDto {
                    change: change.to_string(),
                    before: Some(diff_side(before_entry)),
                    after: Some(diff_side(after_entry)),
                    used_bytes_delta: clamp_i128_to_i64(delta),
                });
            }
            None => {
                let change = if exact {
                    summary.removed += 1;
                    "removed"
                } else {
                    summary.uncertain += 1;
                    "uncertain"
                };
                changes.push(DiffEntryDto {
                    change: change.to_string(),
                    before: Some(diff_side(before_entry)),
                    after: None,
                    used_bytes_delta: -(before_entry.used_bytes.min(i64::MAX as u64) as i64),
                });
            }
        }
    }

    for (path_ref, after_entry) in &after_entries {
        if before_entries.contains_key(path_ref) {
            continue;
        }
        let change = if exact {
            summary.added += 1;
            "added"
        } else {
            summary.uncertain += 1;
            "uncertain"
        };
        changes.push(DiffEntryDto {
            change: change.to_string(),
            before: None,
            after: Some(diff_side(after_entry)),
            used_bytes_delta: after_entry.used_bytes.min(i64::MAX as u64) as i64,
        });
    }

    changes.sort_by(|left, right| {
        side_path_ref(left)
            .cmp(&side_path_ref(right))
            .then_with(|| left.change.cmp(&right.change))
    });

    DiffEnvelope {
        schema_version: DIFF_SCHEMA_VERSION.to_string(),
        status: DiffStatusDto {
            exact,
            uncertain_reasons: reasons,
        },
        before_scan_id: before.scan_id.clone(),
        after_scan_id: after.scan_id.clone(),
        summary,
        changes,
    }
}

fn collect_entries(
    dir: &DirSummary,
    parent_id: &str,
    options: &EnvelopeOptions,
    depth_remaining: usize,
    out: &mut Vec<EntryDto>,
) {
    if depth_remaining == 0 {
        return;
    }
    let mut children = selected_children(&dir.children, options);
    if options.mode == EnvelopeMode::Report && options.top > 0 && children.len() > options.top {
        children.truncate(options.top);
    }

    for child in children {
        let entry = entry_dto(child, Some(parent_id.to_string()), options);
        let child_id = entry.entry_id.clone();
        out.push(entry);
        if let Some(child_dir) = child.as_dir() {
            collect_entries(child_dir, &child_id, options, depth_remaining - 1, out);
        }
    }
}

fn selected_children<'a>(
    children: &'a [EntrySummary],
    options: &EnvelopeOptions,
) -> Vec<&'a EntrySummary> {
    let mut entries: Vec<&EntrySummary> = children
        .iter()
        .filter(|entry| {
            (!options.dirs_only || entry.is_dir()) && (options.include_files || entry.is_dir())
        })
        .collect();
    entries.sort_by(|left, right| match options.sort_key {
        SortKey::Used => right
            .used_bytes()
            .cmp(&left.used_bytes())
            .then_with(|| path_bytes(left.path()).cmp(&path_bytes(right.path()))),
        SortKey::Name => path_bytes(left.path())
            .cmp(&path_bytes(right.path()))
            .then_with(|| right.used_bytes().cmp(&left.used_bytes())),
        SortKey::Files => right
            .file_count()
            .cmp(&left.file_count())
            .then_with(|| path_bytes(left.path()).cmp(&path_bytes(right.path()))),
        SortKey::Dirs => right
            .dir_count()
            .cmp(&left.dir_count())
            .then_with(|| path_bytes(left.path()).cmp(&path_bytes(right.path()))),
    });
    entries
}

fn entry_dto(
    entry: &EntrySummary,
    parent_entry_id: Option<String>,
    options: &EnvelopeOptions,
) -> EntryDto {
    match entry {
        EntrySummary::Dir(dir) => dir_entry(dir, parent_entry_id, options),
        EntrySummary::File(file) => {
            leaf_entry(file, EntryKindDto::RegularFile, parent_entry_id, options)
        }
        EntrySummary::Symlink(file) => {
            leaf_entry(file, EntryKindDto::Symlink, parent_entry_id, options)
        }
        EntrySummary::Other(file) => {
            leaf_entry(file, EntryKindDto::Other, parent_entry_id, options)
        }
    }
}

fn dir_entry(
    dir: &DirSummary,
    parent_entry_id: Option<String>,
    options: &EnvelopeOptions,
) -> EntryDto {
    EntryDto {
        entry_id: entry_id(&dir.path),
        parent_entry_id,
        kind: EntryKindDto::Directory,
        display_name: display_name(dir.name.to_string_lossy().as_ref(), options),
        display_path: display_path_field(&dir.path, options),
        path_ref: path_ref(&dir.path),
        used_bytes: dir.used_bytes,
        own_bytes: dir.own_bytes,
        unique_bytes: None,
        shared_bytes: None,
        counts: counts_dto(dir.counts),
        complete: dir.errors.is_empty(),
        issue_count_below: dir.errors.len() as u64,
        skipped_count_below: dir
            .errors
            .iter()
            .filter(|error| is_skip_issue(error))
            .count() as u64,
    }
}

fn leaf_entry(
    file: &FileSummary,
    kind: EntryKindDto,
    parent_entry_id: Option<String>,
    options: &EnvelopeOptions,
) -> EntryDto {
    EntryDto {
        entry_id: entry_id(&file.path),
        parent_entry_id,
        kind: kind.clone(),
        display_name: display_name(file.name.to_string_lossy().as_ref(), options),
        display_path: display_path_field(&file.path, options),
        path_ref: path_ref(&file.path),
        used_bytes: file.used_bytes,
        own_bytes: file.used_bytes,
        unique_bytes: None,
        shared_bytes: None,
        counts: match kind {
            EntryKindDto::RegularFile => counts_dto(ScannerCounts::regular_file()),
            EntryKindDto::Symlink => counts_dto(ScannerCounts::symlink()),
            EntryKindDto::Other => counts_dto(ScannerCounts::other()),
            EntryKindDto::Directory => counts_dto(ScannerCounts::directory()),
        },
        complete: true,
        issue_count_below: 0,
        skipped_count_below: 0,
    }
}

fn top_file_dto(file: &FileSummary, options: &EnvelopeOptions) -> TopFileDto {
    TopFileDto {
        entry_id: entry_id(&file.path),
        display_name: display_name(file.name.to_string_lossy().as_ref(), options),
        display_path: display_path_field(&file.path, options),
        path_ref: path_ref(&file.path),
        used_bytes: file.used_bytes,
    }
}

fn issue_dto(error: &ScanErrorRecord, options: &EnvelopeOptions) -> IssueDto {
    IssueDto {
        code: issue_code(error).to_string(),
        severity: if is_skip_issue(error) {
            "warning".to_string()
        } else {
            "error".to_string()
        },
        entry_id: Some(entry_id(&error.path)),
        display_path: display_path_field(&error.path, options),
        path_ref: path_ref(&error.path),
        raw_os_error: raw_os_error(error),
        message: error.message.clone(),
    }
}

fn issue_code(error: &ScanErrorRecord) -> &'static str {
    match error.kind.as_str() {
        "PermissionDenied" => "PERMISSION_DENIED",
        "NotFound" => "NOT_FOUND_DURING_SCAN",
        "cross_file_system" => "CROSS_FILESYSTEM_SKIPPED",
        "InvalidInput" => "INVALID_PATH",
        _ => "UNSUPPORTED_FILE_TYPE",
    }
}

fn is_skip_issue(error: &ScanErrorRecord) -> bool {
    error.kind == "cross_file_system"
}

fn raw_os_error(error: &ScanErrorRecord) -> Option<i32> {
    error
        .message
        .rsplit_once("os error ")
        .and_then(|(_, suffix)| suffix.trim_end_matches(')').parse().ok())
}

fn counts_dto(counts: ScannerCounts) -> EntryCountsDto {
    EntryCountsDto {
        regular_files: counts.regular_files,
        directories: counts.directories,
        symlinks: counts.symlinks,
        other: counts.other,
    }
}

fn semantics(options: &EnvelopeOptions) -> SemanticsDto {
    SemanticsDto {
        size_metric: "allocated".to_string(),
        accounting_source: if options.fast {
            "getattrlistbulkAllocSize".to_string()
        } else {
            "unixBlocks512".to_string()
        },
        accuracy: if options.fast {
            "approximate".to_string()
        } else {
            "strict".to_string()
        },
        hard_link_policy: if options.fast {
            "mayDoubleCount".to_string()
        } else {
            "firstSeenDeviceInode".to_string()
        },
        filesystem_boundary_policy: if options.cross_file_systems {
            "includeMountedFilesystems".to_string()
        } else {
            "stayOnRootFilesystem".to_string()
        },
        symlink_policy: "countLinkDoNotFollow".to_string(),
        directory_own_bytes_included: !options.fast,
        reclaimable_bytes_known: false,
    }
}

fn effective_options(options: &EnvelopeOptions) -> EffectiveOptionsDto {
    EffectiveOptionsDto {
        mode: match options.mode {
            EnvelopeMode::Report => "report".to_string(),
            EnvelopeMode::Snapshot => "snapshot".to_string(),
        },
        depth: options.depth,
        top: options.top,
        include_files: options.include_files,
        summarize: options.summarize,
        dirs_only: options.dirs_only,
        sort: match options.sort_key {
            SortKey::Used => ProtocolSort::Used,
            SortKey::Name => ProtocolSort::Name,
            SortKey::Files => ProtocolSort::Files,
            SortKey::Dirs => ProtocolSort::Dirs,
        },
        show_errors: options.show_errors,
        fast: options.fast,
        cross_file_systems: options.cross_file_systems,
        jobs: options.jobs,
        max_output_entries: options.max_output_entries,
        redact_paths: options.redact_paths,
    }
}

fn display_name(name: &str, options: &EnvelopeOptions) -> String {
    if options.redact_paths {
        "[redacted]".to_string()
    } else {
        name.to_string()
    }
}

fn display_path_field(path: &Path, options: &EnvelopeOptions) -> String {
    if options.redact_paths {
        "[redacted]".to_string()
    } else {
        display_path(path)
    }
}

fn comparable_entries(envelope: &ScanEnvelope) -> BTreeMap<PathRefDto, EntryDto> {
    std::iter::once(envelope.root.clone())
        .chain(envelope.entries.iter().cloned())
        .map(|entry| (entry.path_ref.clone(), entry))
        .collect()
}

fn diff_side(entry: &EntryDto) -> DiffEntrySideDto {
    DiffEntrySideDto {
        entry_id: entry.entry_id.clone(),
        kind: entry.kind.clone(),
        display_path: entry.display_path.clone(),
        path_ref: entry.path_ref.clone(),
        used_bytes: entry.used_bytes,
    }
}

fn side_path_ref(entry: &DiffEntryDto) -> PathRefDto {
    entry
        .before
        .as_ref()
        .map(|side| side.path_ref.clone())
        .or_else(|| entry.after.as_ref().map(|side| side.path_ref.clone()))
        .expect("diff entry must have before or after side")
}

fn scan_id(scan: &ScanResult) -> String {
    let mut bytes = path_bytes(&scan.root.path);
    bytes.extend_from_slice(&scan.root.used_bytes.to_le_bytes());
    bytes.extend_from_slice(&scan.root.file_count.to_le_bytes());
    bytes.extend_from_slice(&scan.root.dir_count.to_le_bytes());
    format!("scan_{:016x}", fnv1a64(&bytes))
}

fn entry_id(path: &Path) -> String {
    format!("entry_{:016x}", fnv1a64(&path_bytes(path)))
}

fn path_ref(path: &Path) -> PathRefDto {
    PathRefDto {
        encoding: "unixPathBytesHex".to_string(),
        bytes_hex: hex(&path_bytes(path)),
    }
}

fn path_bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().as_bytes().to_vec()
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn cursor_for_offset(offset: usize) -> String {
    format!("cursor:offset:{offset}")
}

fn clamp_i128_to_i64(value: i128) -> i64 {
    value.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}
