use crate::protocol::{
    build_scan_envelope, diff_snapshots, EntryDto, EnvelopeMode, EnvelopeOptions, ScanEnvelope,
    SCAN_SCHEMA_VERSION,
};
use crate::scanner::{scan_recursive, ScanOptions, SortKey};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const JSONRPC_VERSION: &str = "2.0";
const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(30 * 60);
const DEFAULT_MAX_SESSIONS: usize = 8;
const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 500;

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub allowed_roots: Vec<PathBuf>,
    pub max_sessions: usize,
    pub session_ttl: Duration,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            allowed_roots: Vec::new(),
            max_sessions: DEFAULT_MAX_SESSIONS,
            session_ttl: DEFAULT_SESSION_TTL,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug)]
struct Session {
    envelope: ScanEnvelope,
    updated_at: Instant,
}

pub fn run_stdio(config: McpServerConfig) -> Result<()> {
    let mut server = McpServer::new(config)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match server.handle_line(&line) {
            Ok(Some(response)) => {
                writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
                stdout.flush()?;
            }
            Ok(None) => {}
            Err(error) => {
                let fallback = error_response(None, -32603, error.to_string());
                writeln!(stdout, "{}", serde_json::to_string(&fallback)?)?;
                stdout.flush()?;
            }
        }
    }

    Ok(())
}

struct McpServer {
    allowed_roots: Vec<PathBuf>,
    max_sessions: usize,
    session_ttl: Duration,
    sessions: HashMap<String, Session>,
}

impl McpServer {
    fn new(config: McpServerConfig) -> Result<Self> {
        let allowed_roots = if config.allowed_roots.is_empty() {
            vec![std::env::current_dir()?.canonicalize()?]
        } else {
            config
                .allowed_roots
                .iter()
                .map(|root| {
                    root.canonicalize()
                        .with_context(|| format!("failed to canonicalize {}", root.display()))
                })
                .collect::<Result<Vec<_>>>()?
        };
        Ok(Self {
            allowed_roots,
            max_sessions: config.max_sessions.max(1),
            session_ttl: config.session_ttl,
            sessions: HashMap::new(),
        })
    }

    fn handle_line(&mut self, line: &str) -> Result<Option<Value>> {
        self.prune_expired_sessions();
        let request: RpcRequest = serde_json::from_str(line)?;
        let Some(id) = request.id.clone() else {
            return Ok(None);
        };
        let result = match request.method.as_str() {
            "initialize" => self.initialize_result(),
            "tools/list" => self.tools_list_result(),
            "tools/call" => self.call_tool(request.params),
            _ => return Ok(Some(error_response(Some(id), -32601, "method not found"))),
        };
        match result {
            Ok(result) => Ok(Some(success_response(id, result))),
            Err(error) => Ok(Some(error_response(Some(id), -32602, error.to_string()))),
        }
    }

    fn initialize_result(&self) -> Result<Value> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "usedu-mcp",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {}
            }
        }))
    }

    fn tools_list_result(&self) -> Result<Value> {
        Ok(json!({
            "tools": [
                tool_schema(
                    "usedu_scan",
                    "Scan an allowed root and return a versioned usedu scan envelope.",
                    json!({
                        "type": "object",
                        "required": ["root"],
                        "properties": {
                            "root": { "type": "string" },
                            "depth": { "type": "integer", "minimum": 0, "default": 1 },
                            "top": { "type": "integer", "minimum": 0, "default": 30 },
                            "includeFiles": { "type": "boolean", "default": false },
                            "dirsOnly": { "type": "boolean", "default": false },
                            "sort": { "enum": ["used", "name", "files", "dirs"], "default": "used" },
                            "fast": { "type": "boolean", "default": false },
                            "crossFileSystems": { "type": "boolean", "default": false },
                            "maxOutputEntries": { "type": "integer", "minimum": 0 }
                            , "redactPaths": { "type": "boolean", "default": false }
                        }
                    })
                ),
                tool_schema(
                    "usedu_list_children",
                    "List direct children from a stored scan envelope with cursor pagination.",
                    json!({
                        "type": "object",
                        "required": ["scanId", "entryId"],
                        "properties": {
                            "scanId": { "type": "string" },
                            "entryId": { "type": "string" },
                            "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 50 },
                            "cursor": { "type": "string" }
                        }
                    })
                ),
                tool_schema(
                    "usedu_top_entries",
                    "Return top entries from a stored scan envelope.",
                    json!({
                        "type": "object",
                        "required": ["scanId"],
                        "properties": {
                            "scanId": { "type": "string" },
                            "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 50 },
                            "kind": { "enum": ["directory", "regularFile", "symlink", "other"] },
                            "minUsedBytes": { "type": "integer", "minimum": 0 }
                        }
                    })
                ),
                tool_schema(
                    "usedu_get_issues",
                    "Return scan issues from a stored scan envelope with cursor pagination.",
                    json!({
                        "type": "object",
                        "required": ["scanId"],
                        "properties": {
                            "scanId": { "type": "string" },
                            "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 50 },
                            "cursor": { "type": "string" }
                        }
                    })
                ),
                tool_schema(
                    "usedu_compare",
                    "Compare two stored scan envelopes.",
                    json!({
                        "type": "object",
                        "required": ["beforeScanId", "afterScanId"],
                        "properties": {
                            "beforeScanId": { "type": "string" },
                            "afterScanId": { "type": "string" }
                        }
                    })
                ),
                tool_schema(
                    "usedu_close_scan",
                    "Close and remove a stored scan envelope.",
                    json!({
                        "type": "object",
                        "required": ["scanId"],
                        "properties": {
                            "scanId": { "type": "string" }
                        }
                    })
                )
            ]
        }))
    }

    fn call_tool(&mut self, params: Value) -> Result<Value> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing tool name"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let structured = match name {
            "usedu_scan" => self.usedu_scan(arguments)?,
            "usedu_list_children" => self.usedu_list_children(arguments)?,
            "usedu_top_entries" => self.usedu_top_entries(arguments)?,
            "usedu_get_issues" => self.usedu_get_issues(arguments)?,
            "usedu_compare" => self.usedu_compare(arguments)?,
            "usedu_close_scan" => self.usedu_close_scan(arguments)?,
            _ => bail!("unknown tool: {name}"),
        };
        Ok(json!({
            "content": [
                {
                    "type": "text",
                    "text": serde_json::to_string(&structured)?
                }
            ],
            "structuredContent": structured
        }))
    }

    fn usedu_scan(&mut self, arguments: Value) -> Result<Value> {
        let root_arg = required_string(&arguments, "root")?;
        let root = self.ensure_allowed_root(Path::new(root_arg))?;
        let depth = optional_usize(&arguments, "depth")?.unwrap_or(1);
        let top = optional_usize(&arguments, "top")?.unwrap_or(30);
        let include_files = optional_bool(&arguments, "includeFiles").unwrap_or(false);
        let dirs_only = optional_bool(&arguments, "dirsOnly").unwrap_or(false);
        let fast = optional_bool(&arguments, "fast").unwrap_or(false);
        let cross_file_systems = optional_bool(&arguments, "crossFileSystems").unwrap_or(false);
        let sort_key = optional_sort_key(&arguments)?.unwrap_or(SortKey::Used);
        let max_output_entries = optional_usize(&arguments, "maxOutputEntries")?;
        let redact_paths = optional_bool(&arguments, "redactPaths").unwrap_or(false);

        let scan_options = ScanOptions {
            cross_file_systems,
            include_files_in_output: include_files,
            top_files_limit: top,
            retained_tree_depth: depth,
            retain_root_children: true,
            fast,
            ..Default::default()
        };

        let scan = scan_recursive(root, &scan_options)?;
        let envelope = build_scan_envelope(
            &scan,
            &EnvelopeOptions {
                mode: EnvelopeMode::Snapshot,
                depth,
                top,
                include_files,
                summarize: false,
                dirs_only,
                sort_key,
                show_errors: true,
                fast,
                cross_file_systems,
                jobs: scan_options.jobs,
                max_output_entries,
                redact_paths,
            },
        );
        let scan_id = envelope.scan_id.clone();
        self.insert_session(envelope.clone());

        Ok(json!({
            "scanId": scan_id,
            "schemaVersion": SCAN_SCHEMA_VERSION,
            "envelope": envelope
        }))
    }

    fn usedu_list_children(&mut self, arguments: Value) -> Result<Value> {
        let scan_id = required_string(&arguments, "scanId")?;
        let entry_id = required_string(&arguments, "entryId")?;
        let limit = page_limit(&arguments)?;
        let offset = cursor_offset(arguments.get("cursor").and_then(Value::as_str))?;
        let envelope = self.session(scan_id)?;
        let mut rows: Vec<EntryDto> = envelope
            .entries
            .iter()
            .filter(|entry| entry.parent_entry_id.as_deref() == Some(entry_id))
            .cloned()
            .collect();
        rows.sort_by_key(|entry| Reverse(entry.used_bytes));
        let (items, next_cursor) = page(rows, offset, limit);
        Ok(json!({
            "scanId": scan_id,
            "entryId": entry_id,
            "items": items,
            "nextCursor": next_cursor
        }))
    }

    fn usedu_top_entries(&mut self, arguments: Value) -> Result<Value> {
        let scan_id = required_string(&arguments, "scanId")?;
        let limit = page_limit(&arguments)?;
        let kind = arguments.get("kind").and_then(Value::as_str);
        let min_used_bytes = optional_u64(&arguments, "minUsedBytes")?.unwrap_or(0);
        let envelope = self.session(scan_id)?;
        let mut rows: Vec<EntryDto> = envelope
            .entries
            .iter()
            .filter(|entry| kind.is_none_or(|kind| serde_kind(entry) == kind))
            .filter(|entry| entry.used_bytes >= min_used_bytes)
            .cloned()
            .collect();
        rows.sort_by_key(|entry| Reverse(entry.used_bytes));
        rows.truncate(limit);
        Ok(json!({
            "scanId": scan_id,
            "items": rows
        }))
    }

    fn usedu_get_issues(&mut self, arguments: Value) -> Result<Value> {
        let scan_id = required_string(&arguments, "scanId")?;
        let limit = page_limit(&arguments)?;
        let offset = cursor_offset(arguments.get("cursor").and_then(Value::as_str))?;
        let envelope = self.session(scan_id)?;
        let (items, next_cursor) = page(envelope.issues.clone(), offset, limit);
        Ok(json!({
            "scanId": scan_id,
            "items": items,
            "nextCursor": next_cursor
        }))
    }

    fn usedu_compare(&mut self, arguments: Value) -> Result<Value> {
        let before_scan_id = required_string(&arguments, "beforeScanId")?;
        let after_scan_id = required_string(&arguments, "afterScanId")?;
        let before = self.session(before_scan_id)?.clone();
        let after = self.session(after_scan_id)?.clone();
        Ok(json!(diff_snapshots(&before, &after)))
    }

    fn usedu_close_scan(&mut self, arguments: Value) -> Result<Value> {
        let scan_id = required_string(&arguments, "scanId")?;
        let removed = self.sessions.remove(scan_id).is_some();
        Ok(json!({
            "scanId": scan_id,
            "closed": removed
        }))
    }

    fn ensure_allowed_root(&self, path: &Path) -> Result<PathBuf> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", path.display()))?;
        if self
            .allowed_roots
            .iter()
            .any(|allowed| canonical.starts_with(allowed))
        {
            return Ok(canonical);
        }
        bail!("path is outside the MCP allowlist: {}", path.display())
    }

    fn insert_session(&mut self, envelope: ScanEnvelope) {
        while self.sessions.len() >= self.max_sessions {
            let Some(oldest) = self
                .sessions
                .iter()
                .min_by_key(|(_, session)| session.updated_at)
                .map(|(scan_id, _)| scan_id.clone())
            else {
                break;
            };
            self.sessions.remove(&oldest);
        }
        self.sessions.insert(
            envelope.scan_id.clone(),
            Session {
                envelope,
                updated_at: Instant::now(),
            },
        );
    }

    fn session(&mut self, scan_id: &str) -> Result<&ScanEnvelope> {
        let session = self
            .sessions
            .get_mut(scan_id)
            .ok_or_else(|| anyhow!("unknown scanId: {scan_id}"))?;
        session.updated_at = Instant::now();
        Ok(&session.envelope)
    }

    fn prune_expired_sessions(&mut self) {
        let now = Instant::now();
        self.sessions
            .retain(|_, session| now.duration_since(session.updated_at) <= self.session_ttl);
    }
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result
    })
}

fn error_response(id: Option<Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn tool_schema(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn required_string<'a>(arguments: &'a Value, field: &str) -> Result<&'a str> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string field: {field}"))
}

fn optional_bool(arguments: &Value, field: &str) -> Option<bool> {
    arguments.get(field).and_then(Value::as_bool)
}

fn optional_usize(arguments: &Value, field: &str) -> Result<Option<usize>> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| anyhow!("invalid usize field: {field}"))
        })
        .transpose()
}

fn optional_u64(arguments: &Value, field: &str) -> Result<Option<u64>> {
    arguments
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| anyhow!("invalid u64 field: {field}"))
        })
        .transpose()
}

fn optional_sort_key(arguments: &Value) -> Result<Option<SortKey>> {
    arguments
        .get("sort")
        .map(|value| match value.as_str() {
            Some("used") => Ok(SortKey::Used),
            Some("name") => Ok(SortKey::Name),
            Some("files") => Ok(SortKey::Files),
            Some("dirs") => Ok(SortKey::Dirs),
            _ => bail!("invalid sort value"),
        })
        .transpose()
}

fn page_limit(arguments: &Value) -> Result<usize> {
    Ok(optional_usize(arguments, "limit")?
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT))
}

fn cursor_offset(cursor: Option<&str>) -> Result<usize> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    cursor
        .strip_prefix("cursor:offset:")
        .ok_or_else(|| anyhow!("invalid cursor"))?
        .parse()
        .map_err(|_| anyhow!("invalid cursor offset"))
}

fn page<T: Clone>(items: Vec<T>, offset: usize, limit: usize) -> (Vec<T>, Option<String>) {
    if offset >= items.len() {
        return (Vec::new(), None);
    }
    let end = offset.saturating_add(limit).min(items.len());
    let next_cursor = (end < items.len()).then(|| format!("cursor:offset:{end}"));
    (items[offset..end].to_vec(), next_cursor)
}

fn serde_kind(entry: &EntryDto) -> &'static str {
    match entry.kind {
        crate::protocol::EntryKindDto::Directory => "directory",
        crate::protocol::EntryKindDto::RegularFile => "regularFile",
        crate::protocol::EntryKindDto::Symlink => "symlink",
        crate::protocol::EntryKindDto::Other => "other",
    }
}
