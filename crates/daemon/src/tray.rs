// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! The tray menu (PRD §6.4, F30). Rebuilt whenever services or sites
//! change; each item's id says what it does.

use anyhow::Result;
use meow_core::config::{Config, WebServer};
use meow_core::engine::Engine;
use meow_core::service::{self, Kind, Unit};
use meow_core::supervisor::State;
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::icon;

/// What a menu item does, parsed back from its id.
pub enum Action {
    StartAll,
    StopAll,
    OpenApp,
    OpenUrl(String),
    UseWeb(WebServer),
    UsePhp(String),
    Toggle(Unit),
    OpenWww,
    Terminal,
    OpenLogs,
    Report,
    Quit,
}

impl Action {
    pub fn parse(id: &MenuId) -> Option<Action> {
        let id = id.as_ref();
        let (kind, arg) = id.split_once(':').unwrap_or((id, ""));
        Some(match kind {
            "start-all" => Action::StartAll,
            "stop-all" => Action::StopAll,
            "open-app" => Action::OpenApp,
            "url" => Action::OpenUrl(arg.to_owned()),
            "web" => Action::UseWeb(match arg {
                "nginx" => WebServer::Nginx,
                "apache" => WebServer::Apache,
                _ => WebServer::Caddy,
            }),
            "php" => Action::UsePhp(arg.to_owned()),
            "toggle" => Action::Toggle(Unit::parse(arg)?),
            "www" => Action::OpenWww,
            "terminal" => Action::Terminal,
            "logs" => Action::OpenLogs,
            "report" => Action::Report,
            "quit" => Action::Quit,
            _ => return None,
        })
    }
}

pub struct Tray {
    icon: TrayIcon,
    /// Set after a crash, until the user reports or dismisses it.
    pub crash_notice: bool,
    message: Option<String>,
}

impl Tray {
    pub fn new(engine: &Engine, crash_notice: bool) -> Result<Self> {
        let icon = TrayIconBuilder::new().with_tooltip("MeowServer").with_icon(icon::stopped()).build()?;
        let tray = Self { icon, crash_notice, message: None };
        tray.refresh(engine);
        Ok(tray)
    }

    pub fn show_message(&mut self, engine: &Engine, text: &str) {
        self.message = Some(text.chars().take(120).collect());
        self.refresh(engine);
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub fn refresh(&self, engine: &Engine) {
        let status = engine.status();
        let config = Config::load(engine.layout()).unwrap_or_default();
        let running: Vec<&str> = status
            .processes
            .iter()
            .filter(|p| matches!(p.state, State::Running { .. }))
            .map(|p| p.id.split('#').next().unwrap_or(&p.id))
            .collect();
        let failed = status.processes.iter().any(|p| matches!(p.state, State::Failed { .. }));
        let mut names: Vec<String> = Vec::new();
        for id in &running {
            let name = match id.strip_prefix("php-") {
                Some(version) => format!("PHP {version}"),
                None => title(id),
            };
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let headline = match (&self.message, failed, names.is_empty()) {
            (Some(m), _, _) => format!("Error: {m}"),
            (None, true, _) => "A service failed — see logs".to_owned(),
            (None, false, true) => "MeowServer — stopped".to_owned(),
            (None, false, false) => format!("MeowServer — {}", names.join(", ")),
        };
        let _ = self.icon.set_tooltip(Some(&headline));
        let _ = self.icon.set_icon(Some(if running.is_empty() { icon::stopped() } else { icon::running() }));
        match self.menu(engine, &config, &status.units, &headline, &status.sites) {
            Ok(menu) => self.icon.set_menu(Some(Box::new(menu))),
            Err(e) => log::warn!("tray menu: {e:#}"),
        }
    }

    fn menu(
        &self,
        engine: &Engine,
        config: &Config,
        units: &[Unit],
        headline: &str,
        sites: &[meow_core::site::Site],
    ) -> Result<Menu> {
        let layout = engine.layout();
        let item = |id: &str, text: &str| MenuItem::with_id(id, text, true, None);
        let menu = Menu::new();
        menu.append(&MenuItem::new(headline, false, None))?;
        if self.crash_notice {
            menu.append(&item("report", "MeowServer crashed last time — Report…"))?;
        }
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&item("start-all", "Start All"))?;
        menu.append(&item("stop-all", "Stop All"))?;
        menu.append(&MenuItem::with_id("open-app", "Open MeowServer", gui_exe().exists(), None))?;
        menu.append(&PredefinedMenuItem::separator())?;

        let port = config.port("web", 80);
        let suffix = if port == 80 { String::new() } else { format!(":{port}") };
        let sites_menu = Submenu::new("Sites", true);
        sites_menu.append(&item(&format!("url:http://localhost{suffix}/"), "localhost (all sites)"))?;
        if !sites.is_empty() {
            sites_menu.append(&PredefinedMenuItem::separator())?;
        }
        for site in sites.iter().take(40) {
            sites_menu.append(&item(&format!("url:http://{}{suffix}/", site.host), &site.host))?;
        }
        menu.append(&sites_menu)?;

        let web = Submenu::new("Web server", true);
        for (id, name, server) in [
            ("caddy", "Caddy", WebServer::Caddy),
            ("nginx", "Nginx", WebServer::Nginx),
            ("apache", "Apache", WebServer::Apache),
        ] {
            let kind = Kind::from_id(id).unwrap();
            let installed = !service::installed(layout, kind).is_empty();
            let checked = config.general.web_server == server;
            web.append(&CheckMenuItem::with_id(format!("web:{id}"), name, installed, checked, None))?;
        }
        menu.append(&web)?;

        let php = Submenu::new("PHP", true);
        let active =
            service::select(layout, Kind::Php, config.version_selector("php")).map(|i| i.version.line());
        let mut lines: Vec<String> =
            service::installed(layout, Kind::Php).iter().map(|i| i.version.line()).collect();
        lines.dedup();
        for line in &lines {
            let checked = active.as_deref() == Some(line.as_str());
            php.append(&CheckMenuItem::with_id(format!("php:{line}"), line, true, checked, None))?;
        }
        if lines.is_empty() {
            php.append(&MenuItem::new("Not installed", false, None))?;
        }
        menu.append(&php)?;

        let services = Submenu::new("Services", true);
        let mut any = false;
        for (unit, kind, name) in [
            (Unit::Mysql, Kind::Mysql, "MySQL"),
            (Unit::Mariadb, Kind::Mariadb, "MariaDB"),
            (Unit::Postgres, Kind::Postgres, "PostgreSQL"),
            (Unit::Redis, Kind::Redis, "Redis"),
            (Unit::Mailpit, Kind::Mailpit, "Mailpit"),
        ] {
            if service::installed(layout, kind).is_empty() {
                continue;
            }
            any = true;
            let on = units.contains(&unit);
            services.append(&CheckMenuItem::with_id(
                format!("toggle:{}", unit.id()),
                name,
                true,
                on,
                None,
            ))?;
        }
        if !any {
            services.append(&MenuItem::new("None installed", false, None))?;
        }
        menu.append(&services)?;
        menu.append(&PredefinedMenuItem::separator())?;

        menu.append(&item("www", "Open www folder"))?;
        menu.append(&item("terminal", "Terminal"))?;
        if !service::installed(layout, Kind::Mailpit).is_empty() {
            let url = meow_core::service::mailpit::web_url(config);
            menu.append(&item(&format!("url:{url}"), "Mail inbox"))?;
        }
        menu.append(&item("logs", "Open logs folder"))?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&item("report", "Report a problem…"))?;
        menu.append(&item("quit", "Quit (stops all services)") as &dyn IsMenuItem)?;
        Ok(menu)
    }
}

pub fn gui_exe() -> std::path::PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .with_file_name(format!("meowserver{}", std::env::consts::EXE_SUFFIX))
}

fn title(id: &str) -> String {
    match id {
        "caddy" => "Caddy",
        "nginx" => "Nginx",
        "apache" => "Apache",
        "mysql" => "MySQL",
        "mariadb" => "MariaDB",
        "postgres" => "PostgreSQL",
        "redis" => "Redis",
        "mailpit" => "Mailpit",
        other => other,
    }
    .to_owned()
}
