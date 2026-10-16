//! Adds or removes the Agent Pals hooks in each agent's hook config, for the installer:
//!
//!   hook-relay.exe --install-claude-hooks | --uninstall-claude-hooks   (~/.claude/settings.json)
//!   hook-relay.exe --install-codex-hooks  | --uninstall-codex-hooks    (~/.codex/hooks.json)
//!
//! Both files share one shape: `{ "hooks": { "<Event>": [ { "matcher"?, "hooks": [ { "type":
//! "command", "command": ... } ] } ] } }`. A hook entry is ours if it runs a hook-relay with
//! `--agent <id>`; everything else in the file, other hooks included, is left as it was. The first
//! install keeps the original file as <name>.agent-pals.bak.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub struct Target {
    /// Profile id, passed to the relay as `--agent`.
    agent: &'static str,
    events: &'static [&'static str],
    /// Matcher for tool events; None leaves it out, which every agent reads as "all tools".
    tool_matcher: Option<&'static str>,
    file: fn() -> Option<PathBuf>,
    /// Only install if the agent's config folder exists (i.e. the agent is installed).
    needs_dir: bool,
}

const TOOL_EVENTS: &[&str] = &["PreToolUse", "PostToolUse"];

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

pub const CLAUDE: Target = Target {
    agent: "claude-code",
    events: &[
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
    ],
    tool_matcher: Some("*"),
    file: || match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(d) => Some(PathBuf::from(d).join("settings.json")),
        None => home().map(|h| h.join(".claude").join("settings.json")),
    },
    needs_dir: false,
};

/// Codex matchers are regular expressions ("*" is not a valid one), so tool events go
/// unmatched, which Codex reads as every tool. New or changed Codex hooks only run once the
/// user trusts them with /hooks.
pub const CODEX: Target = Target {
    agent: "codex",
    events: &[
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "SubagentStart",
        "SubagentStop",
        "Stop",
        "PreCompact",
        "SessionEnd",
    ],
    tool_matcher: None,
    file: || match std::env::var_os("CODEX_HOME") {
        Some(d) => Some(PathBuf::from(d).join("hooks.json")),
        None => home().map(|h| h.join(".codex").join("hooks.json")),
    },
    needs_dir: true,
};

fn is_ours(hook: &Value, agent: &str) -> bool {
    let tag = format!("--agent {agent}");
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains("hook-relay") && c.contains(&tag))
}

/// Forward slashes, because Claude Code runs hooks through Git Bash on Windows, where a
/// backslash is an escape. Quoted only when the path has a space: an unquoted path also works
/// in cmd and PowerShell, and Codex does not document which shell it uses.
pub fn command_for(relay: &Path, agent: &str) -> String {
    let p = relay.to_string_lossy().replace('\\', "/");
    if p.contains(' ') {
        format!("\"{p}\" --agent {agent}")
    } else {
        format!("{p} --agent {agent}")
    }
}

/// Drops our entries from every event, then, given a command, adds one per event.
pub fn apply(t: &Target, doc: &mut Value, command: Option<&str>) -> Result<(), String> {
    let root = doc.as_object_mut().ok_or("not a JSON object")?;
    let now_empty = {
        let hooks = root.entry("hooks").or_insert_with(|| json!({}));
        let hooks = hooks.as_object_mut().ok_or("\"hooks\" is not an object")?;
        if let Some(ev) = t.events.iter().find(|ev| hooks.get(**ev).is_some_and(|v| !v.is_array())) {
            return Err(format!("hooks.{ev} is not a list"));
        }
        for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
            for g in groups.iter_mut() {
                if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                    hs.retain(|h| !is_ours(h, t.agent));
                }
            }
            groups.retain(|g| g.get("hooks").and_then(Value::as_array).map_or(true, |hs| !hs.is_empty()));
        }
        if let Some(cmd) = command {
            for ev in t.events {
                let mut group = Map::new();
                if let (Some(m), true) = (t.tool_matcher, TOOL_EVENTS.contains(ev)) {
                    group.insert("matcher".into(), json!(m));
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

/// Exit code for the installer: 0 done (or nothing to do), 1 left the file untouched.
pub fn run(t: &Target, install: bool) -> i32 {
    let Some(path) = (t.file)() else {
        eprintln!("cannot find the home folder for {}", t.agent);
        return 1;
    };
    if t.needs_dir && !path.parent().is_some_and(Path::is_dir) {
        println!("{} not found ({}); skipped", t.agent, path.display());
        return 0;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) if !install => return 0,
        Err(_) => "{}".to_string(),
    };
    let mut doc: Value = match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{} is not valid JSON ({e}); not changing it", path.display());
            return 1;
        }
    };
    let command = if install {
        match std::env::current_exe() {
            Ok(exe) => Some(command_for(&exe, t.agent)),
            Err(e) => {
                eprintln!("cannot locate hook-relay.exe: {e}");
                return 1;
            }
        }
    } else {
        None
    };
    if let Err(e) = apply(t, &mut doc, command.as_deref()) {
        eprintln!("{}: {e}; not changing it", path.display());
        return 1;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut backup = path.clone().into_os_string();
    backup.push(".agent-pals.bak");
    let backup = PathBuf::from(backup);
    if install && path.exists() && !backup.exists() {
        let _ = std::fs::copy(&path, &backup);
    }
    let out = serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n";
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

    const CMD: &str = "C:/Users/x/AppData/Local/Programs/AgentPals/hook-relay.exe --agent claude-code";
    const CODEX_CMD: &str = "C:/Users/x/AppData/Local/Programs/AgentPals/hook-relay.exe --agent codex";

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
        apply(&CLAUDE, &mut s, Some(CMD)).unwrap();
        assert_eq!(ours_in(&s), CLAUDE.events.len(), "one entry per event, old ones gone");
        assert!(!s.to_string().contains("C:/Tools"));
        assert_eq!(s["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "lint.sh");
        assert_eq!(s["permissions"]["allow"][0], "Bash(ls)");
        let keys: Vec<&String> = s.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["permissions", "hooks", "effortLevel"], "key order preserved");
    }

    #[test]
    fn install_twice_is_the_same_as_once() {
        let mut once = json!({});
        apply(&CLAUDE, &mut once, Some(CMD)).unwrap();
        let mut twice = once.clone();
        apply(&CLAUDE, &mut twice, Some(CMD)).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn uninstall_removes_only_ours() {
        let mut s = json!({ "hooks": {
            "Stop": [{ "hooks": [{ "type": "command", "command": "notify.sh" }] }]
        }});
        apply(&CLAUDE, &mut s, Some(CMD)).unwrap();
        apply(&CLAUDE, &mut s, None).unwrap();
        assert_eq!(s, json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "notify.sh" }] }] } }));
        let mut only_ours = json!({ "model": "x" });
        apply(&CLAUDE, &mut only_ours, Some(CMD)).unwrap();
        apply(&CLAUDE, &mut only_ours, None).unwrap();
        assert_eq!(only_ours, json!({ "model": "x" }), "an emptied hooks block is removed");
    }

    #[test]
    fn agents_do_not_touch_each_others_hooks() {
        // Both relays in one file, as a user who points both agents at one config might have.
        let mut s = json!({});
        apply(&CLAUDE, &mut s, Some(CMD)).unwrap();
        apply(&CODEX, &mut s, Some(CODEX_CMD)).unwrap();
        apply(&CODEX, &mut s, None).unwrap();
        assert_eq!(ours_in(&s), CLAUDE.events.len(), "removing Codex's left Claude's");
        assert!(!s.to_string().contains("--agent codex"));
    }

    #[test]
    fn codex_tool_hooks_have_no_matcher() {
        let mut s = json!({});
        apply(&CODEX, &mut s, Some(CODEX_CMD)).unwrap();
        assert!(s["hooks"]["PreToolUse"][0].get("matcher").is_none(), "\"*\" is not a valid Codex regex");
        assert_eq!(s["hooks"]["PreToolUse"][0]["hooks"][0]["command"], CODEX_CMD);
        assert!(s["hooks"].get("Notification").is_none(), "Codex has no Notification event");
    }

    #[test]
    fn refuses_a_hooks_block_it_does_not_understand() {
        let mut s = json!({ "hooks": { "Stop": "oops" } });
        assert!(apply(&CLAUDE, &mut s, Some(CMD)).is_err());
        assert_eq!(s, json!({ "hooks": { "Stop": "oops" } }), "left untouched");
    }

    #[test]
    fn command_uses_forward_slashes_and_quotes_only_spaces() {
        let plain = command_for(Path::new(r"C:\Users\ab\AppData\Local\Programs\AgentPals\hook-relay.exe"), "codex");
        assert_eq!(plain, "C:/Users/ab/AppData/Local/Programs/AgentPals/hook-relay.exe --agent codex");
        let spaced = command_for(Path::new(r"C:\Users\A B\AppData\Local\Programs\AgentPals\hook-relay.exe"), "claude-code");
        assert_eq!(spaced, "\"C:/Users/A B/AppData/Local/Programs/AgentPals/hook-relay.exe\" --agent claude-code");
    }
}
