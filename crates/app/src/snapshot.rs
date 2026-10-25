// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Everything the window shows, gathered off the UI thread as plain data.

use meow_core::config::{Config, Suffix, WebServer};
use meow_core::engine::Status;
use meow_core::ipc::{Client, Request};
use meow_core::layout::Layout;
use meow_core::service::{self, Kind, Unit, datadir, mailpit, mysql, postgres, redis, web};
use meow_core::site::{self, Framework, Site, Source};
use meow_core::supervisor::State;

use crate::tr::t;

pub struct ServiceInfo {
    pub unit: Unit,
    pub name: String,
    pub version: Option<String>,
    pub port: String,
    pub state: String,
    pub running: bool,
    pub failed: bool,
}

/// A site's missing need in the window's language: (text, what
/// `install-service` installs, button label).
pub fn need(problem: &site::needs::Problem) -> (String, String, String) {
    use site::needs::Problem;
    let fill =
        |template: &str, values: &[&str]| values.iter().fold(t(template), |s, v| s.replacen("{}", v, 1));
    match problem {
        Problem::Php { constraint, running, suggest } => (
            if running == "none" {
                fill("Needs PHP {}; no PHP is installed.", &[constraint])
            } else {
                fill("Needs PHP {}, but would run on PHP {}.", &[constraint, running])
            },
            format!("php {suggest}"),
            fill("Install PHP {}", &[suggest]),
        ),
        Problem::DatabaseMissing { family } => (
            fill("Uses {}, which is not installed.", &[family.name()]),
            family.kinds()[0].id().to_owned(),
            fill("Install {}", &[family.name()]),
        ),
        Problem::DatabasePort { family, port, served } => {
            let served: Vec<String> = served.iter().map(u16::to_string).collect();
            (
                fill(
                    "Connects to {} on port {}, but MeowServer's runs on {}. Change the port in the project's settings, or run a version on {} (Databases page).",
                    &[family.name(), &port.to_string(), &served.join(", "), &port.to_string()],
                ),
                String::new(),
                String::new(),
            )
        }
    }
}

pub struct DbExtraInfo {
    pub line: String,
    pub enabled: bool,
    pub running: bool,
    pub port: Option<u16>,
}

pub struct DbInfo {
    pub unit: Unit,
    pub name: &'static str,
    pub version: String,
    pub port: u16,
    pub running: bool,
    pub data_dir: String,
    pub user: &'static str,
    pub extras: Vec<DbExtraInfo>,
}

pub struct Snapshot {
    pub headline: String,
    pub any_running: bool,
    pub services: Vec<ServiceInfo>,
    pub sites: Vec<Site>,
    pub php_lines: Vec<String>,
    pub databases: Vec<DbInfo>,
    pub needs_setup: bool,
    pub localhost_url: String,
    pub config: Config,
}

pub fn take(layout: &Layout) -> Snapshot {
    let config = Config::load(layout).unwrap_or_default();
    let status: Option<Status> = Client::connect(layout)
        .and_then(|mut c| c.call(Request::Status))
        .ok()
        .and_then(|v| serde_json::from_value(v).ok());
    let daemon_up = status.is_some();
    let status =
        status.unwrap_or(Status { units: vec![], processes: vec![], sites: vec![], warnings: vec![] });
    let sites = if daemon_up { status.sites.clone() } else { site::discover(layout, &config).sites };

    let web_kind = web::kind(&config);
    let state_of = |unit: Unit| -> (String, bool, bool) {
        let procs: Vec<_> = status.processes.iter().filter(|p| unit.owns(&p.id)).collect();
        if let Some(p) = procs.iter().find(|p| matches!(p.state, State::Failed { .. })) {
            if let State::Failed { reason } = &p.state {
                return (reason.clone(), false, true);
            }
        }
        let running = procs.iter().filter(|p| matches!(p.state, State::Running { .. })).count();
        match running {
            0 => (t("stopped"), false, false),
            1 => (t("running"), true, false),
            n => (format!("{} ({n} {})", t("running"), t("processes")), true, false),
        }
    };
    let active =
        |kind: Kind| service::select(layout, kind, config.version_selector(kind.id())).map(|i| i.version);

    let mut services = Vec::new();
    let rows: [(Unit, Kind, String, Option<u16>); 7] = [
        (Unit::Web, web_kind, title(web_kind).into(), Some(config.port("web", 80))),
        (Unit::Php, Kind::Php, "PHP".into(), None),
        (Unit::Mysql, Kind::Mysql, "MySQL".into(), Some(config.port("mysql", mysql::DEFAULT_PORT))),
        (Unit::Mariadb, Kind::Mariadb, "MariaDB".into(), Some(config.port("mariadb", mysql::DEFAULT_PORT))),
        (
            Unit::Postgres,
            Kind::Postgres,
            "PostgreSQL".into(),
            Some(config.port("postgres", postgres::DEFAULT_PORT)),
        ),
        (Unit::Redis, Kind::Redis, "Redis".into(), Some(config.port("redis", redis::DEFAULT_PORT))),
        (Unit::Mailpit, Kind::Mailpit, "Mailpit".into(), Some(config.web_port("mailpit", mailpit::WEB_PORT))),
    ];
    for (unit, kind, name, port) in rows {
        let version = active(kind).map(|v| v.to_string());
        // Hide optional services nobody installed; always show the core three.
        if version.is_none() && !matches!(unit, Unit::Web | Unit::Php | Unit::Mysql) {
            continue;
        }
        let (state, running, failed) = state_of(unit);
        services.push(ServiceInfo {
            unit,
            name,
            version,
            port: port.map(|p| format!(":{p}")).unwrap_or_default(),
            state,
            running,
            failed,
        });
    }

    let mut php_lines: Vec<String> =
        service::installed(layout, Kind::Php).iter().map(|i| i.version.line()).collect();
    php_lines.dedup();

    let mut databases = Vec::new();
    for (unit, kind, name, port, user) in [
        (Unit::Mysql, Kind::Mysql, "MySQL", config.port("mysql", mysql::DEFAULT_PORT), "root"),
        (Unit::Mariadb, Kind::Mariadb, "MariaDB", config.port("mariadb", mysql::DEFAULT_PORT), "root"),
        (
            Unit::Postgres,
            Kind::Postgres,
            "PostgreSQL",
            config.port("postgres", postgres::DEFAULT_PORT),
            "postgres",
        ),
    ] {
        let Some(installed) = service::select(layout, kind, config.version_selector(kind.id())) else {
            continue;
        };
        // Other installed lines of MySQL and MariaDB, which can run too.
        let mut extras = Vec::new();
        if unit != Unit::Postgres {
            let enabled = config.services.get(kind.id()).map(|s| s.extra.clone()).unwrap_or_default();
            let default_line = datadir::line(kind, &installed.version);
            let mut lines: Vec<String> =
                service::installed(layout, kind).iter().map(|i| datadir::line(kind, &i.version)).collect();
            lines.dedup();
            for line in lines.into_iter().filter(|l| *l != default_line) {
                let id = format!("{}-{line}", kind.id());
                let running =
                    status.processes.iter().any(|p| p.id == id && matches!(p.state, State::Running { .. }));
                extras.push(DbExtraInfo {
                    port: enabled.get(&line).copied(),
                    enabled: enabled.contains_key(&line),
                    running,
                    line,
                });
            }
        }
        databases.push(DbInfo {
            unit,
            name,
            version: installed.version.to_string(),
            port,
            running: status.units.contains(&unit),
            data_dir: datadir::path(layout, &config, kind, &installed.version).display().to_string(),
            user,
            extras,
        });
    }

    let running_names: Vec<String> = services.iter().filter(|s| s.running).map(|s| s.name.clone()).collect();
    let headline = if !daemon_up {
        t("MeowServer is not running")
    } else if services.iter().any(|s| s.failed) {
        t("A service failed — see Home")
    } else if running_names.is_empty() {
        t("Stopped")
    } else {
        format!("{}: {}", t("Running"), running_names.join(", "))
    };
    let port = config.port("web", 80);
    let localhost_url =
        if port == 80 { "http://localhost/".into() } else { format!("http://localhost:{port}/") };
    Snapshot {
        headline,
        any_running: !running_names.is_empty(),
        needs_setup: active(web_kind).is_none() || active(Kind::Php).is_none(),
        services,
        sites,
        php_lines,
        databases,
        localhost_url,
        config,
    }
}

pub fn title(kind: Kind) -> &'static str {
    match kind {
        Kind::Caddy => "Caddy",
        Kind::Nginx => "Nginx",
        Kind::Apache => "Apache",
        Kind::Php => "PHP",
        Kind::Mysql => "MySQL",
        Kind::Mariadb => "MariaDB",
        Kind::Postgres => "PostgreSQL",
        Kind::Redis => "Redis",
        Kind::Mailpit => "Mailpit",
        Kind::Node => "Node.js",
        Kind::Composer => "Composer",
    }
}

pub fn framework(f: Framework) -> &'static str {
    match f {
        Framework::Laravel => "Laravel",
        Framework::Symfony => "Symfony",
        Framework::CodeIgniter => "CodeIgniter",
        Framework::Yii => "Yii",
        Framework::Craft => "Craft",
        Framework::CakePhp => "CakePHP",
        Framework::WordPress => "WordPress",
        Framework::Php => "PHP",
        Framework::Static => "Static",
        Framework::Proxy => "Proxy",
    }
}

pub fn source_label(s: &Source) -> &'static str {
    match s {
        Source::Www => "www",
        Source::Parked(_) => "parked",
        Source::Added => "added",
    }
}

pub fn site_url(site: &Site, config: &Config) -> String {
    let port = config.port("web", 80);
    if port == 80 { format!("http://{}/", site.host) } else { format!("http://{}:{port}/", site.host) }
}

pub fn web_index(server: WebServer) -> i32 {
    match server {
        WebServer::Caddy => 0,
        WebServer::Nginx => 1,
        WebServer::Apache => 2,
    }
}

pub fn suffix_index(suffix: Suffix) -> i32 {
    match suffix {
        Suffix::Localhost => 0,
        Suffix::Test => 1,
    }
}
