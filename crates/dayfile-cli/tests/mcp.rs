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
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dayfile"));
        cmd.arg("--root").arg(root).arg("mcp").arg("serve").args(extra);
        // The server derives new entries' zone from the environment; pin it so the
        // assertions hold on any CI runner.
        cmd.env("TZ", "Asia/Seoul");
        // Never the developer's real app config: capture sources come from here.
        cmd.env("DAYFILE_CONFIG", root.parent().unwrap().join("config.json"));
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let mut child = cmd.spawn().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut c = Self { child, reader, next_id: 1 };
        let init = c.request("initialize", json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}
        }));
        assert_eq!(init["serverInfo"]["name"], "dayfile");
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
    std::env::set_var("DAYFILE_INDEX_DIR", dir.path().join("idx"));
    let root = dir.path().join("Journal");
    let st = Command::new(env!("CARGO_BIN_EXE_dayfile")).arg("--root").arg(&root).arg("init").status().unwrap();
    assert!(st.success());
    (dir, root)
}

#[test]
fn tools_roundtrip_over_stdio() {
    let (_d, root) = fresh_root();
    let mut c = Client::spawn(&root, &[]);

    let tools = c.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in ["list_entries", "get_entry", "search_entries", "on_this_day", "entries_on_date", "list_tags", "list_journals", "create_entry", "append_to_today", "update_entry", "delete_entry", "capture_day", "list_drafts", "accept_draft", "remember", "recall", "fact_history", "list_topics", "retract_fact"] {
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

#[test]
fn capture_and_memory_over_stdio() {
    let (dir, root) = fresh_root();
    // A daily all-day event, so it is on the calendar whatever day the test runs.
    let ics = dir.path().join("cal.ics");
    std::fs::write(&ics, "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:walk\nSUMMARY:Morning walk\nDTSTART;VALUE=DATE:20200101\nRRULE:FREQ=DAILY\nEND:VEVENT\nEND:VCALENDAR\n").unwrap();
    std::fs::write(dir.path().join("config.json"), json!({"capture": {"calendars": [ics]}}).to_string()).unwrap();
    let mut c = Client::spawn(&root, &[]);

    let prompts = c.request("prompts/list", json!({}));
    let names: Vec<&str> = prompts["prompts"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"review_day") && names.contains(&"weekly_review"));
    let got = c.request("prompts/get", json!({"name": "review_day", "arguments": {"date": "2026-10-28"}}));
    assert!(got["messages"][0]["content"]["text"].as_str().unwrap().contains("capture_day for 2026-10-28"), "{got}");

    let cap = c.call("capture_day", json!({}));
    assert_eq!((cap["events"].clone(), cap["created"].clone()), (json!(1), json!(true)), "{cap}");
    assert_eq!(cap["draft"]["draft"], true);
    assert_eq!(cap["draft"]["title"], "Morning walk");
    let id = cap["draft"]["uuid"].as_str().unwrap().to_string();
    assert_eq!(c.call("list_drafts", json!({})).as_array().unwrap().len(), 1);
    assert_eq!(c.call("list_entries", json!({})), json!([]), "drafts stay out of listings");
    assert_eq!(c.call("capture_day", json!({}))["added"], 0);
    c.call("accept_draft", json!({"id": id}));
    assert_eq!(c.call("list_entries", json!({})).as_array().unwrap().len(), 1);

    let busan = c.call("remember", json!({"topic": "Home", "text": "Lives in Busan", "valid_from": "2019-01-01", "source": &id[..8]}));
    assert_eq!(busan["source"], id, "prefix resolves to the entry uuid");
    let seoul = c.call("remember", json!({"topic": "Home", "text": "Lives in Seoul", "valid_from": "2024-03-01", "replaces": [busan["id"]]}));
    assert_eq!(seoul["replaces"], json!([busan["id"]]));
    assert_eq!(c.call("recall", json!({"topic": "Home"}))[0]["text"], "Lives in Seoul");
    assert_eq!(c.call("recall", json!({"topic": "Home", "as_of": "2023-01-01"}))[0]["text"], "Lives in Busan");
    assert_eq!(c.call("fact_history", json!({"topic": "Home"})).as_array().unwrap().len(), 2);
    assert_eq!(c.call("list_topics", json!({})), json!([{"topic": "Home", "current": 1}]));
    assert!(root.join("memory/home.md").exists());
}

#[test]
fn read_only_blocks_memory_and_capture() {
    let (_d, root) = fresh_root();
    let mut c = Client::spawn(&root, &["--read-only"]);
    for (tool, args) in [("remember", json!({"topic": "a", "text": "b"})), ("capture_day", json!({}))] {
        let r = c.call(tool, args);
        assert!(r.get("error").is_some() || r.get("toolError").is_some(), "{tool}: {r}");
    }
    assert_eq!(c.call("recall", json!({})), json!([]));
}
