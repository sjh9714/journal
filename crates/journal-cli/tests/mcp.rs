//! Drive the real binary over stdio with raw JSON-RPC.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

struct Client {
    child: std::process::Child,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Client {
    fn spawn(root: &std::path::Path, extra: &[&str]) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_journal"));
        cmd.arg("--root").arg(root).arg("mcp").arg("serve").args(extra);
        // The server derives new entries' zone from the environment; pin it so the
        // assertions hold on any CI runner.
        cmd.env("TZ", "Asia/Seoul");
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let mut child = cmd.spawn().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut c = Self { child, reader, next_id: 1 };
        let init = c.request("initialize", json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}
        }));
        assert_eq!(init["serverInfo"]["name"], "journal");
        assert!(init["capabilities"]["tools"].is_object());
        c.notify("notifications/initialized", json!({}));
        c
    }
    fn send(&mut self, v: Value) {
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(v.to_string().as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }
    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let mut line = String::new();
            let n = self.reader.read_line(&mut line).unwrap();
            assert!(n > 0, "server closed stdout");
            let v: Value = serde_json::from_str(line.trim()).unwrap();
            if v["id"] == json!(id) {
                if let Some(err) = v.get("error") {
                    return json!({"error": err});
                }
                return v["result"].clone();
            }
        }
    }
    fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        if r.get("error").is_some() {
            return r;
        }
        if r["isError"] == json!(true) {
            return json!({"toolError": r["content"]});
        }
        r["structuredContent"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn fresh_root() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    std::env::set_var("JOURNAL_INDEX_DIR", dir.path().join("idx"));
    let root = dir.path().join("Journal");
    let st = Command::new(env!("CARGO_BIN_EXE_journal")).arg("--root").arg(&root).arg("init").status().unwrap();
    assert!(st.success());
    (dir, root)
}

#[test]
fn tools_roundtrip_over_stdio() {
    let (_d, root) = fresh_root();
    let mut c = Client::spawn(&root, &[]);

    let tools = c.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in ["list_entries", "get_entry", "search_entries", "on_this_day", "entries_on_date", "list_tags", "list_journals", "create_entry", "append_to_today", "update_entry", "delete_entry"] {
        assert!(names.contains(&expected), "missing tool {expected}: {names:?}");
    }
    let create = tools["tools"].as_array().unwrap().iter().find(|t| t["name"] == "create_entry").unwrap();
    assert_eq!(create["inputSchema"]["required"], json!(["body"]));

    let made = c.call("create_entry", json!({"body": "# From an agent\n\nHello journal", "tags": ["agent"], "created": "2025-10-28T10:00:00+09:00"}));
    assert_eq!(made["title"], "From an agent");
    assert_eq!(made["date"], "2025-10-28");
    let uuid = made["uuid"].as_str().unwrap().to_string();

    let got = c.call("get_entry", json!({"id": &uuid[..8]}));
    assert_eq!(got["body"], "# From an agent\n\nHello journal\n");
    assert_eq!(got["timezone"], "Asia/Seoul");

    let hits = c.call("search_entries", json!({"query": "hello"}));
    assert_eq!(hits.as_array().unwrap().len(), 1);

    let otd = c.call("on_this_day", json!({"date": "2026-10-28"}));
    assert_eq!(otd[0]["uuid"], uuid);

    let appended = c.call("append_to_today", json!({"text": "first line today"}));
    let today_uuid = appended["uuid"].as_str().unwrap().to_string();
    let appended2 = c.call("append_to_today", json!({"text": "second line today"}));
    assert_eq!(appended2["uuid"], today_uuid, "same entry reused");
    assert!(appended2["body"].as_str().unwrap().contains("second line today"));

    let upd = c.call("update_entry", json!({"id": uuid, "starred": true, "tags": ["agent", "edited"]}));
    assert_eq!(upd["starred"], true);
    let tags = c.call("list_tags", json!({}));
    assert_eq!(tags.as_array().unwrap().len(), 2);

    let listed = c.call("list_entries", json!({"starred": true}));
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let bad = c.call("get_entry", json!({"id": "zzzz"}));
    assert!(bad.get("error").is_some() || bad.get("toolError").is_some(), "unknown id must error: {bad}");

    let del = c.call("delete_entry", json!({"id": today_uuid}));
    assert_eq!(del["uuid"], today_uuid);
    assert!(!root.join(del["path"].as_str().unwrap()).exists());
}

#[test]
fn read_only_blocks_writes() {
    let (_d, root) = fresh_root();
    let mut c = Client::spawn(&root, &["--read-only"]);
    let r = c.call("create_entry", json!({"body": "nope"}));
    assert!(r.get("error").is_some() || r.get("toolError").is_some(), "{r}");
    let listed = c.call("list_entries", json!({}));
    assert_eq!(listed, json!([]));
}
