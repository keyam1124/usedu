#![cfg(unix)]

use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn mcp_stdio_scans_lists_and_closes_sessions() {
    let fixture = Fixture::new("mcp-stdio");
    fs::create_dir(fixture.path("child")).unwrap();
    write_file(&fixture.path("child/file.txt"), b"file");

    let mut server = McpProcess::start(fixture.root());
    let initialize = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    }));
    assert_eq!(initialize["result"]["serverInfo"]["name"], "usedu-mcp");

    let tools = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    assert!(tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "usedu_scan"));

    let scan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "usedu_scan",
            "arguments": {
                "root": fixture.root(),
                "depth": 1,
                "includeFiles": true
            }
        }
    }));
    let structured = &scan["result"]["structuredContent"];
    let scan_id = structured["scanId"].as_str().unwrap();
    let root_id = structured["envelope"]["root"]["entryId"].as_str().unwrap();

    let children = server.request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "usedu_list_children",
            "arguments": {
                "scanId": scan_id,
                "entryId": root_id,
                "limit": 10
            }
        }
    }));
    assert_eq!(
        children["result"]["structuredContent"]["items"][0]["displayName"],
        "child"
    );

    let close = server.request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "usedu_close_scan",
            "arguments": { "scanId": scan_id }
        }
    }));
    assert_eq!(close["result"]["structuredContent"]["closed"], true);
}

#[test]
fn mcp_stdio_background_scan_reports_progress_and_can_be_cancelled() {
    let fixture = Fixture::new("mcp-background");
    for index in 0..400 {
        let dir = fixture.path(format!("child-{index:04}"));
        fs::create_dir(&dir).unwrap();
        write_file(&dir.join("file.txt"), b"file");
    }

    let mut server = McpProcess::start(fixture.root());
    let scan = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "usedu_scan",
            "arguments": {
                "root": fixture.root(),
                "background": true,
                "depth": 1,
                "includeFiles": true
            }
        }
    }));
    let scan_id = scan["result"]["structuredContent"]["scanId"]
        .as_str()
        .unwrap();
    assert_eq!(scan["result"]["structuredContent"]["state"], "running");

    let status = server.request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "usedu_scan_status",
            "arguments": { "scanId": scan_id }
        }
    }));
    assert!(
        status["result"]["structuredContent"]["progress"]["entriesSeen"]
            .as_u64()
            .is_some()
    );

    let cancel = server.request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "usedu_cancel_scan",
            "arguments": { "scanId": scan_id }
        }
    }));
    let structured = &cancel["result"]["structuredContent"];
    assert!(structured["cancelRequested"].as_bool().is_some());
    assert!(matches!(
        structured["state"].as_str().unwrap(),
        "running" | "complete" | "cancelled"
    ));
}

#[test]
fn mcp_stdio_rejects_paths_outside_allowlist() {
    let fixture = Fixture::new("mcp-allowlist");
    let mut server = McpProcess::start(fixture.root());

    let response = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "usedu_scan",
            "arguments": {
                "root": "/",
                "depth": 0
            }
        }
    }));

    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("outside the MCP allowlist"));
}

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl McpProcess {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_usedu"))
            .args(["mcp", "--stdio", "--allow-root"])
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn request(&mut self, request: Value) -> Value {
        writeln!(self.stdin, "{request}").unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("usedu-{name}-{nonce}"));
        fs::create_dir(&root).unwrap();
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_file(path: &Path, bytes: &[u8]) {
    let mut file = File::create(path).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
