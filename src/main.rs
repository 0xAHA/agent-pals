#![windows_subsystem = "windows"]

mod config;
mod ipc;
mod overlay;
mod profile;
mod render;
mod scene;
mod sprites;
mod tray;

use config::{log, Config};
use overlay::Overlay;
use render::{Canvas, BUBBLE_H};
use scene::Scene;
use sprites::Theme;
use std::sync::mpsc;
use std::time::Instant;
use tray::{Tray, TrayCmd};
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, PostQuitMessage, SetTimer, TranslateMessage, MSG, WM_TIMER,
};

const TIMER_ID: usize = 1;
const SLOW_MS: u32 = 5000;
const ICON_PX: u32 = 32;

#[derive(PartialEq, Clone, Copy)]
enum Tick {
    Off,
    Slow,
    Fast,
}

fn single_instance() -> bool {
    unsafe {
        match CreateMutexW(None, true, w!("Local\\AgentPals.Singleton")) {
            Ok(_) => GetLastError() != ERROR_ALREADY_EXISTS,
            Err(_) => true,
        }
    }
}

/// Largest logical frame across the themes in play; the strip is laid out for that.
fn frame_size(themes: &[Theme]) -> (u32, u32) {
    themes.iter().fold((1, 1), |(w, h), t| (w.max(t.frame_w), h.max(t.frame_h)))
}

/// Resize canvas + window to suit the current themes/scale.
fn fit(canvas: &mut Canvas, overlay: &mut Overlay, cfg: &Config, themes: &[Theme]) {
    canvas.resize(cfg.strip_width, frame_size(themes).1 + BUBBLE_H, cfg.scale);
    if let Err(e) = overlay.resize(canvas.w as i32, canvas.h as i32, cfg.margin_px) {
        log(format!("overlay resize failed: {e}"));
    }
}

fn main() {
    if !single_instance() {
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    config::trim_log();

    let dir = config::base_dir();
    let mut cfg = Config::load(dir);
    let mut profiles = profile::load_all(&dir.join("profiles"));
    let mut themes = sprites::load_set(dir, &cfg.theme);
    let (fw, fh) = frame_size(&themes);
    let mut scene = Scene::new(cfg.strip_width as f32, fw as f32, cfg.max_agents);
    scene.set_theme_count(themes.len());
    let mut canvas = Canvas::new(cfg.strip_width, fh + BUBBLE_H, cfg.scale);

    let mut overlay = match Overlay::create(canvas.w as i32, canvas.h as i32, cfg.margin_px) {
        Ok(o) => o,
        Err(e) => {
            log(format!("overlay create failed: {e}"));
            return;
        }
    };
    let mut theme_names = vec![sprites::RANDOM.to_string()];
    theme_names.extend(sprites::list_themes(&dir.join("themes")));
    let mut tray = match Tray::new(&cfg, &theme_names, themes[0].icon_rgba(ICON_PX), ICON_PX) {
        Ok(t) => t,
        Err(e) => {
            log(format!("tray create failed: {e}"));
            return;
        }
    };

    let (tx, rx) = mpsc::channel();
    ipc::spawn_server(tx, overlay.hwnd_raw());
    let loaded: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
    log(format!("started: {} profiles, theme '{}' ({})", profiles.len(), cfg.theme, loaded.join(", ")));

    let mut tick = Tick::Off;
    let mut last = Instant::now();
    let mut msg = MSG::default();

    loop {
        if !unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            break; // WM_QUIT (or error)
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let mut dirty = false;
        if msg.message == ipc::WM_APP_EVENT {
            let now = Instant::now();
            while let Ok(env) = rx.try_recv() {
                scene.apply(&env, &profiles, now);
            }
            dirty = true;
        } else if msg.message == WM_TIMER && msg.wParam.0 == TIMER_ID {
            dirty = true;
        }
        if overlay::take_reposition() {
            overlay.reposition(cfg.margin_px);
            dirty = true;
        }

        while let Some(cmd) = tray.poll() {
            dirty = true;
            match cmd {
                TrayCmd::Show(v) => {
                    cfg.visible = v;
                    cfg.save(dir);
                }
                TrayCmd::Scale(s) => {
                    cfg.scale = s;
                    cfg.save(dir);
                    fit(&mut canvas, &mut overlay, &cfg, &themes);
                }
                TrayCmd::Theme(name) => {
                    themes = sprites::load_set(dir, &name);
                    cfg.theme = name;
                    cfg.save(dir);
                    scene.set_theme_count(themes.len());
                    scene.set_frame_w(frame_size(&themes).0 as f32);
                    fit(&mut canvas, &mut overlay, &cfg, &themes);
                    tray.set_icon(themes[0].icon_rgba(ICON_PX), ICON_PX);
                }
                TrayCmd::Reload => {
                    profiles = profile::load_all(&dir.join("profiles"));
                    themes = sprites::load_set(dir, &cfg.theme);
                    scene.set_theme_count(themes.len());
                    scene.set_frame_w(frame_size(&themes).0 as f32);
                    fit(&mut canvas, &mut overlay, &cfg, &themes);
                    tray.set_icon(themes[0].icon_rgba(ICON_PX), ICON_PX);
                    log(format!("reloaded: {} profiles, {} themes", profiles.len(), themes.len()));
                }
                TrayCmd::OpenDir => {
                    let _ = std::process::Command::new("explorer").arg(dir).spawn();
                }
                TrayCmd::Quit => unsafe { PostQuitMessage(0) },
            }
        }

        if !dirty {
            continue;
        }
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.25);
        last = now;
        scene.tick(dt, now);

        if cfg.visible && !scene.is_empty() {
            scene.render(&mut canvas, &themes, now);
            overlay.present(&canvas.px);
        } else {
            overlay.hide();
        }
        tray.set_tooltip(&scene.summary());

        // Fast frames only while something moves; slow tick for timeouts; nothing when empty.
        let want = if scene.is_empty() {
            Tick::Off
        } else if cfg.visible && scene.is_animating(now) {
            Tick::Fast
        } else {
            Tick::Slow
        };
        if want != tick {
            unsafe {
                match want {
                    Tick::Off => {
                        let _ = KillTimer(overlay.hwnd, TIMER_ID);
                    }
                    Tick::Slow => {
                        SetTimer(overlay.hwnd, TIMER_ID, SLOW_MS, None);
                    }
                    Tick::Fast => {
                        SetTimer(overlay.hwnd, TIMER_ID, 1000 / cfg.fps.clamp(2, 30), None);
                    }
                }
            }
            tick = want;
        }
    }
}
