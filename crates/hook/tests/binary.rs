//! End-to-end tests of the `hopback-hook` binary: every event, bad input, exit code.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SID: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";

fn run(dir: &Path, stdin: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hopback-hook"))
        .env("HOPBACK_DIR", dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn event(name: &str, extra: &str) -> Vec<u8> {
    format!(
        r#"{{"session_id":"{SID}","transcript_path":"/Users/alex/.claude/projects/-Users-alex-web-app/{SID}.jsonl","cwd":"/Users/alex/web-app","permission_mode":"default","hook_event_name":"{name}"{extra}}}"#
    )
    .into_bytes()
}

fn state(dir: &Path) -> Option<String> {
    let raw = std::fs::read(dir.join(format!("{SID}.json"))).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    Some(v["state"].as_str().unwrap().to_owned())
}

fn assert_quiet_success(out: &std::process::Output) {
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "stdout must stay empty");
    assert!(out.stderr.is_empty(), "stderr must stay empty");
}

#[test]
fn full_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let steps: &[(&str, &str, Option<&str>)] = &[
        ("SessionStart", r#","source":"startup""#, Some("idle")),
        ("UserPromptSubmit", r#","prompt":"fix the build""#, Some("working")),
        ("PreToolUse", r#","tool_name":"Bash","tool_input":{"command":"ls"}"#, Some("working")),
        ("PermissionRequest", r#","tool_name":"Bash","tool_input":{"command":"ls"}"#, Some("approve")),
        ("Notification", r#","message":"Claude needs your permission","notification_type":"permission_prompt""#, Some("approve")),
        ("PostToolUse", r#","tool_name":"Bash","tool_response":{}"#, Some("working")),
        ("PostToolUseFailure", r#","tool_name":"Bash","error":"exit 1""#, Some("working")),
        ("PreToolUse", r#","tool_name":"AskUserQuestion","tool_input":{}"#, Some("question")),
        ("Notification", r#","message":"waiting","notification_type":"permission_prompt""#, Some("question")),
        ("PostToolUse", r#","tool_name":"AskUserQuestion""#, Some("working")),
        ("PreToolUse", r#","tool_name":"ExitPlanMode""#, Some("question")),
        ("SubagentStop", "", Some("question")),
        ("Stop", r#","stop_hook_active":false"#, Some("idle")),
        ("Notification", r#","notification_type":"idle_prompt""#, Some("idle")),
        ("SessionEnd", r#","reason":"exit""#, None),
    ];
    for (name, extra, expected) in steps {
        let out = run(d, &event(name, extra));
        assert_quiet_success(&out);
        assert_eq!(state(d).as_deref(), *expected, "after {name}");
    }
    assert!(d.join(".hook-seen").exists());
    // Only the .hook-seen file is left; no temp files.
    let names: Vec<_> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, vec![std::ffi::OsString::from(".hook-seen")]);
}

#[test]
fn marker_contents() {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &event("PermissionRequest", r#","tool_name":"Edit""#));
    let raw = std::fs::read(dir.path().join(format!("{SID}.json"))).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v["session_id"], SID);
    assert_eq!(v["cwd"], "/Users/alex/web-app");
    assert_eq!(v["tool"], "Edit");
    assert!(v["transcript_path"].as_str().unwrap().ends_with(".jsonl"));
    assert!(v["updated_at"].as_u64().unwrap() > 1_600_000_000_000);
    // The test harness is the parent, so some pid is recorded.
    assert!(v["pid"].as_i64().unwrap() > 1);
}

#[test]
fn malformed_and_unexpected_input_is_a_quiet_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let cases: &[&[u8]] = &[
        b"",
        b"not json",
        b"{",
        b"[]",
        b"null",
        b"{}",
        br#"{"hook_event_name":"Stop"}"#,
        br#"{"session_id":"../../escape","hook_event_name":"Stop"}"#,
        br#"{"session_id":42,"hook_event_name":"Stop"}"#,
        br#"{"session_id":"abc","hook_event_name":"SomethingNew"}"#,
        &[0xff, 0xfe, 0x00, 0x01],
    ];
    for raw in cases {
        let out = run(dir.path(), raw);
        assert_quiet_success(&out);
    }
    let markers = std::fs::read_dir(dir.path())
        .map(|it| it.filter(|e| e.as_ref().unwrap().file_name() != ".hook-seen").count())
        .unwrap_or(0);
    assert_eq!(markers, 0);
}

#[test]
fn missing_fields_still_write_a_marker() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), format!(r#"{{"session_id":"{SID}","hook_event_name":"Stop"}}"#).as_bytes());
    assert_quiet_success(&out);
    assert_eq!(state(dir.path()).as_deref(), Some("idle"));
}

#[test]
fn unwritable_directory_still_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-dir");
    std::fs::write(&file, b"x").unwrap();
    let out = run(&file.join("sub"), &event("Stop", ""));
    assert_quiet_success(&out);
}

#[test]
fn large_input_is_fast() {
    let dir = tempfile::tempdir().unwrap();
    let big = "x".repeat(2_000_000);
    let raw = event("PostToolUse", &format!(r#","tool_name":"Read","tool_response":{{"content":"{big}"}}"#));
    let t = Instant::now();
    let out = run(dir.path(), &raw);
    assert_quiet_success(&out);
    assert!(t.elapsed() < Duration::from_millis(500));
    assert_eq!(state(dir.path()).as_deref(), Some("working"));
}
