//! Tiny relay called by agent hooks. Forwards a slimmed payload to Agent Pals over a
//! named pipe and always exits 0 quietly, so it can never block or break the agent.
//!
//!   hook-relay.exe --agent claude-code          (payload on stdin)
//!   hook-relay.exe --agent codex --arg <json>   (payload as last argument)

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::time::Duration;

const PIPE: &str = r"\\.\pipe\agent-pals";
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PIPE_BUSY: i32 = 231;

fn main() {
    let args: Vec<String> = std::env::args().collect();
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

/// Keep top-level scalars only (strings capped). Drops tool_input/tool_response,
/// so file contents and prompts never leave the hook.
fn slim(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .filter_map(|(k, v)| match v {
                    Value::String(s) => Some((k, Value::String(s.chars().take(200).collect()))),
                    Value::Number(_) | Value::Bool(_) => Some((k, v)),
                    _ => None,
                })
                .collect(),
        ),
        _ => Value::Null,
    }
}
