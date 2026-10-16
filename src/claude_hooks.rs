//! Adds or removes the Agent Pals hooks in Claude Code's settings.json, for the installer:
//!
//!   hook-relay.exe --install-claude-hooks     (points every hook at this relay)
//!   hook-relay.exe --uninstall-claude-hooks
//!
//! A hook entry is ours if it runs a hook-relay with `--agent claude-code`. Everything else in
//! the file, other hooks included, is left as it was. The first install keeps the original file
//! as settings.json.agent-pals.bak.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "PreCompact",
    "SessionEnd",
];
/// Events whose hook groups take a tool matcher.
const TOOL_EVENTS: &[&str] = &["PreToolUse", "PostToolUse"];

fn settings_path() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(d).join("settings.json"));
    }
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".claude").join("settings.json"))
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains("hook-relay") && c.contains("--agent claude-code"))
}

/// Claude Code runs hooks through Git Bash on Windows, where a backslash is an escape
/// character: use forward slashes, and quote in case the path has spaces.
pub fn command_for(relay: &Path) -> String {
    format!("\"{}\" --agent claude-code", relay.to_string_lossy().replace('\\', "/"))
}

/// Drops our entries from every event, then, given a command, adds one per event.
pub fn apply(settings: &mut Value, command: Option<&str>) -> Result<(), String> {
    let root = settings.as_object_mut().ok_or("settings.json is not a JSON object")?;
    let now_empty = {
        let hooks = root.entry("hooks").or_insert_with(|| json!({}));
        let hooks = hooks.as_object_mut().ok_or("\"hooks\" is not an object")?;
        if let Some(ev) = EVENTS.iter().find(|ev| hooks.get(**ev).is_some_and(|v| !v.is_array())) {
            return Err(format!("hooks.{ev} is not a list"));
        }
        for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
            for g in groups.iter_mut() {
                if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                    hs.retain(|h| !is_ours(h));
                }
            }
            groups.retain(|g| g.get("hooks").and_then(Value::as_array).map_or(true, |hs| !hs.is_empty()));
        }
        if let Some(cmd) = command {
            for ev in EVENTS {
                let mut group = Map::new();
                if TOOL_EVENTS.contains(ev) {
                    group.insert("matcher".into(), json!("*"));
                }
                group.insert("hooks".into(), json!([{ "type": "command", "command": cmd }]));
                let list = hooks.entry(ev.to_string()).or_insert_with(|| json!([]));
                if let Some(a) = list.as_array_mut() {
                    a.push(Value::Object(group));
                }
            }
        }
        hooks.retain(|_, v| v.as_array().map_or(true, |a| !a.is_empty()));
        hooks.is_empty()
    };
    if now_empty {
        root.remove("hooks");
    }
    Ok(())
}

/// Exit code for the installer: 0 done, 1 left the file untouched.
pub fn run(install: bool) -> i32 {
    let Some(path) = settings_path() else {
        eprintln!("no USERPROFILE or CLAUDE_CONFIG_DIR; cannot find Claude Code settings");
        return 1;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) if !install => return 0,
        Err(_) => "{}".to_string(),
    };
    let mut settings: Value = match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{} is not valid JSON ({e}); not changing it", path.display());
            return 1;
        }
    };
    let command = if install {
        match std::env::current_exe() {
            Ok(exe) => Some(command_for(&exe)),
            Err(e) => {
                eprintln!("cannot locate hook-relay.exe: {e}");
                return 1;
            }
        }
    } else {
        None
    };
    if let Err(e) = apply(&mut settings, command.as_deref()) {
        eprintln!("{}: {e}; not changing it", path.display());
        return 1;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let backup = path.with_extension("json.agent-pals.bak");
    if install && path.exists() && !backup.exists() {
        let _ = std::fs::copy(&path, &backup);
    }
    let out = serde_json::to_string_pretty(&settings).unwrap_or_default() + "\n";
    match std::fs::write(&path, out) {
        Ok(()) => {
            println!("{} Agent Pals hooks in {}", if install { "installed" } else { "removed" }, path.display());
            0
        }
        Err(e) => {
            eprintln!("cannot write {}: {e}", path.display());
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "\"C:/Users/x/AppData/Local/Programs/AgentPals/hook-relay.exe\" --agent claude-code";

    fn ours_in(v: &Value) -> usize {
        v.to_string().matches("hook-relay").count()
    }

    #[test]
    fn install_replaces_an_old_relay_and_keeps_everything_else() {
        let mut s = json!({
            "permissions": { "allow": ["Bash(ls)"] },
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [{ "type": "command", "command": "lint.sh" }] },
                    { "matcher": "*", "hooks": [{ "type": "command", "command": "C:/Tools/agent-pals/hook-relay.exe --agent claude-code" }] }
                ],
                "Stop": [{ "hooks": [{ "type": "command", "command": "C:\\Tools\\agent-pals\\hook-relay.exe --agent claude-code" }] }]
            },
            "effortLevel": "high"
        });
        apply(&mut s, Some(CMD)).unwrap();
        assert_eq!(ours_in(&s), EVENTS.len(), "one entry per event, old ones gone");
        assert!(!s.to_string().contains("C:/Tools"));
        assert_eq!(s["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "lint.sh");
        assert_eq!(s["permissions"]["allow"][0], "Bash(ls)");
        let keys: Vec<&String> = s.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["permissions", "hooks", "effortLevel"], "key order preserved");
    }

    #[test]
    fn install_twice_is_the_same_as_once() {
        let mut once = json!({});
        apply(&mut once, Some(CMD)).unwrap();
        let mut twice = once.clone();
        apply(&mut twice, Some(CMD)).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn uninstall_removes_only_ours() {
        let mut s = json!({ "hooks": {
            "Stop": [{ "hooks": [{ "type": "command", "command": "notify.sh" }] }]
        }});
        apply(&mut s, Some(CMD)).unwrap();
        apply(&mut s, None).unwrap();
        assert_eq!(s, json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "notify.sh" }] }] } }));
        let mut only_ours = json!({ "model": "x" });
        apply(&mut only_ours, Some(CMD)).unwrap();
        apply(&mut only_ours, None).unwrap();
        assert_eq!(only_ours, json!({ "model": "x" }), "an emptied hooks block is removed");
    }

    #[test]
    fn refuses_a_hooks_block_it_does_not_understand() {
        let mut s = json!({ "hooks": { "Stop": "oops" } });
        assert!(apply(&mut s, Some(CMD)).is_err());
        assert_eq!(s, json!({ "hooks": { "Stop": "oops" } }), "left untouched");
    }

    #[test]
    fn command_uses_forward_slashes_and_quotes() {
        let c = command_for(Path::new(r"C:\Users\A B\AppData\Local\Programs\AgentPals\hook-relay.exe"));
        assert_eq!(c, "\"C:/Users/A B/AppData/Local/Programs/AgentPals/hook-relay.exe\" --agent claude-code");
    }
}
