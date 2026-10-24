// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Turns `meow.toml` and the project folders into running services. This is
//! what `meowd` exposes over IPC; the tray and GUI only call into it.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::mpsc::Sender;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{Config, WebServer};
use crate::ipc::{Request, Response};
use crate::layout::Layout;
use crate::service::{self, caddy, php};
use crate::site::{self, Site};
use crate::supervisor::{self, ProcessSpec, Supervisor};

pub struct Engine {
    layout: Layout,
    supervisor: Supervisor,
    /// Serializes start/stop/reload so they never interleave.
    busy: Mutex<Running>,
}

#[derive(Default)]
struct Running {
    ids: BTreeSet<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Status {
    pub processes: Vec<supervisor::Status>,
    pub sites: Vec<Site>,
    pub warnings: Vec<String>,
}

impl Engine {
    pub fn new(layout: Layout, events: Option<Sender<supervisor::Event>>) -> Self {
        Self { layout, supervisor: Supervisor::new(events), busy: Mutex::default() }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn handle(&self, request: Request) -> Response {
        let result = match request {
            Request::Status => serde_json::to_value(self.status()).map_err(Into::into),
            Request::StartAll => self.start_all().map(|()| serde_json::Value::Null),
            Request::StopAll => {
                self.stop_all();
                Ok(serde_json::Value::Null)
            }
            Request::Reload => self.reload().map(|()| serde_json::Value::Null),
            Request::Shutdown => unreachable!("handled by meowd"),
        };
        match result {
            Ok(data) => Response::ok(data),
            Err(e) => Response::err(format!("{e:#}")),
        }
    }

    /// Starts everything listed in `general.autostart`. Already running
    /// processes are left alone.
    pub fn start_all(&self) -> Result<()> {
        let mut running = self.busy.lock().unwrap();
        let (specs, warnings) = self.plan()?;
        running.warnings = warnings;
        for spec in specs {
            let id = spec.id.clone();
            self.supervisor.start(spec)?;
            running.ids.insert(id);
        }
        Ok(())
    }

    pub fn stop_all(&self) {
        let mut running = self.busy.lock().unwrap();
        self.supervisor.stop_all();
        running.ids.clear();
    }

    /// Regenerates configs and restarts what changed. PHP pools that are
    /// still needed keep running; the web server is restarted so it picks
    /// up new sites.
    pub fn reload(&self) -> Result<()> {
        let mut running = self.busy.lock().unwrap();
        if running.ids.is_empty() {
            return Ok(());
        }
        let (specs, warnings) = self.plan()?;
        running.warnings = warnings;
        let wanted: BTreeSet<String> = specs.iter().map(|s| s.id.clone()).collect();
        for gone in running.ids.difference(&wanted) {
            let _ = self.supervisor.stop(gone);
        }
        for spec in specs {
            if is_web_server(&spec.id) {
                let _ = self.supervisor.stop(&spec.id);
            }
            self.supervisor.start(spec)?;
        }
        running.ids = wanted;
        Ok(())
    }

    pub fn status(&self) -> Status {
        let warnings = self.busy.lock().unwrap().warnings.clone();
        let config = Config::load(&self.layout).unwrap_or_default();
        let discovery = site::discover(&self.layout, &config);
        let mut all_warnings = discovery.warnings;
        all_warnings.extend(warnings);
        Status { processes: self.supervisor.status(), sites: discovery.sites, warnings: all_warnings }
    }

    /// Everything that should run, PHP pools first so the web server finds
    /// its upstreams ready.
    fn plan(&self) -> Result<(Vec<ProcessSpec>, Vec<String>)> {
        self.layout.ensure()?;
        let config = Config::load(&self.layout)?;
        std::fs::create_dir_all(self.layout.resolve(&config.general.www))?;
        let discovery = site::discover(&self.layout, &config);
        let mut warnings = discovery.warnings;
        let autostart = |name: &str| config.general.autostart.iter().any(|a| a == name);

        let mut specs = Vec::new();
        if autostart("web") || autostart("php") {
            let php_plan = php::plan(&self.layout, &config, &discovery.sites)?;
            warnings.extend(php_plan.warnings.iter().cloned());
            for pool in &php_plan.pools {
                specs.extend(pool.specs.iter().cloned());
            }
            if autostart("web") {
                specs.push(match config.general.web_server {
                    WebServer::Caddy => caddy::plan(&self.layout, &config, &discovery.sites, &php_plan)?,
                    other => bail!("{other:?} is not supported yet; set general.web_server = \"caddy\""),
                });
            }
        }
        for name in &config.general.autostart {
            if !matches!(name.as_str(), "web" | "php") && service::Kind::from_id(name).is_some() {
                warnings.push(format!("{name} cannot be started by this version yet"));
            }
        }
        Ok((specs, warnings))
    }
}

fn is_web_server(id: &str) -> bool {
    matches!(id, "caddy" | "nginx" | "apache")
}
