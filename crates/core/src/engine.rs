// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Turns `meow.toml` and the project folders into running services. This is
//! what `meowd` exposes over IPC; the tray and GUI only call into it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::sync::mpsc::Sender;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::config::{Config, WebServer};
use crate::ipc::{Request, Response};
use crate::layout::Layout;
use crate::platform;
use crate::service::web::Planned;
use crate::service::{Kind, Unit, apache, caddy, mailpit, mysql, nginx, php, postgres, redis};
use crate::site::{self, Site};
use crate::supervisor::{self, Invocation, ProcessSpec, Supervisor};

pub struct Engine {
    layout: Layout,
    supervisor: Supervisor,
    /// Serializes start/stop/reload so they never interleave.
    state: Mutex<Running>,
}

#[derive(Default)]
struct Running {
    units: BTreeMap<Unit, BTreeSet<String>>,
    web_reload: Option<Invocation>,
    warnings: Vec<String>,
    /// Why a unit runs on another port than usual. Kept while the unit
    /// runs: reloads (e.g. after meow.toml changes) must not hide it.
    moved: BTreeMap<Unit, Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Status {
    pub units: Vec<Unit>,
    pub processes: Vec<supervisor::Status>,
    pub sites: Vec<Site>,
    pub warnings: Vec<String>,
}

impl Engine {
    pub fn new(layout: Layout, events: Option<Sender<supervisor::Event>>) -> Self {
        Self { layout, supervisor: Supervisor::new(events), state: Mutex::default() }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn handle(&self, request: Request) -> Response {
        let done = |r: Result<()>| r.map(|()| serde_json::Value::Null);
        let result = match request {
            Request::Status => serde_json::to_value(self.status()).map_err(Into::into),
            Request::StartAll => done(self.start_all()),
            Request::StopAll => {
                self.stop_all();
                done(Ok(()))
            }
            Request::Start { unit } => done(parse_unit(&unit).and_then(|u| self.start(&[u]))),
            Request::Stop { unit } => done(parse_unit(&unit).map(|u| self.stop(u))),
            Request::Restart { unit } => done(parse_unit(&unit).and_then(|u| self.restart(u))),
            Request::Reload => done(self.reload()),
            Request::StartApp { site } => done(self.start_app(&site)),
            Request::StopApp { site } => done(self.stop_app(&site)),
            Request::Shutdown => unreachable!("handled by meowd"),
        };
        match result {
            Ok(data) => Response::ok(data),
            Err(e) => Response::err(format!("{e:#}")),
        }
    }

    /// Ends services from this folder that a killed `meowd` left running,
    /// so this start gets their ports back. Windows ends them with the Job
    /// Object and Linux with `PR_SET_PDEATHSIG`; macOS has neither. Call
    /// once this `meowd` owns the folder's endpoint.
    pub fn end_strays(&self) {
        const SERVICES: &[&str] = &[
            "caddy",
            "nginx",
            "httpd",
            "php-fpm",
            "php-cgi",
            "mysqld",
            "mariadbd",
            "postgres",
            "redis-server",
            "mailpit",
        ];
        let strays: Vec<(u32, String)> = platform::processes_named(SERVICES)
            .into_iter()
            .filter(|(pid, exe)| self.is_ours(*pid, exe))
            .collect();
        for (pid, path) in &strays {
            log::warn!("ending {path} (pid {pid}), left running by an earlier MeowServer");
            platform::terminate(*pid);
        }
        // Databases shut down cleanly on the request; give them time.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while strays.iter().any(|(pid, _)| platform::is_alive(*pid)) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    /// Starts every unit in `general.autostart`.
    pub fn start_all(&self) -> Result<()> {
        let config = Config::load(&self.layout)?;
        let mut units = Vec::new();
        let mut unknown = Vec::new();
        for name in &config.general.autostart {
            match Unit::parse(name) {
                Some(u) => units.push(u),
                None => unknown.push(format!("general.autostart: unknown service {name:?}")),
            }
        }
        let result = self.start(&units);
        self.state.lock().unwrap().warnings.extend(unknown);
        result
    }

    /// Starts the given units in dependency order; running ones are left
    /// alone. Keeps going when one fails and reports all failures.
    pub fn start(&self, units: &[Unit]) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let mut ctx = self.context()?;
        state.warnings = ctx.warnings.clone();
        let mut wanted: BTreeSet<Unit> = units.iter().copied().collect();
        if wanted.contains(&Unit::Web) {
            wanted.insert(Unit::Php);
        }
        // php.ini sends mail() to Mailpit only when Mailpit was installed as
        // PHP started; until PHP starts again, mail goes to the system's
        // sendmail (or nowhere on Windows). Mailpit starts first.
        if wanted.contains(&Unit::Mailpit)
            && !state.units.contains_key(&Unit::Mailpit)
            && state.units.contains_key(&Unit::Php)
            && !wanted.contains(&Unit::Php)
        {
            self.stop_unit(&mut state, Unit::Php);
            wanted.insert(Unit::Php);
        }
        let mut errors = Vec::new();
        for unit in Unit::ALL.into_iter().filter(|u| wanted.contains(u)) {
            if let Err(e) = self.start_unit(&mut state, &mut ctx, unit) {
                log::error!("{}: {e:#}", unit.id());
                errors.push(format!("{}: {e:#}", unit.id()));
            }
        }
        if let Err(e) = crate::dbtool::prepare(&self.layout, &ctx.config) {
            log::warn!("database tools: {e:#}");
        }
        // Compile the sites' PHP before anyone opens them (PRD F47).
        if state.units.contains_key(&Unit::Php) {
            crate::warm::spawn(self.layout.clone(), ctx.config.clone(), ctx.sites.clone());
        }
        if errors.is_empty() { Ok(()) } else { Err(anyhow!(errors.join("\n"))) }
    }

    pub fn stop(&self, unit: Unit) {
        let mut state = self.state.lock().unwrap();
        self.stop_unit(&mut state, unit);
    }

    pub fn restart(&self, unit: Unit) -> Result<()> {
        {
            let mut state = self.state.lock().unwrap();
            self.stop_unit(&mut state, unit);
        }
        self.start(&[unit])
    }

    /// Starts a project's development server on the port its site name
    /// forwards to (PRD F43). That port is saved the first time, so the
    /// project's own settings can name it.
    pub fn start_app(&self, name: &str) -> Result<()> {
        let _state = self.state.lock().unwrap();
        let mut config = Config::load(&self.layout)?;
        let site = site::discover(&self.layout, &config)
            .sites
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("no site named {name}"))?;
        let port = site.app_port.ok_or_else(|| anyhow!("{name} does not run its own server"))?;
        let spec = crate::app::plan(&self.layout, &config, &site)?;
        if self.supervisor.is_running(&spec.id) {
            return Ok(());
        }
        self.ensure_port_free(&format!("{name}'s dev server"), port)?;
        if config.sites.get(name).and_then(|s| s.port).is_none() {
            site::manage::settings(&mut config, name).port = Some(port);
            config.save(&self.layout)?;
        }
        self.supervisor.start(spec)
    }

    pub fn stop_app(&self, name: &str) -> Result<()> {
        let id = crate::app::process_id(name);
        if self.supervisor.is_running(&id) { self.supervisor.stop(&id) } else { Ok(()) }
    }

    pub fn stop_all(&self) {
        let mut state = self.state.lock().unwrap();
        self.supervisor.stop_all();
        state.units.clear();
        state.web_reload = None;
    }

    /// Regenerates configs after `meow.toml` or project folders change:
    /// starts PHP pools that became needed, stops unneeded ones, and makes
    /// the web server pick up the new sites without dropping connections
    /// where it can.
    pub fn reload(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if !state.units.contains_key(&Unit::Web) && !state.units.contains_key(&Unit::Php) {
            return Ok(());
        }
        let ctx = self.context()?;
        state.warnings = ctx.warnings.clone();
        let php_plan = php::plan(&self.layout, &ctx.config, &ctx.sites)?;
        state.warnings.extend(php_plan.warnings.iter().cloned());

        let wanted: BTreeSet<String> =
            php_plan.pools.iter().flat_map(|p| &p.specs).map(|s| s.id.clone()).collect();
        let current = state.units.remove(&Unit::Php).unwrap_or_default();
        for gone in current.difference(&wanted) {
            let _ = self.supervisor.stop(gone);
        }
        for spec in php_plan.pools.iter().flat_map(|p| p.specs.iter().cloned()) {
            self.supervisor.start(spec)?;
        }
        state.units.insert(Unit::Php, wanted);

        if let Some(web_ids) = state.units.get(&Unit::Web).cloned() {
            let planned = plan_web(&self.layout, &ctx.config, &ctx.sites, &php_plan)?;
            let same_server = web_ids.contains(&planned.spec.id);
            let reloaded = same_server
                && self.supervisor.is_running(&planned.spec.id)
                && planned.reload.as_ref().is_some_and(supervisor::run);
            if !reloaded {
                for id in &web_ids {
                    let _ = self.supervisor.stop(id);
                }
                self.supervisor.start(planned.spec.clone())?;
            }
            state.units.insert(Unit::Web, BTreeSet::from([planned.spec.id.clone()]));
            state.web_reload = planned.reload;
        }
        if let Err(e) = crate::dbtool::prepare(&self.layout, &ctx.config) {
            log::warn!("database tools: {e:#}");
        }
        // New sites, or new Composer packages: compile them in the background.
        crate::warm::spawn(self.layout.clone(), ctx.config.clone(), ctx.sites.clone());
        Ok(())
    }

    pub fn status(&self) -> Status {
        let state = self.state.lock().unwrap();
        let config = Config::load(&self.layout).unwrap_or_default();
        let discovery = site::discover(&self.layout, &config);
        let mut warnings = discovery.warnings;
        for w in state.warnings.iter().chain(state.moved.values().flatten()) {
            if !warnings.contains(w) {
                warnings.push(w.clone());
            }
        }
        Status {
            units: state.units.keys().copied().collect(),
            processes: self.supervisor.status(),
            sites: discovery.sites,
            warnings,
        }
    }

    pub fn running_units(&self) -> Vec<Unit> {
        self.state.lock().unwrap().units.keys().copied().collect()
    }

    fn context(&self) -> Result<Context> {
        self.layout.ensure()?;
        let config = Config::load(&self.layout)?;
        std::fs::create_dir_all(self.layout.resolve(&config.general.www))?;
        let discovery = site::discover(&self.layout, &config);
        Ok(Context { config, sites: discovery.sites, warnings: discovery.warnings })
    }

    fn start_unit(&self, state: &mut Running, ctx: &mut Context, unit: Unit) -> Result<()> {
        // Units started after this one (the web server after PHP) plan with
        // the moved ports too.
        if let Some(moved) = self.move_off_busy_ports(state, &ctx.config, &ctx.sites, unit)? {
            ctx.config = moved;
        }
        let config = &ctx.config;
        let specs: Vec<ProcessSpec> = match unit {
            Unit::Mysql | Unit::Mariadb => {
                let kind = if unit == Unit::Mysql { Kind::Mysql } else { Kind::Mariadb };
                let (specs, warnings) = mysql::plan(&self.layout, config, kind)?;
                state.warnings.extend(warnings);
                // The default server's port is checked below with the others.
                for (line, port) in extra_versions(config, kind) {
                    let id = format!("{}-{line}", kind.id());
                    if specs.iter().any(|s| s.id == id) && !self.supervisor.is_running(&id) {
                        self.ensure_port_free(&format!("{} {line}", kind.id()), port)?;
                    }
                }
                specs
            }
            Unit::Postgres => vec![postgres::plan(&self.layout, config)?],
            Unit::Redis => vec![redis::plan(&self.layout, config)?],
            Unit::Mailpit => vec![mailpit::plan(&self.layout, config)?],
            Unit::Php => {
                let plan = php::plan(&self.layout, config, &ctx.sites)?;
                state.warnings.extend(plan.warnings.iter().cloned());
                for pool in &plan.pools {
                    if pool.specs.iter().any(|s| !self.supervisor.is_running(&s.id)) {
                        for port in pool.ports() {
                            self.ensure_port_free("php", port)?;
                        }
                    }
                }
                plan.pools.into_iter().flat_map(|p| p.specs).collect()
            }
            Unit::Web => {
                let php_plan = php::plan(&self.layout, config, &ctx.sites)?;
                let planned = plan_web(&self.layout, config, &ctx.sites, &php_plan)?;
                if planned.spec.id == "caddy" && !self.supervisor.is_running("caddy") {
                    self.ensure_port_free("caddy", config.port("caddy", caddy::DEFAULT_ADMIN_PORT))?;
                }
                state.web_reload = planned.reload;
                vec![planned.spec]
            }
        };
        if let Some((name, port)) = unit_port(config, unit) {
            let fresh = specs.iter().any(|s| !self.supervisor.is_running(&s.id));
            if fresh {
                self.ensure_port_free(name, port)?;
            }
        }
        let ids = state.units.entry(unit).or_default();
        for spec in specs {
            let id = spec.id.clone();
            self.supervisor.start(spec)?;
            ids.insert(id);
        }
        if unit == Unit::Mariadb {
            mysql::after_start(&self.layout, &ctx.config, crate::service::Kind::Mariadb);
        }
        Ok(())
    }

    /// Refuses to start when another program holds the port, on any
    /// address, and names it (PRD F9, §8.7).
    fn ensure_port_free(&self, name: &str, port: u16) -> Result<()> {
        let foreign = self.foreign_owners(port);
        if foreign.is_empty() {
            return Ok(());
        }
        Err(anyhow!(
            "port {port} is used by {}. Stop that program, or give {name} another port in meow.toml ([services.{name}] port = …).",
            foreign.join(" and ")
        ))
    }

    /// A process of this MeowServer folder: its program is inside the
    /// folder, or it was started with the folder's settings. The second
    /// covers a build linked into bin/ (a Homebrew keg), whose executable
    /// the OS reports at the link's target, outside the folder.
    fn is_ours(&self, pid: u32, exe: &str) -> bool {
        let root = self.layout.root();
        if platform::is_inside(exe, root) {
            return true;
        }
        let Some(line) = platform::command_line(pid) else { return false };
        let real = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        [root, real.as_path()].iter().any(|r| {
            let r = r.to_string_lossy();
            // Followed by a separator: not another folder whose name
            // starts the same (MeowServer2).
            line.contains(&format!("{r}/")) || line.contains(&format!("{r}\\"))
        })
    }

    /// Programs from outside this MeowServer folder listening on `port`.
    fn foreign_owners(&self, port: u16) -> Vec<String> {
        name_owners(self.foreign_processes(port))
    }

    /// What holds `port` besides `unit` itself: other programs, and
    /// MeowServer's other services (MySQL and MariaDB both default to 3306).
    fn other_owners(&self, unit: Unit, port: u16) -> Vec<String> {
        let own: Vec<u32> = self
            .supervisor
            .status()
            .into_iter()
            .filter(|p| unit.owns(&p.id))
            .filter_map(|p| match p.state {
                supervisor::State::Running { pid } => Some(pid),
                _ => None,
            })
            .collect();
        name_owners(platform::port_owners(port).into_iter().filter(|(pid, _)| !own.contains(pid)).collect())
    }

    fn foreign_processes(&self, port: u16) -> Vec<(u32, String)> {
        platform::port_owners(port).into_iter().filter(|(pid, exe)| !self.is_ours(*pid, exe)).collect()
    }

    /// When another program (Laragon's MySQL, another server on port 80,
    /// a second MeowServer folder) holds one of `unit`'s ports, moves the
    /// service to the next free port and saves that in meow.toml. Saved
    /// rather than chosen on each start, so a project configured for the
    /// new port keeps working after the other program quits. Returns the
    /// configuration to plan with when something moved; with no free port,
    /// `ensure_port_free` reports the clash.
    fn move_off_busy_ports(
        &self,
        state: &mut Running,
        config: &Config,
        sites: &[Site],
        unit: Unit,
    ) -> Result<Option<Config>> {
        let mut moved = config.clone();
        let mut changes: Vec<Change> = Vec::new();
        let mut notes = Vec::new();
        for (service, field, default, first) in listening_ports(unit) {
            if field == PortField::Tls && !config.general.https {
                continue;
            }
            if service == "caddy" && config.general.web_server != WebServer::Caddy {
                continue;
            }
            let current = field.get(&moved, service, default);
            let original = field.moved_from(&moved, service);
            // Moved earlier because something held the usual port, and it
            // is free again (Laragon quit): go back, so projects set up for
            // that port, like Laragon's, work again.
            if let Some(original) = original
                && original != current
                && port_free(original)
            {
                change(&mut moved, &mut changes, Change::Port { service, field, port: original, from: None });
                notes.push(format!(
                    "{service}: port {original} is free again, so MeowServer's {service} is back on it"
                ));
                continue;
            }
            let owners = self.other_owners(unit, current);
            if owners.is_empty() {
                // Nobody holds it, yet it cannot be opened: macOS lets only
                // root bind ports below 1024 to 127.0.0.1. Moving is for
                // good, as waiting would not help.
                if !needs_admin(config, current) {
                    continue;
                }
                let taken = configured_ports(&moved);
                let Some(free) =
                    (first..first.saturating_add(200)).find(|p| !taken.contains(p) && port_free(*p))
                else {
                    continue;
                };
                change(&mut moved, &mut changes, Change::Port { service, field, port: free, from: None });
                notes.push(format!(
                    "{service}: port {current} can only be opened with administrator rights, so MeowServer's \
                     {service} uses port {free} (saved in meow.toml)"
                ));
                continue;
            }
            let taken = configured_ports(&moved);
            let Some(free) = (first..first.saturating_add(200)).find(|p| !taken.contains(p) && port_free(*p))
            else {
                continue;
            };
            // MeowServer's own other service (MySQL beside MariaDB): stay
            // on the new port, or the two would swap whenever one stops.
            if self.foreign_owners(current).is_empty() {
                change(&mut moved, &mut changes, Change::Port { service, field, port: free, from: original });
                notes.push(format!(
                    "{service}: port {current} is used by {}, so MeowServer's {service} uses port {free} \
                     (saved in meow.toml)",
                    owners.join(" and "),
                ));
                continue;
            }
            let from = Some(original.unwrap_or(current));
            change(&mut moved, &mut changes, Change::Port { service, field, port: free, from });
            notes.push(format!(
                "{service}: port {current} is used by {}, so MeowServer's {service} uses port {free} for now \
                 (it goes back to {current} when that port is free)",
                owners.join(" and "),
            ));
        }
        if let Some(kind) = database_kind(unit) {
            for (line, port) in extra_versions(&moved, kind) {
                let owners = self.other_owners(unit, port);
                if owners.is_empty() {
                    continue;
                }
                let taken = configured_ports(&moved);
                let Some(free) =
                    (port + 1..port.saturating_add(200)).find(|p| !taken.contains(p) && port_free(*p))
                else {
                    continue;
                };
                notes.push(format!(
                    "{} {line}: port {port} is used by {}, so it now uses port {free} (saved in meow.toml)",
                    kind.id(),
                    owners.join(" and ")
                ));
                change(&mut moved, &mut changes, Change::Extra { kind, line, port: free });
            }
        }
        if unit == Unit::Php
            && let Some((base, owners)) = self.free_php_base(&moved, sites)
        {
            let port_change = Change::Port { service: "php", field: PortField::Port, port: base, from: None };
            change(&mut moved, &mut changes, port_change);
            notes.push(format!(
                "php: its ports are used by {}, so MeowServer's PHP now uses ports from {base} \
                 (saved in meow.toml as [services.php] port)",
                owners.join(" and ")
            ));
        }
        if changes.is_empty() {
            return Ok(None);
        }
        // Write only these changes onto meow.toml as it is now.
        let mut saved = Config::load(&self.layout)?;
        for c in changes {
            c.apply(&mut saved);
        }
        saved.save(&self.layout)?;
        for note in &notes {
            log::warn!("{note}");
        }
        state.moved.insert(unit, notes);
        Ok(Some(moved))
    }

    /// When another program holds one of the PHP pools' FastCGI ports, the
    /// first base (in steps of 1000) where every pool port is free, with
    /// the programs that were in the way. `None` when nothing is in the way
    /// or no base is free; `ensure_port_free` then has the last word.
    fn free_php_base(&self, config: &Config, sites: &[Site]) -> Option<(u16, Vec<String>)> {
        let ports = |config: &Config| -> Vec<u16> {
            php::plan(&self.layout, config, sites)
                .map(|plan| plan.pools.iter().flat_map(|p| p.ports()).collect())
                .unwrap_or_default()
        };
        // A pool is many processes of one program: name the program once.
        let mut owners: Vec<String> =
            ports(config).into_iter().flat_map(|p| self.foreign_processes(p)).map(|(_, exe)| exe).collect();
        if owners.is_empty() {
            return None;
        }
        owners.sort();
        owners.dedup();
        let current = config.port("php", php::DEFAULT_PORT_BASE);
        (1..=40u16)
            .map(|step| php::DEFAULT_PORT_BASE + step * 1000)
            .take_while(|base| *base <= 64_000)
            .filter(|base| *base != current)
            .find(|base| {
                let mut candidate = config.clone();
                PortField::Port.set(&mut candidate, "php", *base);
                ports(&candidate).into_iter().all(port_free)
            })
            .map(|base| (base, owners))
    }

    fn stop_unit(&self, state: &mut Running, unit: Unit) {
        state.moved.remove(&unit);
        let mut ids: BTreeSet<String> = state.units.remove(&unit).unwrap_or_default();
        // Include strays, e.g. a process started before a config change.
        ids.extend(self.supervisor.status().into_iter().map(|s| s.id).filter(|id| unit.owns(id)));
        std::thread::scope(|s| {
            for id in &ids {
                s.spawn(move || {
                    let _ = self.supervisor.stop(id);
                });
            }
        });
        if unit == Unit::Web {
            state.web_reload = None;
        }
    }
}

struct Context {
    config: Config,
    sites: Vec<Site>,
    warnings: Vec<String>,
}

/// The port a unit listens on, under its `[services.<name>]` key.
fn unit_port(config: &Config, unit: Unit) -> Option<(&'static str, u16)> {
    use crate::service::{mailpit, mysql, postgres, redis};
    Some(match unit {
        Unit::Web => ("web", config.port("web", 80)),
        Unit::Mysql => ("mysql", config.port("mysql", mysql::DEFAULT_PORT)),
        Unit::Mariadb => ("mariadb", config.port("mariadb", mysql::DEFAULT_PORT)),
        Unit::Postgres => ("postgres", config.port("postgres", postgres::DEFAULT_PORT)),
        Unit::Redis => ("redis", config.port("redis", redis::DEFAULT_PORT)),
        Unit::Mailpit => ("mailpit", config.port("mailpit", mailpit::SMTP_PORT)),
        Unit::Php => return None,
    })
}

/// A port change made while starting, applied to the plan and to meow.toml.
enum Change {
    /// `from`: the usual port to return to once free; `None` clears it.
    Port {
        service: &'static str,
        field: PortField,
        port: u16,
        from: Option<u16>,
    },
    Extra {
        kind: Kind,
        line: String,
        port: u16,
    },
}

impl Change {
    fn apply(&self, config: &mut Config) {
        match self {
            Change::Port { service, field, port, from } => {
                field.set(config, service, *port);
                let settings = config.services.entry((*service).into()).or_default();
                match from {
                    Some(from) => settings.moved_from.insert(field.key().into(), *from),
                    None => settings.moved_from.remove(field.key()),
                };
            }
            Change::Extra { kind, line, port } => {
                config.services.entry(kind.id().into()).or_default().extra.insert(line.clone(), *port);
            }
        }
    }
}

fn change(config: &mut Config, changes: &mut Vec<Change>, c: Change) {
    c.apply(config);
    changes.push(c);
}

/// Every port this configuration listens on that people connect to,
/// extra database versions included.
pub fn configured_ports(config: &Config) -> BTreeSet<u16> {
    let mut ports: BTreeSet<u16> =
        Unit::ALL.into_iter().flat_map(listening_ports).map(|(s, f, d, _)| f.get(config, s, d)).collect();
    for kind in [Kind::Mysql, Kind::Mariadb] {
        ports.extend(extra_versions(config, kind).into_iter().map(|(_, port)| port));
    }
    ports
}

/// The next port from `from` that nothing listens on and this
/// configuration does not use, for a new extra database version.
pub fn free_port(config: &Config, from: u16) -> Option<u16> {
    let taken = configured_ports(config);
    (from..from.saturating_add(200)).find(|p| !taken.contains(p) && port_free(*p))
}

fn extra_versions(config: &Config, kind: Kind) -> Vec<(String, u16)> {
    config.services.get(kind.id()).map(|s| s.extra.clone().into_iter().collect()).unwrap_or_default()
}

fn database_kind(unit: Unit) -> Option<Kind> {
    match unit {
        Unit::Mysql => Some(Kind::Mysql),
        Unit::Mariadb => Some(Kind::Mariadb),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortField {
    Port,
    Tls,
    Web,
}

impl PortField {
    fn get(self, config: &Config, service: &str, default: u16) -> u16 {
        match self {
            PortField::Port => config.port(service, default),
            PortField::Tls => config.tls_port(service, default),
            PortField::Web => config.web_port(service, default),
        }
    }

    fn set(self, config: &mut Config, service: &str, port: u16) {
        let settings = config.services.entry(service.into()).or_default();
        match self {
            PortField::Port => settings.port = Some(port),
            PortField::Tls => settings.tls_port = Some(port),
            PortField::Web => settings.web_port = Some(port),
        }
    }

    /// The usual port this field had before MeowServer moved it.
    fn moved_from(self, config: &Config, service: &str) -> Option<u16> {
        config.services.get(service).and_then(|s| s.moved_from.get(self.key())).copied()
    }

    fn key(self) -> &'static str {
        match self {
            PortField::Port => "port",
            PortField::Tls => "tls_port",
            PortField::Web => "web_port",
        }
    }
}

/// The ports a unit listens on that people and projects connect to, as
/// (service, field, default, first port to try when it is taken).
fn listening_ports(unit: Unit) -> Vec<(&'static str, PortField, u16, u16)> {
    use PortField::{Port, Tls, Web};
    match unit {
        Unit::Web => vec![
            ("web", Port, 80, 8080),
            ("web", Tls, 443, 8443),
            // Internal, but a second MeowServer folder wants the same one.
            // 19990–: between the default PHP ports and moved PHP bases.
            ("caddy", Port, caddy::DEFAULT_ADMIN_PORT, 19_990),
        ],
        Unit::Mysql => vec![("mysql", Port, mysql::DEFAULT_PORT, 3307)],
        Unit::Mariadb => vec![("mariadb", Port, mysql::DEFAULT_PORT, 3307)],
        Unit::Postgres => vec![("postgres", Port, postgres::DEFAULT_PORT, 5433)],
        Unit::Redis => vec![("redis", Port, redis::DEFAULT_PORT, 6380)],
        Unit::Mailpit => {
            vec![("mailpit", Port, mailpit::SMTP_PORT, 1026), ("mailpit", Web, mailpit::WEB_PORT, 8026)]
        }
        // Internal ports, configurable but never seen by projects.
        Unit::Php => Vec::new(),
    }
}

/// Nobody listens on `port` and it can be bound: Windows reserves port
/// ranges (Hyper-V, WSL) that have no listener yet refuse to bind.
fn port_free(port: u16) -> bool {
    platform::port_owners(port).is_empty() && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// "nginx (pid 1065, 1066)": a server's worker processes share one entry.
fn name_owners(processes: Vec<(u32, String)>) -> Vec<String> {
    let mut by_exe: Vec<(String, Vec<String>)> = Vec::new();
    for (pid, exe) in processes {
        match by_exe.iter_mut().find(|(e, _)| *e == exe) {
            Some((_, pids)) => pids.push(pid.to_string()),
            None => by_exe.push((exe, vec![pid.to_string()])),
        }
    }
    by_exe.into_iter().map(|(exe, pids)| format!("{exe} (pid {})", pids.join(", "))).collect()
}

/// The address services listen on refuses `port` for lack of rights.
fn needs_admin(config: &Config, port: u16) -> bool {
    let address = if config.general.lan_access { "0.0.0.0" } else { "127.0.0.1" };
    matches!(
        std::net::TcpListener::bind((address, port)),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied
    )
}

fn parse_unit(name: &str) -> Result<Unit> {
    Unit::parse(name).ok_or_else(|| {
        let known: Vec<&str> = Unit::ALL.iter().map(|u| u.id()).collect();
        anyhow!("unknown service {name:?}; known: {}", known.join(", "))
    })
}

fn plan_web(layout: &Layout, config: &Config, sites: &[Site], php: &php::Plan) -> Result<Planned> {
    match config.general.web_server {
        WebServer::Caddy => caddy::plan(layout, config, sites, php),
        WebServer::Nginx => nginx::plan(layout, config, sites, php),
        WebServer::Apache => apache::plan(layout, config, sites, php),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_are_named_once_per_program() {
        let owners = name_owners(vec![
            (1065, "/Applications/Herd.app/nginx".into()),
            (900, "/opt/homebrew/bin/mysqld".into()),
            (1066, "/Applications/Herd.app/nginx".into()),
        ]);
        assert_eq!(
            owners,
            ["/Applications/Herd.app/nginx (pid 1065, 1066)", "/opt/homebrew/bin/mysqld (pid 900)"]
        );
        assert!(name_owners(Vec::new()).is_empty());
    }
}
