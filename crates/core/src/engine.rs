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
use crate::service::{Unit, apache, caddy, mailpit, mysql, nginx, php, postgres, redis};
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
            Request::Shutdown => unreachable!("handled by meowd"),
        };
        match result {
            Ok(data) => Response::ok(data),
            Err(e) => Response::err(format!("{e:#}")),
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
        let ctx = self.context()?;
        state.warnings = ctx.warnings.clone();
        let mut wanted: BTreeSet<Unit> = units.iter().copied().collect();
        if wanted.contains(&Unit::Web) {
            wanted.insert(Unit::Php);
        }
        let mut errors = Vec::new();
        for unit in Unit::ALL.into_iter().filter(|u| wanted.contains(u)) {
            if let Err(e) = self.start_unit(&mut state, &ctx, unit) {
                log::error!("{}: {e:#}", unit.id());
                errors.push(format!("{}: {e:#}", unit.id()));
            }
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

    fn start_unit(&self, state: &mut Running, ctx: &Context, unit: Unit) -> Result<()> {
        let moved = self.move_off_busy_ports(state, &ctx.config, unit)?;
        let config = moved.as_ref().unwrap_or(&ctx.config);
        let specs: Vec<ProcessSpec> = match unit {
            Unit::Mysql => vec![mysql::plan(&self.layout, config, crate::service::Kind::Mysql)?],
            Unit::Mariadb => vec![mysql::plan(&self.layout, config, crate::service::Kind::Mariadb)?],
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

    /// Programs from outside this MeowServer folder listening on `port`.
    fn foreign_owners(&self, port: u16) -> Vec<String> {
        let root = self.layout.root().to_string_lossy().to_lowercase();
        platform::port_owners(port)
            .into_iter()
            .filter(|(_, exe)| !exe.to_lowercase().starts_with(&root))
            .map(|(pid, exe)| format!("{exe} (pid {pid})"))
            .collect()
    }

    /// When another program (Laragon's MySQL, another server on port 80)
    /// holds one of `unit`'s ports, moves the service to the next free port
    /// and saves that in meow.toml. Saved rather than chosen on each start,
    /// so a project configured for the new port keeps working after the
    /// other program quits. Returns the configuration to plan with when
    /// something moved; with no free port, `ensure_port_free` reports it.
    fn move_off_busy_ports(
        &self,
        state: &mut Running,
        config: &Config,
        unit: Unit,
    ) -> Result<Option<Config>> {
        let mut moved = config.clone();
        let mut notes = Vec::new();
        for (service, field, default, first) in listening_ports(unit) {
            if field == PortField::Tls && !config.general.https {
                continue;
            }
            let current = field.get(&moved, service, default);
            let owners = self.foreign_owners(current);
            if owners.is_empty() {
                continue;
            }
            let taken: BTreeSet<u16> = Unit::ALL
                .into_iter()
                .flat_map(listening_ports)
                .map(|(s, f, d, _)| f.get(&moved, s, d))
                .collect();
            let Some(free) = (first..first.saturating_add(200))
                .find(|p| !taken.contains(p) && platform::port_owners(*p).is_empty() && can_bind(*p))
            else {
                continue;
            };
            field.set(&mut moved, service, free);
            notes.push(format!(
                "{service}: port {current} is used by {}, so MeowServer's {service} now uses port {free} \
                 (saved in meow.toml; set [services.{service}] {} = {current} there to go back)",
                owners.join(" and "),
                field.key(),
            ));
        }
        if notes.is_empty() {
            return Ok(None);
        }
        // Write only the moved ports onto meow.toml as it is now.
        let mut saved = Config::load(&self.layout)?;
        for (service, field, default, _) in listening_ports(unit) {
            let port = field.get(&moved, service, default);
            if port != field.get(config, service, default) {
                field.set(&mut saved, service, port);
            }
        }
        saved.save(&self.layout)?;
        for note in &notes {
            log::warn!("{note}");
        }
        state.moved.insert(unit, notes);
        Ok(Some(moved))
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
        Unit::Web => vec![("web", Port, 80, 8080), ("web", Tls, 443, 8443)],
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

/// Windows reserves port ranges (Hyper-V, WSL) that nobody listens on but
/// nobody may bind either, so "no listener" is not enough.
fn can_bind(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
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
