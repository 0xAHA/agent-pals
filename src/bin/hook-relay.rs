//! Tiny relay called by agent hooks. Forwards a slimmed payload to Agent Pals over a
//! named pipe and always exits 0 quietly, so it can never block or break the agent.
//!
//!   hook-relay.exe --agent claude-code          (payload on stdin; Codex hooks too, --agent codex)
//!   hook-relay.exe --agent <id> --arg <json>    (payload as last argument)
//!   hook-relay.exe --install-claude-hooks | --uninstall-claude-hooks   (used by the installer)
//!   hook-relay.exe --install-codex-hooks  | --uninstall-codex-hooks

#[path = "../agent_hooks.rs"]
mod agent_hooks;

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::time::Duration;

const PIPE: &str = r"\\.\pipe\agent-pals";
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PIPE_BUSY: i32 = 231;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    for (flag, target, install) in [
        ("--install-claude-hooks", &agent_hooks::CLAUDE, true),
        ("--uninstall-claude-hooks", &agent_hooks::CLAUDE, false),
        ("--install-codex-hooks", &agent_hooks::CODEX, true),
        ("--uninstall-codex-hooks", &agent_hooks::CODEX, false),
    ] {
        if args.iter().any(|a| a == flag) {
            std::process::exit(agent_hooks::run(target, install));
        }
    }
    let agent = args
        .iter()
        .position(|a| a == "--agent")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "claude-code".into());

    let raw = if args.iter().any(|a| a == "--arg") {
        args.last().cloned().unwrap_or_default()
    } else {
        let mut s = String::new();
        let _ = std::io::stdin().take(4 * 1024 * 1024).read_to_string(&mut s);
        s
    };
    debug_capture(&raw);
    let Ok(payload) = serde_json::from_str::<Value>(&raw) else { return };
    let msg = json!({ "agent": agent, "payload": slim(payload) }).to_string();

    for _ in 0..3 {
        match std::fs::OpenOptions::new().write(true).open(PIPE) {
            Ok(mut f) => {
                let _ = f.write_all(msg.as_bytes());
                return;
            }
            Err(e) if matches!(e.raw_os_error(), Some(ERROR_PIPE_BUSY) | Some(ERROR_FILE_NOT_FOUND)) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return,
        }
    }
}

/// Opt-in: with a `debug-hooks` file next to the exe, append each raw payload (first 2 KB)
/// to `hook-debug.jsonl` there. For working out which fields an agent actually sends;
/// unlike the pipe message it is not slimmed, so delete the flag when done.
fn debug_capture(raw: &str) {
    let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) else {
        return;
    };
    if !dir.join("debug-hooks").exists() {
        return;
    }
    let line: String = raw.trim().chars().take(2048).collect();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("hook-debug.jsonl")) {
        let _ = writeln!(f, "{line}");
    }
}

/// tool_input fields lifted to the top level before slimming: addressing only, never content.
/// `to` is SendMessage's recipient, so the overlay can show who is talking to whom.
const HOIST: &[&str] = &["to"];

/// The only fields forwarded: what profiles key on (event, tool, session, subagent, recipient,
/// label). An allow-list, because hooks also carry top-level text such as `prompt`,
/// `last_assistant_message` and `message`, and a new field must not leak just by appearing.
/// Claude Code and Codex hooks share these names.
const KEEP: &[&str] = &["hook_event_name", "tool_name", "session_id", "agent_id", "agent_type", "cwd", "to"];

/// Allow-listed top-level scalars only (strings capped). tool_input, tool_response, prompts,
/// replies and file contents never leave the hook.
fn slim(v: Value) -> Value {
    match v {
        Value::Object(mut m) => {
            let lifted: Vec<(String, Value)> = m
                .get("tool_input")
                .and_then(Value::as_object)
                .map(|ti| {
                    HOIST
                        .iter()
                        .filter_map(|k| ti.get(*k).filter(|v| v.is_string()).map(|v| (k.to_string(), v.clone())))
                        .collect()
                })
                .unwrap_or_default();
            for (k, v) in lifted {
                m.entry(k).or_insert(v);
            }
            slim_scalars(m)
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_message_recipient_but_no_content() {
        let raw = json!({
            "hook_event_name": "PreToolUse", "tool_name": "SendMessage", "agent_id": "a1",
            "tool_input": { "to": "a2", "message": "secret plans", "summary": "hi" },
        });
        let out = slim(raw);
        assert_eq!(out["to"], "a2");
        assert_eq!(out["agent_id"], "a1");
        assert!(out.get("tool_input").is_none());
        assert!(!out.to_string().contains("secret plans"));
    }

    #[test]
    fn drops_prompts_and_replies() {
        // Shapes seen in captured Claude Code payloads.
        let out = slim(json!({
            "hook_event_name": "UserPromptSubmit", "session_id": "s1", "cwd": "C:\\work\\proj",
            "prompt": "my secret prompt", "transcript_path": "C:\\Users\\me\\t.jsonl",
            "last_assistant_message": "the reply", "message": "Claude needs permission",
        }));
        let s = out.to_string();
        for leaked in ["secret prompt", "the reply", "needs permission", "t.jsonl"] {
            assert!(!s.contains(leaked), "{leaked} must not be forwarded: {s}");
        }
        assert_eq!(out["hook_event_name"], "UserPromptSubmit");
        assert_eq!(out["cwd"], "C:\\work\\proj");
    }

    #[test]
    fn hoist_never_overrides_a_top_level_field() {
        let out = slim(json!({ "to": "top", "tool_input": { "to": "nested" } }));
        assert_eq!(out["to"], "top");
    }
}

fn slim_scalars(m: serde_json::Map<String, Value>) -> Value {
    Value::Object(
        m.into_iter()
            .filter(|(k, _)| KEEP.contains(&k.as_str()))
            .filter_map(|(k, v)| match v {
                Value::String(s) => Some((k, Value::String(s.chars().take(200).collect()))),
                Value::Number(_) | Value::Bool(_) => Some((k, v)),
                _ => None,
            })
            .collect(),
    )
}
