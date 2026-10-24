// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `meow doctor` (PRD F32): finds what will stop MeowServer from working
//! and says how to fix it.

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::layout::Layout;
use crate::service::{self, Kind, Unit, datadir, mailpit, mysql, postgres, redis, web};
use crate::{hosts, platform, site};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
    pub fix: Option<String>,
}

impl Check {
    fn ok(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { name: name.into(), ok: true, detail: detail.into(), fix: None }
    }

    fn problem(name: impl Into<String>, detail: impl Into<String>, fix: impl Into<String>) -> Self {
        Self { name: name.into(), ok: false, detail: detail.into(), fix: Some(fix.into()) }
    }
}

pub fn run(layout: &Layout) -> Vec<Check> {
    let mut checks = Vec::new();
    let config = match Config::load(layout) {
        Ok(c) => c,
        Err(e) => {
            checks.push(Check::problem(
                "meow.toml",
                format!("{e:#}"),
                "Fix the file or delete it to use defaults.",
            ));
            return checks;
        }
    };
    let units: Vec<Unit> = config.general.autostart.iter().filter_map(|n| Unit::parse(n)).collect();

    // Installed binaries for everything that starts automatically.
    let mut needed = vec![];
    for unit in &units {
        match unit {
            Unit::Web => needed.extend([web::kind(&config), Kind::Php]),
            Unit::Php => needed.push(Kind::Php),
            Unit::Mysql => needed.push(Kind::Mysql),
            Unit::Mariadb => needed.push(Kind::Mariadb),
            Unit::Postgres => needed.push(Kind::Postgres),
            Unit::Redis => needed.push(Kind::Redis),
            Unit::Mailpit => needed.push(Kind::Mailpit),
        }
    }
    needed.dedup();
    for kind in needed {
        let selector = config.version_selector(kind.id());
        match service::select(layout, kind, selector) {
            Some(i) => checks.push(Check::ok(format!("{} installed", kind.id()), i.version.to_string())),
            None => checks.push(Check::problem(
                format!("{} installed", kind.id()),
                format!(
                    "{} {}is not installed",
                    kind.id(),
                    selector.map(|s| format!("{s} ")).unwrap_or_default()
                ),
                format!("meow install {} {}", kind.id(), selector.unwrap_or("")).trim_end().to_owned(),
            )),
        }
    }

    // Ports, unless the process holding them is one of ours.
    let mut ports: Vec<(String, u16)> = Vec::new();
    for unit in &units {
        match unit {
            Unit::Web => ports.push(("web".into(), config.port("web", 80))),
            Unit::Mysql => ports.push(("mysql".into(), config.port("mysql", mysql::DEFAULT_PORT))),
            Unit::Mariadb => ports.push(("mariadb".into(), config.port("mariadb", mysql::DEFAULT_PORT))),
            Unit::Postgres => {
                ports.push(("postgres".into(), config.port("postgres", postgres::DEFAULT_PORT)))
            }
            Unit::Redis => ports.push(("redis".into(), config.port("redis", redis::DEFAULT_PORT))),
            Unit::Mailpit => {
                ports.push(("mailpit smtp".into(), config.port("mailpit", mailpit::SMTP_PORT)));
                ports.push(("mailpit web".into(), config.web_port("mailpit", mailpit::WEB_PORT)));
            }
            Unit::Php => {}
        }
    }
    let root = layout.root().to_string_lossy().to_lowercase();
    for (name, port) in &ports {
        if let Some(clash) = ports.iter().find(|(n, p)| p == port && n < name) {
            checks.push(Check::problem(
                format!("port {port}"),
                format!("{name} and {} are both set to port {port}", clash.0),
                format!(
                    "Give one of them another port in meow.toml, e.g. [services.{name}] port = {}",
                    port + 1
                ),
            ));
            continue;
        }
        let owners = platform::port_owners(*port);
        let foreign: Vec<String> = owners
            .iter()
            .filter(|(_, exe)| !exe.to_lowercase().starts_with(&root))
            .map(|(pid, exe)| format!("{exe} (pid {pid})"))
            .collect();
        if !foreign.is_empty() {
            checks.push(Check::problem(
                format!("port {port}"),
                format!("needed by {name}, but used by {}", foreign.join(" and ")),
                format!("Stop that program, or set another port in meow.toml: [services.{name}] port = …"),
            ));
        } else if owners.is_empty() {
            checks.push(Check::ok(format!("port {port}"), format!("free for {name}")));
        } else {
            checks.push(Check::ok(format!("port {port}"), format!("used by MeowServer's {name}")));
        }
    }

    // Database folders.
    for (unit, kind) in
        [(Unit::Mysql, Kind::Mysql), (Unit::Mariadb, Kind::Mariadb), (Unit::Postgres, Kind::Postgres)]
    {
        if !units.contains(&unit) {
            continue;
        }
        let Some(installed) = service::select(layout, kind, config.version_selector(kind.id())) else {
            continue;
        };
        let dir = datadir::path(layout, &config, kind, &installed.version);
        let others: Vec<String> = std::fs::read_dir(layout.data())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p != &dir
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with(&format!("{}-", kind.id())))
            })
            .filter(|p| matches!(datadir::detect(p), datadir::Found::Data { .. }))
            .map(|p| p.display().to_string())
            .collect();
        if !others.is_empty() {
            checks.push(Check::problem(
                format!("{} data", kind.id()),
                format!("data from another {} version is not in use: {}", kind.id(), others.join(", ")),
                format!(
                    "To keep using it: meow use {} <that version>. To move it to {}: meow db upgrade {} {}",
                    kind.id(),
                    installed.version.line(),
                    kind.id(),
                    installed.version.line()
                ),
            ));
        }
        match datadir::check(&dir, kind, &installed.version) {
            Ok(true) => {
                checks.push(Check::ok(format!("{} data", kind.id()), "will be created on first start"))
            }
            Ok(false) => checks.push(Check::ok(format!("{} data", kind.id()), dir.display().to_string())),
            Err(e) => checks.push(Check::problem(
                format!("{} data", kind.id()),
                format!("{e:#}"),
                "See the message above.",
            )),
        }
    }

    // Sites and hosts entries.
    let discovery = site::discover(layout, &config);
    for warning in discovery.warnings {
        checks.push(Check::problem(
            "sites",
            warning,
            "Rename one of the folders or set a name in meow.toml.",
        ));
    }
    if config.general.manage_hosts && !discovery.sites.is_empty() {
        if hosts::is_current(layout, &config) {
            checks.push(Check::ok("hosts file", "entries for all sites are present"));
        } else {
            checks.push(Check::problem(
                "hosts file",
                "some sites have no hosts entry; browsers still work, but PHP and Node cannot reach them",
                "meow hosts sync",
            ));
        }
    }

    if config.general.https && !crate::tls::is_trusted(layout) {
        checks.push(Check::problem(
            "HTTPS",
            "browsers show a warning on https:// sites until the local certificate authority is trusted",
            "meow ssl trust (or Settings → Trust HTTPS certificate)",
        ));
    }

    if crate::defender::realtime_enabled() == Some(true) {
        let missing = crate::defender::missing(layout, &config);
        if missing.is_empty() {
            checks.push(Check::ok("Defender", "MeowServer and project folders are excluded from scanning"));
        } else {
            let list: Vec<String> = missing.iter().map(|p| p.display().to_string()).collect();
            checks.push(Check::problem(
                "Defender",
                format!(
                    "real-time scanning reads every PHP file on first use, which can make the first page \
                     after a restart take many seconds; not excluded: {}",
                    list.join(", ")
                ),
                "meow defender exclude (asks for administrator rights once). On Windows 11, a Dev Drive \
                 for projects is another option.",
            ));
        }
    }

    if platform::is_running("laragon.exe") {
        checks.push(Check::problem(
            "Laragon",
            "Laragon is running and may hold the same ports or database files",
            "Quit Laragon before starting MeowServer, or give MeowServer other ports.",
        ));
    }
    checks
}
