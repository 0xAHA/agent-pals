use crate::ipc::Envelope;
use crate::profile::{Action, AgentState, Profiles};
use crate::render::{draw_bubble, Bubble, Canvas, BUBBLE_H};
use crate::sprites::{mix, seed_of, Theme};
use std::time::{Duration, Instant};

const WALK_SPEED: f32 = 20.0; // logical px / sec
const GAP: f32 = 2.0;
const DONE_FOR: Duration = Duration::from_secs(4);
const ZZZ_AFTER: Duration = Duration::from_secs(10);

pub struct Pal {
    key: String,
    label: String,
    state: AgentState,
    state_since: Instant,
    last_event: Instant,
    timeout: Duration,
    x: f32,
    target_x: f32,
    facing_left: bool,
    anim_t: f32,
    seed: u64,
    /// Index into the themes in play, dealt once at arrival (see `Scene::deal_theme`).
    theme: usize,
    helper_of: Option<String>,
    leaving: bool,
    talk_until: Option<Instant>,
    talk_with: Option<String>,
    /// Walks over to this pal while the chat lasts, then back to its slot.
    visiting: Option<String>,
    /// A helper for a known subagent (vs. a counted placeholder from SpawnHelper); it leaves on
    /// its own SubagentStop rather than when the parent finishes a turn.
    tracked: bool,
    // Per-pal liveliness, so a family on stage doesn't move in lockstep.
    /// Playback speed multiplier (0.85..1.15) and time offset into each anim.
    speed: f32,
    phase: f32,
    /// Picks the anim variant; rerolled on state change and every few seconds.
    roll: u64,
    rolls: u64,
    next_roll: Instant,
    /// Which way to face when standing about; flips now and then.
    face_left: bool,
    next_turn: Instant,
}

impl Pal {
    fn new(key: String, label: String, state: AgentState, x: f32, now: Instant, timeout: Duration) -> Self {
        let seed = seed_of(&key);
        let m = mix(seed);
        Pal {
            seed,
            theme: 0,
            key,
            label,
            state,
            state_since: now,
            last_event: now,
            timeout,
            x,
            target_x: x,
            facing_left: true,
            anim_t: 0.0,
            helper_of: None,
            leaving: false,
            talk_until: None,
            talk_with: None,
            visiting: None,
            tracked: false,
            speed: 0.85 + (m % 31) as f32 / 100.0,
            phase: ((m >> 8) % 1000) as f32 / 250.0,
            roll: mix(seed ^ 1),
            rolls: 1,
            next_roll: now + Duration::from_secs(5 + (m >> 20) % 6),
            face_left: (m >> 32) & 1 == 0,
            next_turn: now + Duration::from_secs(3 + (m >> 40) % 6),
        }
    }

    /// New variant pick, and when to pick again (5-10 s).
    fn reroll(&mut self, now: Instant) {
        self.rolls += 1;
        self.roll = mix(self.seed ^ self.rolls.wrapping_mul(0x9E3779B97F4A7C15));
        self.next_roll = now + Duration::from_secs(5 + self.roll % 6);
    }

    fn asleep(&self, now: Instant) -> bool {
        self.state == AgentState::Idle && now - self.state_since > ZZZ_AFTER
    }

    fn moving(&self) -> bool {
        (self.target_x - self.x).abs() > 0.5
    }

    fn talking(&self, now: Instant) -> bool {
        self.talk_until.map_or(false, |t| now < t)
    }

    fn set_state(&mut self, s: AgentState, now: Instant) {
        if self.state != s {
            self.state = s;
            self.state_since = now;
            self.anim_t = 0.0;
            self.reroll(now);
        }
    }

    fn anim_name(&self, now: Instant) -> &'static str {
        if self.moving() {
            return "walk";
        }
        if self.talking(now) {
            return "talk";
        }
        match self.state {
            AgentState::Idle => "idle",
            AgentState::Thinking => "think",
            AgentState::Working => "work",
            AgentState::Waiting => "wait",
            AgentState::Talking => "talk",
            AgentState::Done => "done",
        }
    }

    fn bubble(&self, now: Instant) -> Option<Bubble> {
        if self.moving() || self.talking(now) {
            return None;
        }
        match self.state {
            AgentState::Waiting => Some(Bubble::Bang),
            AgentState::Thinking => Some(Bubble::Dots(1 + ((self.anim_t * 3.0) as u8 % 3))),
            AgentState::Done => Some(Bubble::Check),
            AgentState::Idle if self.asleep(now) => Some(Bubble::Zzz),
            _ => None,
        }
    }
}

pub struct Scene {
    pals: Vec<Pal>,
    strip_w: f32,
    frame_w: f32,
    max: usize,
    helper_seq: u64,
    n_themes: usize,
}

impl Scene {
    pub fn new(strip_w: f32, frame_w: f32, max: usize) -> Self {
        let mut s = Scene { pals: Vec::new(), strip_w, frame_w, max, helper_seq: 0, n_themes: 1 };
        s.set_frame_w(frame_w);
        s
    }

    /// How many themes are in play. Pals already on stage keep their slot (wrapped if it shrank).
    pub fn set_theme_count(&mut self, n: usize) {
        self.n_themes = n.max(1);
    }

    /// The least-used theme among pals on stage, ties broken by the seed. Families don't
    /// repeat until every one is showing, and a pal keeps its family for life.
    fn deal_theme(&self, seed: u64) -> usize {
        let n = self.n_themes;
        let mut counts = vec![0usize; n];
        for p in self.pals.iter().filter(|p| !p.leaving) {
            counts[p.theme % n] += 1;
        }
        let least = counts.iter().copied().min().unwrap_or(0);
        let free: Vec<usize> = (0..n).filter(|&i| counts[i] == least).collect();
        free[(mix(seed ^ 0x7f4a7c159e3779b9) % free.len() as u64) as usize]
    }

    pub fn set_frame_w(&mut self, fw: f32) {
        self.frame_w = fw;
        let fit = (self.strip_w / (fw + GAP)).floor() as usize;
        self.max = self.max.min(fit).max(1);
        self.layout(Instant::now());
    }

    pub fn is_empty(&self) -> bool {
        self.pals.is_empty()
    }

    /// Any agent mid-task. An update restarts the app and swaps the hook relay, so automatic
    /// installs wait until this is false.
    pub fn busy(&self) -> bool {
        self.pals.iter().any(|p| {
            !p.leaving && matches!(p.state, AgentState::Working | AgentState::Thinking | AgentState::Waiting)
        })
    }

    fn find(&self, key: &str) -> Option<usize> {
        self.pals.iter().position(|p| p.key == key)
    }

    fn active(&self) -> usize {
        self.pals.iter().filter(|p| !p.leaving).count()
    }

    fn offstage(&self) -> f32 {
        self.strip_w + 2.0
    }

    pub fn apply(&mut self, env: &Envelope, profiles: &Profiles, now: Instant) {
        let Some(prof) = profiles.get(&env.agent) else { return };
        let Some(rule) = prof.match_rule(&env.payload) else { return };
        let parent = format!("{}:{}", prof.id, prof.session_of(&env.payload));
        let sub = prof.subagent_of(&env.payload);
        let key = match &sub {
            Some(a) => format!("{parent}#{a}"),
            None => parent.clone(),
        };
        // Subagents share the parent's cwd, so they keep "helper" rather than its label.
        let label = if sub.is_some() { None } else { prof.label_of(&env.payload) };
        let timeout = Duration::from_secs(prof.idle_timeout_secs);

        let idx = match self.find(&key) {
            Some(i) => i,
            None => {
                if rule.action == Some(Action::Leave) || self.active() >= self.max {
                    return;
                }
                if sub.is_some() {
                    self.add_tracked_helper(&parent, key.clone(), rule.state, now, timeout)
                } else {
                    let name = label.clone().unwrap_or_else(|| prof.display_name.clone());
                    let mut p = Pal::new(key.clone(), name, rule.state, self.offstage(), now, timeout);
                    p.theme = self.deal_theme(p.seed);
                    self.pals.push(p);
                    self.pals.len() - 1
                }
            }
        };

        {
            let p = &mut self.pals[idx];
            p.last_event = now;
            p.timeout = timeout;
            p.leaving = false;
            if let Some(l) = label {
                p.label = l;
            }
            p.set_state(rule.state, now);
        }
        // Parent activity keeps its helpers alive.
        for h in self.pals.iter_mut().filter(|h| h.helper_of.as_deref() == Some(key.as_str())) {
            h.last_event = now;
        }

        match rule.action {
            Some(Action::SpawnHelper) => self.spawn_helper(&key, now),
            Some(Action::DismissHelper) => self.dismiss_helper(&key, now, false),
            Some(Action::Leave) => {
                self.dismiss_helper(&key, now, true);
                self.pals[idx].leaving = true;
            }
            Some(Action::Talk) => {
                let to = match prof.target_of(&env.payload).as_deref() {
                    Some("main") | None => sub.as_ref().map(|_| parent.clone()),
                    Some(t) => Some(format!("{parent}#{t}")),
                };
                if let Some(to) = to {
                    self.talk(&key, &to, now);
                }
            }
            None => {}
        }
        if rule.state == AgentState::Done {
            self.dismiss_helper(&key, now, true);
        }
        self.layout(now);
    }

    /// Helpers sit just after their parent (and its earlier helpers) in the strip.
    fn insert_helper(&mut self, parent: &str, h: Pal) -> usize {
        let Some(pi) = self.find(parent) else {
            self.pals.push(h);
            return self.pals.len() - 1;
        };
        let mut at = pi + 1;
        while at < self.pals.len() && self.pals[at].helper_of.as_deref() == Some(parent) {
            at += 1;
        }
        self.pals.insert(at, h);
        at
    }

    /// A pal for one known subagent; it appears beside the parent and walks to its slot.
    fn add_tracked_helper(&mut self, parent: &str, key: String, state: AgentState, now: Instant, timeout: Duration) -> usize {
        let pi = self.find(parent);
        let x = pi.map_or(self.offstage(), |i| self.pals[i].x);
        let mut h = Pal::new(key, "helper".into(), state, x, now, timeout);
        h.theme = self.deal_theme(h.seed);
        h.tracked = true;
        // Without a parent on stage, tick() would send a helper straight off again.
        h.helper_of = pi.map(|_| parent.to_string());
        self.insert_helper(parent, h)
    }

    /// `from` walks over to `to` and they chat; the walk counts towards the chat, so a far
    /// visitor still gets ~2.5 s of talking on arrival.
    fn talk(&mut self, from: &str, to: &str, now: Instant) {
        let (Some(fi), Some(ti)) = (self.find(from), self.find(to)) else { return };
        if fi == ti || self.pals[fi].leaving || self.pals[ti].leaving {
            return;
        }
        let walk = ((self.pals[fi].x - self.pals[ti].x).abs() - self.frame_w).max(0.0) / WALK_SPEED;
        let until = now + Duration::from_secs_f32(walk + 2.5);
        let f = &mut self.pals[fi];
        f.talk_until = Some(until);
        f.talk_with = Some(to.to_string());
        f.visiting = Some(to.to_string());
        let t = &mut self.pals[ti];
        t.talk_until = Some(until);
        t.talk_with = Some(from.to_string());
    }

    fn spawn_helper(&mut self, parent: &str, now: Instant) {
        if self.active() >= self.max {
            return;
        }
        let Some(pi) = self.find(parent) else { return };
        self.helper_seq += 1;
        let hkey = format!("{parent}#{}", self.helper_seq);
        let (px, timeout) = (self.pals[pi].x, self.pals[pi].timeout);
        let mut h = Pal::new(hkey.clone(), "helper".into(), AgentState::Working, px, now, timeout);
        h.theme = self.deal_theme(h.seed);
        h.helper_of = Some(parent.to_string());
        h.talk_until = Some(now + Duration::from_secs(2));
        h.talk_with = Some(parent.to_string());
        let p = &mut self.pals[pi];
        p.talk_until = Some(now + Duration::from_secs(2));
        p.talk_with = Some(hkey);
        self.insert_helper(parent, h);
    }

    /// Placeholder helpers only; tracked ones come and go with their own subagent events.
    fn dismiss_helper(&mut self, parent: &str, now: Instant, all: bool) {
        let mut dismissed = false;
        for h in self.pals.iter_mut() {
            if h.helper_of.as_deref() == Some(parent) && !h.leaving && !h.tracked {
                h.leaving = true;
                dismissed = true;
                if !all {
                    break;
                }
            }
        }
        if dismissed && !all {
            if let Some(i) = self.find(parent) {
                self.pals[i].talk_until = Some(now + Duration::from_millis(1200));
                self.pals[i].talk_with = None;
            }
        }
    }

    /// Slots fill from the right (tray side) leftwards; leavers walk off right.
    /// A pal visiting someone stands beside them, on the near side, until the chat ends.
    fn layout(&mut self, now: Instant) {
        let step = self.frame_w + GAP;
        let off = self.offstage();
        let mut slot = 0.0;
        for p in self.pals.iter_mut() {
            if p.leaving {
                p.target_x = off;
                p.visiting = None;
            } else {
                slot += 1.0;
                p.target_x = self.strip_w - slot * step;
            }
        }
        let slots: Vec<(String, f32)> = self.pals.iter().map(|p| (p.key.clone(), p.target_x)).collect();
        let beside = self.frame_w * 0.75;
        for p in self.pals.iter_mut().filter(|p| !p.leaving) {
            if !p.talking(now) {
                p.visiting = None;
            }
            let Some(host) = &p.visiting else { continue };
            if let Some((_, hx)) = slots.iter().find(|(k, _)| k == host) {
                p.target_x = if p.target_x < *hx { hx - beside } else { hx + beside };
            }
        }
    }

    pub fn tick(&mut self, dt: f32, now: Instant) {
        let spots: Vec<(String, f32)> = self.pals.iter().map(|p| (p.key.clone(), p.x)).collect();
        for p in self.pals.iter_mut() {
            p.anim_t = (p.anim_t + dt) % 600.0;
            let d = p.target_x - p.x;
            if d.abs() > 0.5 {
                p.x += d.signum() * (WALK_SPEED * dt).min(d.abs());
                p.facing_left = d < 0.0;
            } else {
                p.x = p.target_x;
                if !p.asleep(now) && now >= p.next_turn {
                    p.face_left = !p.face_left;
                    p.next_turn = now + Duration::from_secs(2 + mix(p.roll ^ p.rolls) % 7);
                }
                p.facing_left = match (&p.talk_with, p.talking(now)) {
                    (Some(w), true) => spots.iter().find(|(k, _)| k == w).map_or(true, |(_, wx)| *wx < p.x),
                    _ => p.face_left,
                };
            }
            if now >= p.next_roll {
                p.reroll(now);
            }
            if p.state == AgentState::Done && now - p.state_since > DONE_FOR {
                p.set_state(AgentState::Idle, now);
            }
            if !p.leaving && now - p.last_event > p.timeout {
                p.leaving = true;
            }
        }
        // Helpers whose parent is gone or leaving follow them out.
        let parents: Vec<String> =
            self.pals.iter().filter(|p| !p.leaving).map(|p| p.key.clone()).collect();
        for p in self.pals.iter_mut() {
            if let Some(par) = &p.helper_of {
                if !parents.contains(par) {
                    p.leaving = true;
                }
            }
        }
        let gone = self.offstage() - 0.5;
        self.pals.retain(|p| !(p.leaving && p.x >= gone));
        self.layout(now);
    }

    /// True while anything on stage needs frames. Otherwise the timer drops to slow housekeeping.
    pub fn is_animating(&self, now: Instant) -> bool {
        self.pals.iter().any(|p| {
            p.moving()
                || p.talking(now)
                || p.state != AgentState::Idle
                || now - p.state_since <= ZZZ_AFTER + Duration::from_secs(1)
        })
    }

    /// `themes` is never empty; with several (random mode) each pal keeps one for life.
    pub fn render(&self, c: &mut Canvas, themes: &[Theme], now: Instant) {
        c.clear();
        let floor = BUBBLE_H as i32 + themes.iter().map(|t| t.frame_h).max().unwrap_or(0) as i32;
        for p in &self.pals {
            let theme = &themes[p.theme % themes.len()];
            let fw = theme.frame_w as i32;
            let fh = theme.frame_h as i32;
            let y = floor - fh;
            let x = p.x.round() as i32;
            for dx in 4..fw - 4 {
                c.dot(x + dx, y + fh - 1, [0, 0, 0], 70);
            }
            let t = p.anim_t * p.speed + p.phase;
            theme.draw(c, p.anim_name(now), p.roll, t, p.seed, x, y, p.facing_left);
            if let Some(b) = p.bubble(now) {
                let bounce = if b == Bubble::Bang && (p.anim_t * 4.0) as u32 % 2 == 0 { 1 } else { 0 };
                draw_bubble(c, x + 8, y - 10 - bounce + 1, b);
            }
        }
    }

    pub fn summary(&self) -> String {
        const NAMES: [&str; 6] = ["idle", "thinking", "working", "waiting", "talking", "done"];
        let mut counts = [0usize; 6];
        let mut waiting: Vec<&str> = Vec::new();
        let mut helpers = 0;
        for p in self.pals.iter().filter(|p| !p.leaving) {
            if p.helper_of.is_some() {
                helpers += 1;
                continue;
            }
            counts[p.state as usize] += 1;
            if p.state == AgentState::Waiting {
                waiting.push(&p.label);
            }
        }
        let mut parts: Vec<String> = counts
            .iter()
            .zip(NAMES)
            .filter(|(n, _)| **n > 0)
            .map(|(n, name)| format!("{n} {name}"))
            .collect();
        if helpers > 0 {
            parts.push(format!("{helpers} helper{}", if helpers == 1 { "" } else { "s" }));
        }
        let mut s = String::from("Agent Pals");
        if !parts.is_empty() {
            s.push('\n');
            s.push_str(&parts.join(" · "));
        }
        if !waiting.is_empty() {
            s.push_str("\nNeeds you: ");
            s.push_str(&waiting.join(", "));
        }
        s.chars().take(120).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const SID: &str = "s1";

    fn scene() -> (Scene, Profiles) {
        let profiles = crate::profile::load_all(std::path::Path::new("profiles"));
        assert!(profiles.contains_key("claude-code"), "run from the crate root");
        (Scene::new(160.0, 16.0, 8), profiles)
    }

    /// Payloads shaped like captured Claude Code hooks, after the relay has slimmed them.
    fn ev(s: &mut Scene, p: &Profiles, payload: Value) {
        let env = Envelope { agent: "claude-code".into(), payload };
        s.apply(&env, p, Instant::now());
    }

    fn tool(agent: Option<&str>, name: &str) -> Value {
        let mut v = json!({ "session_id": SID, "hook_event_name": "PreToolUse", "tool_name": name });
        if let Some(a) = agent {
            v["agent_id"] = json!(a);
        }
        v
    }

    fn pal<'a>(s: &'a Scene, key: &str) -> &'a Pal {
        s.pals.iter().find(|p| p.key == key).unwrap_or_else(|| panic!("no pal {key}"))
    }

    #[test]
    fn each_subagent_gets_its_own_helper() {
        let (mut s, p) = scene();
        ev(&mut s, &p, tool(None, "Agent"));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStart", "agent_id": "a1" }));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStart", "agent_id": "a2" }));
        ev(&mut s, &p, tool(Some("a1"), "Bash"));
        assert_eq!(s.pals.len(), 3, "parent + one helper per subagent, no placeholders");
        assert_eq!(pal(&s, "claude-code:s1#a1").helper_of.as_deref(), Some("claude-code:s1"));
        assert_eq!(pal(&s, "claude-code:s1#a1").state, AgentState::Working);
        // The subagent's tool call animates its helper, not the parent.
        assert_eq!(pal(&s, "claude-code:s1#a2").state, AgentState::Working);
    }

    #[test]
    fn send_message_walks_sender_to_recipient() {
        let (mut s, p) = scene();
        ev(&mut s, &p, tool(None, "Agent"));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStart", "agent_id": "a1" }));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStart", "agent_id": "a2" }));
        let mut msg = tool(Some("a1"), "SendMessage");
        msg["to"] = json!("a2");
        ev(&mut s, &p, msg);
        let (a1, a2) = (pal(&s, "claude-code:s1#a1"), pal(&s, "claude-code:s1#a2"));
        assert_eq!(a1.visiting.as_deref(), Some("claude-code:s1#a2"));
        assert_eq!(a2.talk_with.as_deref(), Some("claude-code:s1#a1"));
        assert!((a1.target_x - a2.target_x).abs() < 16.0, "sender stands beside the recipient");
    }

    #[test]
    fn message_to_main_visits_the_parent() {
        let (mut s, p) = scene();
        ev(&mut s, &p, tool(None, "Agent"));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStart", "agent_id": "a1" }));
        let mut msg = tool(Some("a1"), "SendMessage");
        msg["to"] = json!("main");
        ev(&mut s, &p, msg);
        assert_eq!(pal(&s, "claude-code:s1#a1").visiting.as_deref(), Some("claude-code:s1"));
    }

    #[test]
    fn subagent_stop_removes_that_helper_only() {
        let (mut s, p) = scene();
        ev(&mut s, &p, tool(None, "Agent"));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStart", "agent_id": "a1" }));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStart", "agent_id": "a2" }));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStop", "agent_id": "a2" }));
        assert!(!pal(&s, "claude-code:s1#a1").leaving);
        assert!(pal(&s, "claude-code:s1#a2").leaving);
    }

    #[test]
    fn parent_finishing_a_turn_keeps_running_helpers() {
        let (mut s, p) = scene();
        ev(&mut s, &p, tool(None, "Agent"));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "SubagentStart", "agent_id": "a1" }));
        ev(&mut s, &p, json!({ "session_id": SID, "hook_event_name": "Stop" }));
        assert!(!pal(&s, "claude-code:s1#a1").leaving, "background agents outlive the parent's turn");
    }
}
