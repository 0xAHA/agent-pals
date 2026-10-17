#![windows_subsystem = "windows"]

mod autostart;
mod config;
mod http;
mod ipc;
mod overlay;
mod profile;
mod render;
mod scene;
mod sprites;
mod tray;
mod update;

use config::{log, Config, Placement};
use overlay::Overlay;
use render::{Canvas, BUBBLE_H};
use scene::Scene;
use sprites::Theme;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Instant;
use tray::{Tray, TrayCmd};
use update::{Release, UpdateMsg};
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
    scene.set_hide_idle(cfg.hide_idle_secs);
    let placement = Placement::parse(&cfg.position);
    scene.set_anchor(placement);
    let mut canvas = Canvas::new(cfg.strip_width, fh + BUBBLE_H, cfg.scale);

    let mut overlay = match Overlay::create(canvas.w as i32, canvas.h as i32, cfg.margin_px, placement) {
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
    log(format!(
        "started {}: {} profiles, theme '{}' ({})",
        update::CURRENT,
        profiles.len(),
        cfg.theme,
        loaded.join(", ")
    ));

    let (utx, urx) = mpsc::channel();
    let updates_on = Arc::new(AtomicBool::new(cfg.updates != "off"));
    update::spawn_schedule(utx.clone(), overlay.hwnd_raw(), updates_on.clone());
    let mut available: Option<Release> = None;
    let mut installing = false;

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
        } else if msg.message == update::WM_APP_UPDATE {
            while let Ok(m) = urx.try_recv() {
                match m {
                    UpdateMsg::Checked { result: Ok(Some(rel)), .. } => {
                        log(format!("update {} available", rel.version));
                        tray.set_update_status(&format!("Install update {}", rel.version), true);
                        available = Some(rel);
                    }
                    UpdateMsg::Checked { result: Ok(None), manual } => {
                        if manual {
                            tray.set_update_status(&format!("Agent Pals {} (up to date)", update::CURRENT), false);
                        }
                    }
                    UpdateMsg::Checked { result: Err(e), manual } => {
                        log(format!("update check failed: {e}"));
                        if manual {
                            tray.set_update_status("Update check failed (see log)", false);
                        }
                    }
                    UpdateMsg::InstallStarted(v) => {
                        // The installer stops this process shortly.
                        log(format!("installing update {v}"));
                        tray.set_update_status(&format!("Installing {v}..."), false);
                    }
                    UpdateMsg::InstallFailed(e) => {
                        log(format!("update failed: {e}"));
                        installing = false;
                        if let Some(rel) = &available {
                            tray.set_update_status(&format!("Retry update {} (failed, see log)", rel.version), true);
                        }
                    }
                }
            }
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
                TrayCmd::CheckUpdates => {
                    tray.set_update_status("Checking for updates...", false);
                    update::spawn_check(utx.clone(), overlay.hwnd_raw());
                }
                TrayCmd::InstallUpdate => {
                    if let (Some(rel), false) = (&available, installing) {
                        installing = true;
                        tray.set_update_status(&format!("Downloading {}...", rel.version), false);
                        update::spawn_install(rel.clone(), utx.clone(), overlay.hwnd_raw());
                    }
                }
                TrayCmd::UpdateMode(m) => {
                    updates_on.store(m != "off", Ordering::Relaxed);
                    cfg.updates = m;
                    cfg.save(dir);
                }
                TrayCmd::Autostart(on) => {
                    if let Err(e) = autostart::set(on) {
                        log(format!("start with Windows: {e}"));
                    }
                    tray.set_autostart(autostart::enabled());
                }
                TrayCmd::Scale(s) => {
                    cfg.scale = s;
                    cfg.save(dir);
                    fit(&mut canvas, &mut overlay, &cfg, &themes);
                }
                TrayCmd::Position(v) => {
                    let placement = Placement::parse(&v);
                    cfg.position = v;
                    cfg.save(dir);
                    overlay.placement = placement;
                    overlay.reposition(cfg.margin_px);
                    scene.set_anchor(placement);
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
        let mut tip = scene.summary();
        if let Some(rel) = &available {
            tip.push_str(&format!("\nUpdate {} available", rel.version));
        }
        tray.set_tooltip(&tip);

        // Automatic updates wait for a quiet moment: the install restarts the app and replaces
        // the hook relay mid-session otherwise.
        if cfg.updates == "auto" && !installing && !scene.busy() {
            if let Some(rel) = &available {
                installing = true;
                tray.set_update_status(&format!("Downloading {}...", rel.version), false);
                update::spawn_install(rel.clone(), utx.clone(), overlay.hwnd_raw());
            }
        }

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
