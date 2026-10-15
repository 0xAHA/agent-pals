use crate::config::log;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Thinking,
    Working,
    Waiting,
    Talking,
    Done,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    SpawnHelper,
    DismissHelper,
    Leave,
}

#[derive(Deserialize, Clone)]
pub struct Rule {
    pub event: String,
    /// Extra top-level field equality checks, e.g. { "tool_name": "Task" }.
    #[serde(default)]
    pub when: HashMap<String, String>,
    pub state: AgentState,
    #[serde(default)]
    pub action: Option<Action>,
}

#[derive(Deserialize, Clone)]
pub struct Profile {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    pub event_field: String,
    #[serde(default)]
    pub session_field: Option<String>,
    #[serde(default)]
    pub label_field: Option<String>,
    #[serde(default = "default_timeout")]
    pub idle_timeout_secs: u64,
    pub rules: Vec<Rule>,
}

fn default_timeout() -> u64 {
    900
}

pub type Profiles = HashMap<String, Profile>;

fn field_str(v: &Value, f: &str) -> Option<String> {
    match v.get(f)? {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

impl Profile {
    /// First matching rule wins, so put specific (`when`) rules first.
    pub fn match_rule(&self, v: &Value) -> Option<&Rule> {
        let ev = field_str(v, &self.event_field)?;
        self.rules.iter().find(|r| {
            r.event == ev
                && r.when
                    .iter()
                    .all(|(k, want)| field_str(v, k).as_deref() == Some(want.as_str()))
        })
    }

    pub fn session_of(&self, v: &Value) -> String {
        self.session_field
            .as_deref()
            .and_then(|f| field_str(v, f))
            .unwrap_or_else(|| "default".into())
    }

    /// Short label, e.g. the last folder of `cwd`.
    pub fn label_of(&self, v: &Value) -> Option<String> {
        let raw = field_str(v, self.label_field.as_deref()?)?;
        let last = raw
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&raw);
        Some(last.chars().take(24).collect())
    }
}

/// One JSON file per agent type, so new agents are a drop-in file.
pub fn load_all(dir: &Path) -> Profiles {
    let mut out = Profiles::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        log(format!("no profiles folder at {}", dir.display()));
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Profile>(&s).map_err(|e| e.to_string()))
        {
            Ok(prof) => {
                out.insert(prof.id.clone(), prof);
            }
            Err(e) => log(format!("profile {} skipped: {e}", p.display())),
        }
    }
    out
}
