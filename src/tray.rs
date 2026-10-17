use crate::config::Config;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub enum TrayCmd {
    Show(bool),
    Autostart(bool),
    Scale(u32),
    Position(String),
    Theme(String),
    Reload,
    OpenDir,
    Quit,
    CheckUpdates,
    InstallUpdate,
    UpdateMode(String),
}

const SCALES: [u32; 4] = [2, 3, 4, 5];
/// Config value and menu label for each strip position.
const POSITIONS: [(&str, &str); 3] = [("left", "Left"), ("center", "Middle"), ("right", "Right")];
/// Config value and menu label for each update mode.
const UPDATE_MODES: [(&str, &str); 3] =
    [("notify", "Tell me about updates"), ("auto", "Install updates automatically"), ("off", "Don't check")];

pub struct Tray {
    icon: TrayIcon,
    show: CheckMenuItem,
    autostart: CheckMenuItem,
    scales: Vec<(u32, CheckMenuItem)>,
    positions: Vec<(String, CheckMenuItem)>,
    themes: Vec<(String, CheckMenuItem)>,
    reload: MenuItem,
    open_dir: MenuItem,
    quit: MenuItem,
    /// "Agent Pals 0.2.2 (up to date)", or "Install update 0.3.0" once one is found.
    update_status: MenuItem,
    check_updates: MenuItem,
    update_modes: Vec<(String, CheckMenuItem)>,
    last_tip: String,
}

fn e2s(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Tray {
    pub fn new(cfg: &Config, theme_names: &[String], icon_rgba: Vec<u8>, icon_size: u32) -> Result<Self, String> {
        let menu = Menu::new();
        let show = CheckMenuItem::new("Show pals", true, cfg.visible, None);

        let scale_menu = Submenu::new("Scale", true);
        let scales: Vec<(u32, CheckMenuItem)> = SCALES
            .iter()
            .map(|&s| (s, CheckMenuItem::new(format!("{s}x"), true, cfg.scale == s, None)))
            .collect();
        for (_, item) in &scales {
            scale_menu.append(item).map_err(e2s)?;
        }

        let position_menu = Submenu::new("Position", true);
        let current = crate::config::Placement::parse(&cfg.position);
        let positions: Vec<(String, CheckMenuItem)> = POSITIONS
            .iter()
            .map(|(v, label)| {
                let on = crate::config::Placement::parse(v) == current;
                (v.to_string(), CheckMenuItem::new(*label, true, on, None))
            })
            .collect();
        for (_, item) in &positions {
            position_menu.append(item).map_err(e2s)?;
        }

        let theme_menu = Submenu::new("Theme", true);
        let themes: Vec<(String, CheckMenuItem)> = theme_names
            .iter()
            .map(|n| (n.clone(), CheckMenuItem::new(n, true, *n == cfg.theme, None)))
            .collect();
        for (_, item) in &themes {
            theme_menu.append(item).map_err(e2s)?;
        }

        let update_menu = Submenu::new("Updates", true);
        let update_status = MenuItem::new(format!("Agent Pals {}", crate::update::CURRENT), false, None);
        let check_updates = MenuItem::new("Check now", cfg.updates != "off", None);
        let update_modes: Vec<(String, CheckMenuItem)> = UPDATE_MODES
            .iter()
            .map(|(v, label)| (v.to_string(), CheckMenuItem::new(*label, true, cfg.updates == *v, None)))
            .collect();
        update_menu.append_items(&[&update_status, &check_updates, &PredefinedMenuItem::separator()]).map_err(e2s)?;
        for (_, item) in &update_modes {
            update_menu.append(item).map_err(e2s)?;
        }

        let autostart = CheckMenuItem::new("Start with Windows", true, crate::autostart::enabled(), None);
        let reload = MenuItem::new("Reload profiles and themes", true, None);
        let open_dir = MenuItem::new("Open config folder", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append_items(&[
            &show,
            &scale_menu,
            &position_menu,
            &theme_menu,
            &PredefinedMenuItem::separator(),
            &autostart,
            &update_menu,
            &reload,
            &open_dir,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(e2s)?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Agent Pals")
            .with_icon(Icon::from_rgba(icon_rgba, icon_size, icon_size).map_err(e2s)?)
            .build()
            .map_err(e2s)?;

        Ok(Tray {
            icon,
            show,
            autostart,
            scales,
            positions,
            themes,
            reload,
            open_dir,
            quit,
            update_status,
            check_updates,
            update_modes,
            last_tip: String::new(),
        })
    }

    /// Non-blocking; call after each dispatched message.
    pub fn poll(&self) -> Option<TrayCmd> {
        let ev = MenuEvent::receiver().try_recv().ok()?;
        let id = &ev.id;
        if id == self.show.id() {
            return Some(TrayCmd::Show(self.show.is_checked()));
        }
        if id == self.autostart.id() {
            return Some(TrayCmd::Autostart(self.autostart.is_checked()));
        }
        if id == self.reload.id() {
            return Some(TrayCmd::Reload);
        }
        if id == self.open_dir.id() {
            return Some(TrayCmd::OpenDir);
        }
        if id == self.quit.id() {
            return Some(TrayCmd::Quit);
        }
        if id == self.check_updates.id() {
            return Some(TrayCmd::CheckUpdates);
        }
        if id == self.update_status.id() {
            return Some(TrayCmd::InstallUpdate);
        }
        if let Some((m, _)) = self.update_modes.iter().find(|(_, i)| i.id() == id) {
            let m = m.clone();
            for (v, i) in &self.update_modes {
                i.set_checked(*v == m);
            }
            self.check_updates.set_enabled(m != "off");
            return Some(TrayCmd::UpdateMode(m));
        }
        if let Some((s, _)) = self.scales.iter().find(|(_, i)| i.id() == id) {
            let s = *s;
            for (v, i) in &self.scales {
                i.set_checked(*v == s);
            }
            return Some(TrayCmd::Scale(s));
        }
        if let Some((v, _)) = self.positions.iter().find(|(_, i)| i.id() == id) {
            let v = v.clone();
            for (p, i) in &self.positions {
                i.set_checked(*p == v);
            }
            return Some(TrayCmd::Position(v));
        }
        if let Some((n, _)) = self.themes.iter().find(|(_, i)| i.id() == id) {
            let n = n.clone();
            for (v, i) in &self.themes {
                i.set_checked(*v == n);
            }
            return Some(TrayCmd::Theme(n));
        }
        None
    }

    /// The status line at the top of Updates; `clickable` when it installs an update.
    pub fn set_update_status(&self, text: &str, clickable: bool) {
        self.update_status.set_text(text);
        self.update_status.set_enabled(clickable);
    }

    /// Re-sync the tick with the registry, e.g. after a failed write.
    pub fn set_autostart(&self, on: bool) {
        self.autostart.set_checked(on);
    }

    pub fn set_tooltip(&mut self, tip: &str) {
        if tip != self.last_tip {
            let _ = self.icon.set_tooltip(Some(tip));
            self.last_tip = tip.to_string();
        }
    }

    pub fn set_icon(&self, rgba: Vec<u8>, size: u32) {
        if let Ok(i) = Icon::from_rgba(rgba, size, size) {
            let _ = self.icon.set_icon(Some(i));
        }
    }
}
