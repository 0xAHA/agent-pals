use crate::ipc::Envelope;
use crate::profile::{Action, AgentState, Profiles};
use crate::render::{draw_bubble, Bubble, Canvas, BUBBLE_H};
use crate::sprites::{seed_of, Theme};
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
    helper_of: Option<String>,
    leaving: bool,
    talk_until: Option<Instant>,
    talk_with: Option<String>,
}

impl Pal {
    fn new(key: String, label: String, state: AgentState, x: f32, now: Instant, timeout: Duration) -> Self {
        Pal {
            seed: seed_of(&key),
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
        }
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
            AgentState::Idle if now - self.state_since > ZZZ_AFTER => Some(Bubble::Zzz),
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
}

impl Scene {
    pub fn new(strip_w: f32, frame_w: f32, max: usize) -> Self {
        let mut s = Scene { pals: Vec::new(), strip_w, frame_w, max, helper_seq: 0 };
        s.set_frame_w(frame_w);
        s
    }

    pub fn set_frame_w(&mut self, fw: f32) {
        self.frame_w = fw;
        let fit = (self.strip_w / (fw + GAP)).floor() as usize;
        self.max = self.max.min(fit).max(1);
        self.layout();
    }

    pub fn is_empty(&self) -> bool {
        self.pals.is_empty()
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
        let key = format!("{}:{}", prof.id, prof.session_of(&env.payload));
        let label = prof.label_of(&env.payload);
        let timeout = Duration::from_secs(prof.idle_timeout_secs);

        let idx = match self.find(&key) {
            Some(i) => i,
            None => {
                if rule.action == Some(Action::Leave) || self.active() >= self.max {
                    return;
                }
                let name = label.clone().unwrap_or_else(|| prof.display_name.clone());
                self.pals.push(Pal::new(key.clone(), name, rule.state, self.offstage(), now, timeout));
                self.pals.len() - 1
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
            None => {}
        }
        if rule.state == AgentState::Done {
            self.dismiss_helper(&key, now, true);
        }
        self.layout();
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
        h.helper_of = Some(parent.to_string());
        h.talk_until = Some(now + Duration::from_secs(2));
        h.talk_with = Some(parent.to_string());
        let p = &mut self.pals[pi];
        p.talk_until = Some(now + Duration::from_secs(2));
        p.talk_with = Some(hkey);
        let mut at = pi + 1;
        while at < self.pals.len() && self.pals[at].helper_of.as_deref() == Some(parent) {
            at += 1;
        }
        self.pals.insert(at, h);
    }

    fn dismiss_helper(&mut self, parent: &str, now: Instant, all: bool) {
        let mut dismissed = false;
        for h in self.pals.iter_mut() {
            if h.helper_of.as_deref() == Some(parent) && !h.leaving {
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
    fn layout(&mut self) {
        let step = self.frame_w + GAP;
        let off = self.offstage();
        let mut slot = 0.0;
        for p in self.pals.iter_mut() {
            if p.leaving {
                p.target_x = off;
            } else {
                slot += 1.0;
                p.target_x = self.strip_w - slot * step;
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
                p.facing_left = match (&p.talk_with, p.talking(now)) {
                    (Some(w), true) => spots.iter().find(|(k, _)| k == w).map_or(true, |(_, wx)| *wx < p.x),
                    _ => true,
                };
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
        self.layout();
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

    pub fn render(&self, c: &mut Canvas, theme: &Theme, now: Instant) {
        c.clear();
        let y = BUBBLE_H as i32;
        let fw = theme.frame_w as i32;
        let fh = theme.frame_h as i32;
        for p in &self.pals {
            let x = p.x.round() as i32;
            for dx in 4..fw - 4 {
                c.dot(x + dx, y + fh - 1, [0, 0, 0], 70);
            }
            theme.draw(c, p.anim_name(now), p.anim_t, p.seed, x, y, p.facing_left);
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
