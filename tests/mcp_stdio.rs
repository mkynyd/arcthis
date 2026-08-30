#![cfg(feature = "mcp")]

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Client {
    fn start(root: &std::path::Path, extra: &[&str]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_arcthis"));
        command
            .arg("mcp")
            .arg("--allow-root")
            .arg(root)
            .args(extra)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn MCP server");
        let input = child.stdin.take().expect("MCP stdin");
        let output = BufReader::new(child.stdout.take().expect("MCP stdout"));
        Self {
            child,
            input: Some(input),
            output,
        }
    }

    fn send(&mut self, value: &Value) {
        let input = self.input.as_mut().expect("open MCP stdin");
        serde_json::to_writer(&mut *input, value).expect("write JSON-RPC");
        input.write_all(b"\n").expect("write delimiter");
        input.flush().expect("flush JSON-RPC");
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        let input = self.input.as_mut().expect("open MCP stdin");
        input.write_all(bytes).expect("write raw MCP bytes");
        input.flush().expect("flush raw MCP bytes");
    }

    fn response(&mut self) -> Value {
        let mut line = String::new();
        let count = self.output.read_line(&mut line).expect("read MCP response");
        assert!(count > 0, "MCP server closed before responding");
        assert_eq!(line.lines().count(), 1, "one JSON object per stdout line");
        serde_json::from_str(&line).expect("stdout must contain only JSON-RPC")
    }

    fn initialize(&mut self) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "arcthis-test-client", "version": "1"}
            }
        }));
        let initialized = self.response();
        assert_eq!(initialized["id"], 1);
        assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
        self.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    }

    #[allow(clippy::needless_pass_by_value)] // Test calls intentionally hand off owned JSON requests.
    fn call_tool(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        self.send(&json!({
            "jsonrpc":"2.0","id":id,"method":"tools/call","params":{
                "name":name,"arguments":arguments
            }
        }));
        self.response()
    }

    fn finish(mut self) -> String {
        drop(self.input.take());
        let status = self.child.wait().expect("wait for MCP server");
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .expect("MCP stderr")
            .read_to_string(&mut stderr)
            .expect("read MCP stderr");
        assert!(status.success(), "MCP server failed: {stderr}");
        stderr
    }
}

fn fixture() -> (TempDir, std::path::PathBuf) {
    let temporary = TempDir::new().expect("create fixture directory");
    let archive_path = temporary.path().join("mcp.zip");
    let file = File::create(&archive_path).expect("create fixture ZIP");
    let mut archive = ZipWriter::new(file);
    archive
        .start_file("hello.txt", SimpleFileOptions::default())
        .expect("start fixture entry");
    archive
        .write_all(b"hello MCP window")
        .expect("write fixture entry");
    archive.finish().expect("finish fixture ZIP");
    (temporary, archive_path)
}

fn cancellation_fixture() -> (TempDir, std::path::PathBuf) {
    let temporary = TempDir::new().expect("create cancellation fixture directory");
    let archive_path = temporary.path().join("cancel.zip");
    let file = File::create(&archive_path).expect("create cancellation ZIP");
    let mut archive = ZipWriter::new(file);
    archive
        .start_file(
            "large.bin",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .expect("start cancellation entry");
    let block = vec![0x5a; 1024 * 1024];
    for _ in 0..64 {
        archive.write_all(&block).expect("write cancellation entry");
    }
    archive.finish().expect("finish cancellation ZIP");
    (temporary, archive_path)
}

#[test]
fn discovery_declares_read_only_tools_and_schemas() {
    let (temporary, _archive) = fixture();
    let mut client = Client::start(temporary.path(), &[]);
    client.initialize();
    client.send(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
    let response = client.response();
    let tools = response["result"]["tools"].as_array().expect("tool array");
    assert_eq!(tools.len(), 9);
    for tool in tools {
        assert!(tool["inputSchema"].is_object());
        assert!(tool["outputSchema"].is_object());
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["destructiveHint"], false);
        assert_eq!(tool["annotations"]["idempotentHint"], true);
        assert_eq!(tool["annotations"]["openWorldHint"], false);
    }
    client.finish();
}

#[test]
fn bounded_read_and_root_policy_return_structured_results() {
    let (temporary, archive) = fixture();
    let mut client = Client::start(temporary.path(), &["--max-read-window", "5"]);
    client.initialize();
    client.send(&json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"archive_read",
            "arguments":{"path":archive,"entry":"hello.txt","offset":6,"length":3}
        }
    }));
    let response = client.response();
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(response["result"]["structuredContent"]["encoding"], "utf8");
    assert_eq!(response["result"]["structuredContent"]["data"], "MCP");
    assert_eq!(response["result"]["structuredContent"]["raw_size"], 3);

    client.send(&json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"archive_read",
            "arguments":{"path":archive,"entry":"hello.txt","offset":0,"length":6}
        }
    }));
    let bounded = client.response();
    assert_eq!(bounded["result"]["isError"], true);
    assert!(
        bounded["result"]["content"][0]["text"]
            .as_str()
            .expect("error text")
            .contains("resource_limit")
    );

    let outside = std::env::current_exe().expect("current executable");
    client.send(&json!({
        "jsonrpc":"2.0","id":4,"method":"tools/call","params":{
            "name":"archive_list","arguments":{"path":outside}
        }
    }));
    let denied = client.response();
    assert_eq!(denied["result"]["isError"], true);
    assert!(
        denied["result"]["content"][0]["text"]
            .as_str()
            .expect("denial text")
            .contains("permission_denied")
    );
    client.finish();
}

#[test]
fn malformed_message_does_not_contaminate_or_stop_stdio() {
    let (temporary, _archive) = fixture();
    let mut client = Client::start(temporary.path(), &[]);
    client.initialize();
    client.send_raw(b"{not-json}\n");
    client.send(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
    let response = client.response();
    assert_eq!(response["id"], 2);
    assert!(response["result"]["tools"].is_array());
    client.finish();
}

#[test]
fn cancelled_request_emits_no_late_tool_response() {
    let (temporary, archive) = cancellation_fixture();
    let mut client = Client::start(temporary.path(), &[]);
    client.initialize();
    client.send(&json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"archive_verify","arguments":{"path":archive}
        }
    }));
    client.send(&json!({
        "jsonrpc":"2.0","method":"notifications/cancelled","params":{
            "requestId":2,"reason":"test cancellation"
        }
    }));
    client.send(&json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}));
    let response = client.response();
    assert_eq!(
        response["id"], 3,
        "cancelled request must not emit a late response"
    );
    client.finish();
}

#[test]
fn mutation_tools_are_policy_gated_and_pack_plan_executes() {
    let temporary = TempDir::new().expect("mutation temp directory");
    let source = temporary.path().join("source");
    std::fs::create_dir(&source).expect("create pack source");
    std::fs::write(source.join("payload.txt"), b"mutation payload").expect("write pack source");
    let output = temporary.path().join("packed.zip");
    let output_root = temporary.path().to_str().expect("UTF-8 temp path");
    let mut client = Client::start(temporary.path(), &["--allow-output-root", output_root]);
    client.initialize();

    client.send(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
    let tools = client.response();
    assert_eq!(
        tools["result"]["tools"].as_array().expect("tools").len(),
        15
    );

    let request = json!({"path":source,"output":output});
    let plan = client.call_tool(3, "archive_pack_plan", request.clone());
    assert_eq!(plan["result"]["isError"], false, "{plan:#}");
    let digest = plan["result"]["structuredContent"]["plan_digest"]
        .as_str()
        .expect("plan digest");
    let mut execute = request;
    execute["plan_digest"] = Value::String(digest.to_owned());
    let result = client.call_tool(4, "archive_pack_execute", execute);
    assert_eq!(result["result"]["isError"], false);
    assert_eq!(
        result["result"]["structuredContent"]["result"]["verification"]["verified"],
        true
    );
    assert!(output.is_file());
    client.finish();
}

#[test]
fn extract_plan_execute_materializes_only_after_matching_digest() {
    let temporary = TempDir::new().expect("extraction temp directory");
    let archive = temporary.path().join("input.zip");
    let file = File::create(&archive).expect("create extraction ZIP");
    let mut writer = ZipWriter::new(file);
    writer
        .start_file("hello.txt", SimpleFileOptions::default())
        .expect("start extraction entry");
    writer
        .write_all(b"extract through MCP")
        .expect("write extraction entry");
    writer.finish().expect("finish extraction ZIP");

    let destination = temporary.path().join("extracted");
    let output_root = temporary.path().to_str().expect("UTF-8 temp path");
    let mut client = Client::start(temporary.path(), &["--allow-output-root", output_root]);
    client.initialize();

    let request = json!({"path":archive,"output":destination});
    let plan = client.call_tool(2, "archive_extract_plan", request.clone());
    assert_eq!(plan["result"]["isError"], false);
    assert!(!destination.exists(), "planning must not create output");
    let digest = plan["result"]["structuredContent"]["plan_digest"]
        .as_str()
        .expect("extract plan digest")
        .to_owned();
    let mut execute = request;
    execute["plan_digest"] = Value::String(digest);
    let result = client.call_tool(3, "archive_extract_execute", execute);
    assert_eq!(result["result"]["isError"], false);
    assert_eq!(
        std::fs::read(destination.join("hello.txt")).expect("read extracted file"),
        b"extract through MCP"
    );
    client.finish();
}

#[test]
fn stale_source_and_destination_races_are_rejected_without_commit() {
    let temporary = TempDir::new().expect("race temp directory");
    let source = temporary.path().join("source.txt");
    std::fs::write(&source, b"before").expect("write source");
    let packed = temporary.path().join("stale.zip");
    let output_root = temporary.path().to_str().expect("UTF-8 temp path");
    let mut client = Client::start(temporary.path(), &["--allow-output-root", output_root]);
    client.initialize();

    let request = json!({"path":source,"output":packed});
    let plan = client.call_tool(2, "archive_pack_plan", request.clone());
    let digest = plan["result"]["structuredContent"]["plan_digest"]
        .as_str()
        .expect("pack digest")
        .to_owned();
    std::fs::write(&source, b"after").expect("mutate source after plan");
    let mut execute = request;
    execute["plan_digest"] = Value::String(digest);
    let stale = client.call_tool(3, "archive_pack_execute", execute);
    assert_eq!(stale["result"]["isError"], true);
    assert!(
        stale["result"]["content"][0]["text"]
            .as_str()
            .expect("stale error")
            .contains("stale MCP mutation plan")
    );
    assert!(!packed.exists());

    let (_fixture_root, archive) = fixture();
    let copied_archive = temporary.path().join("source.zip");
    std::fs::copy(archive, &copied_archive).expect("copy extraction source");
    let destination = temporary.path().join("extract-output");
    let extract_request = json!({"path":copied_archive,"output":destination});
    let extract_plan = client.call_tool(4, "archive_extract_plan", extract_request.clone());
    let extract_digest = extract_plan["result"]["structuredContent"]["plan_digest"]
        .as_str()
        .expect("extract digest")
        .to_owned();
    std::fs::create_dir(&destination).expect("create destination race");
    let marker = destination.join("marker.txt");
    std::fs::write(&marker, b"preserve").expect("write race marker");
    let mut extract_execute = extract_request;
    extract_execute["plan_digest"] = Value::String(extract_digest);
    let raced = client.call_tool(5, "archive_extract_execute", extract_execute);
    assert_eq!(raced["result"]["isError"], true);
    assert_eq!(std::fs::read(&marker).expect("read marker"), b"preserve");
    assert!(copied_archive.exists());
    client.finish();
}

#[test]
fn output_roots_and_source_deletion_require_explicit_policy() {
    let temporary = TempDir::new().expect("policy temp directory");
    let source = temporary.path().join("delete-me.txt");
    std::fs::write(&source, b"delete only after verified pack").expect("write deletion source");
    let output = temporary.path().join("delete-policy.zip");
    let output_root = temporary.path().to_str().expect("UTF-8 temp path");

    let mut denied_client = Client::start(temporary.path(), &["--allow-output-root", output_root]);
    denied_client.initialize();
    let denied = denied_client.call_tool(
        2,
        "archive_pack_plan",
        json!({"path":source,"output":output,"delete_source":true}),
    );
    assert_eq!(denied["result"]["isError"], true);
    assert!(source.exists());
    assert!(!output.exists());
    denied_client.finish();

    let mut allowed_client = Client::start(
        temporary.path(),
        &[
            "--allow-output-root",
            output_root,
            "--allow-source-deletion",
        ],
    );
    allowed_client.initialize();
    let request = json!({"path":source,"output":output,"delete_source":true});
    let plan = allowed_client.call_tool(2, "archive_pack_plan", request.clone());
    let digest = plan["result"]["structuredContent"]["plan_digest"]
        .as_str()
        .expect("delete plan digest")
        .to_owned();
    let mut execute = request;
    execute["plan_digest"] = Value::String(digest);
    let completed = allowed_client.call_tool(3, "archive_pack_execute", execute);
    assert_eq!(completed["result"]["isError"], false);
    assert_eq!(
        completed["result"]["structuredContent"]["result"]["source_deleted"],
        true
    );
    assert!(!source.exists());
    assert!(output.exists());
    allowed_client.finish();
}

#[test]
fn convert_plan_execute_is_verified_and_output_bounded() {
    let temporary = TempDir::new().expect("convert temp directory");
    let archive = temporary.path().join("input.zip");
    let file = File::create(&archive).expect("create conversion ZIP");
    let mut writer = ZipWriter::new(file);
    writer
        .start_file("one.txt", SimpleFileOptions::default())
        .expect("start conversion entry");
    writer
        .write_all(b"convert me")
        .expect("write conversion entry");
    writer.finish().expect("finish conversion ZIP");
    let output = temporary.path().join("output.tar");
    let output_root = temporary.path().to_str().expect("UTF-8 temp path");
    let mut client = Client::start(temporary.path(), &["--allow-output-root", output_root]);
    client.initialize();
    let request = json!({"path":archive,"output":output});
    let plan = client.call_tool(2, "archive_convert_plan", request.clone());
    let digest = plan["result"]["structuredContent"]["plan_digest"]
        .as_str()
        .expect("convert digest")
        .to_owned();
    let mut execute = request;
    execute["plan_digest"] = Value::String(digest);
    let result = client.call_tool(3, "archive_convert_execute", execute);
    assert_eq!(result["result"]["isError"], false);
    assert_eq!(
        result["result"]["structuredContent"]["result"]["verification"]["verified"],
        true
    );
    assert!(output.is_file());

    let outside = temporary
        .path()
        .parent()
        .expect("outside root")
        .join("outside.zip");
    let rejected = client.call_tool(
        4,
        "archive_pack_plan",
        json!({"path":archive,"output":outside}),
    );
    assert_eq!(rejected["result"]["isError"], true);
    assert!(!outside.exists());
    client.finish();
}

#[test]
fn cancelled_mutation_stops_after_revalidation_without_writing() {
    let temporary = TempDir::new().expect("mutation cancellation temp directory");
    let source = temporary.path().join("large-source.bin");
    let file = File::create(&source).expect("create large mutation source");
    file.set_len(128 * 1024 * 1024)
        .expect("size large mutation source");
    drop(file);
    let output = temporary.path().join("cancelled.zip");
    let output_root = temporary.path().to_str().expect("UTF-8 temp path");
    let mut client = Client::start(temporary.path(), &["--allow-output-root", output_root]);
    client.initialize();
    let request = json!({"path":source,"output":output});
    let plan = client.call_tool(2, "archive_pack_plan", request.clone());
    let digest = plan["result"]["structuredContent"]["plan_digest"]
        .as_str()
        .expect("cancellation plan digest")
        .to_owned();
    let mut execute = request;
    execute["plan_digest"] = Value::String(digest);
    client.send(&json!({
        "jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"archive_pack_execute","arguments":execute
        }
    }));
    client.send(&json!({
        "jsonrpc":"2.0","method":"notifications/cancelled","params":{
            "requestId":3,"reason":"cancel before perform"
        }
    }));
    client.send(&json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}}));
    let response = client.response();
    assert_eq!(response["id"], 4);
    assert!(
        !output.exists(),
        "cancelled mutation must not commit output"
    );
    assert!(source.exists(), "cancelled mutation must preserve source");
    client.finish();
}

#[cfg(unix)]
#[test]
fn symlink_escapes_are_rejected_for_input_and_output() {
    use std::os::unix::fs::symlink;

    let temporary = TempDir::new().expect("symlink policy temp directory");
    let outside = TempDir::new().expect("outside temp directory");
    let outside_source = outside.path().join("outside.txt");
    std::fs::write(&outside_source, b"outside").expect("write outside source");
    let linked_source = temporary.path().join("linked-source");
    symlink(&outside_source, &linked_source).expect("link input outside root");
    let output = temporary.path().join("should-not-exist.zip");
    let output_root = temporary.path().to_str().expect("UTF-8 temp path");
    let mut client = Client::start(temporary.path(), &["--allow-output-root", output_root]);
    client.initialize();
    let denied_input = client.call_tool(
        2,
        "archive_pack_plan",
        json!({"path":linked_source,"output":output}),
    );
    assert_eq!(denied_input["result"]["isError"], true);
    assert!(!output.exists());

    let inside_source = temporary.path().join("inside.txt");
    std::fs::write(&inside_source, b"inside").expect("write inside source");
    let outside_target = outside.path().join("outside-output.zip");
    let linked_output = temporary.path().join("linked-output.zip");
    symlink(&outside_target, &linked_output).expect("link output outside root");
    let denied_output = client.call_tool(
        3,
        "archive_pack_plan",
        json!({"path":inside_source,"output":linked_output}),
    );
    assert_eq!(denied_output["result"]["isError"], true);
    assert!(!outside_target.exists());
    client.finish();
}
