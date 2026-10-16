use crate::config::Config;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub enum TrayCmd {
    Show(bool),
    Autostart(bool),
    Scale(u32),
    Theme(String),
    Reload,
    OpenDir,
    Quit,
}

const SCALES: [u32; 4] = [2, 3, 4, 5];

pub struct Tray {
    icon: TrayIcon,
    show: CheckMenuItem,
    autostart: CheckMenuItem,
    scales: Vec<(u32, CheckMenuItem)>,
    themes: Vec<(String, CheckMenuItem)>,
    reload: MenuItem,
    open_dir: MenuItem,
    quit: MenuItem,
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

        let theme_menu = Submenu::new("Theme", true);
        let themes: Vec<(String, CheckMenuItem)> = theme_names
            .iter()
            .map(|n| (n.clone(), CheckMenuItem::new(n, true, *n == cfg.theme, None)))
            .collect();
        for (_, item) in &themes {
            theme_menu.append(item).map_err(e2s)?;
        }

        let autostart = CheckMenuItem::new("Start with Windows", true, crate::autostart::enabled(), None);
        let reload = MenuItem::new("Reload profiles and themes", true, None);
        let open_dir = MenuItem::new("Open config folder", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append_items(&[
            &show,
            &scale_menu,
            &theme_menu,
            &PredefinedMenuItem::separator(),
            &autostart,
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

        Ok(Tray { icon, show, autostart, scales, themes, reload, open_dir, quit, last_tip: String::new() })
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
        if let Some((s, _)) = self.scales.iter().find(|(_, i)| i.id() == id) {
            let s = *s;
            for (v, i) in &self.scales {
                i.set_checked(*v == s);
            }
            return Some(TrayCmd::Scale(s));
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
