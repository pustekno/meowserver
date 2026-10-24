// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Caddy, the default web server (PRD §9.8).

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::Serialize;

use super::web::{self, Planned, TemplateSite};
use super::{Kind, php, write_generated};
use crate::config::Config;
use crate::layout::Layout;
use crate::site::Site;
use crate::supervisor::{Invocation, ProcessSpec, Restart};
use crate::template;

/// Caddy's admin API, on loopback only, at the top of MeowServer's range
/// (`[services.caddy] port`).
pub const DEFAULT_ADMIN_PORT: u16 = 19999;

#[derive(Serialize)]
struct Context {
    admin: String,
    http_port: u16,
    https_port: u16,
    storage: PathBuf,
    custom_dir: PathBuf,
    global_override: Option<PathBuf>,
    listen: Vec<&'static str>,
    index_dir: PathBuf,
    sites: Vec<TemplateSite>,
    tls: Option<web::Tls>,
}

pub fn plan(layout: &Layout, config: &Config, sites: &[Site], php: &php::Plan) -> Result<Planned> {
    let Some(installed) = super::select(layout, Kind::Caddy, config.version_selector("caddy")) else {
        bail!("Caddy is not installed. Run `meow install caddy`.");
    };
    let admin = format!("127.0.0.1:{}", config.port("caddy", DEFAULT_ADMIN_PORT));
    let custom = layout.custom().join("caddy");
    let global = custom.join("global.caddy");
    let ctx = Context {
        admin: admin.clone(),
        http_port: config.port("web", 80),
        https_port: config.tls_port("web", 443),
        storage: layout.data().join("caddy"),
        global_override: global.exists().then_some(global),
        custom_dir: custom,
        listen: web::listen(config, &[]),
        index_dir: web::write_index(layout, config, sites, php)?,
        sites: web::sites(layout, config, Kind::Caddy, sites, php),
        tls: web::tls(layout, config, sites),
    };
    let caddyfile = layout.generated().join("caddy").join("Caddyfile");
    write_generated(&caddyfile, &template::render("caddy/Caddyfile", ctx)?)?;

    // Keep Caddy's own state inside the MeowServer folder (PRD §3, portable).
    let state = layout.data().join("caddy");
    let config_args = |verb: &str| -> Vec<OsString> {
        vec![verb.into(), "--config".into(), caddyfile.clone().into(), "--adapter".into(), "caddyfile".into()]
    };
    let mut reload_args = config_args("reload");
    reload_args.extend(["--address".into(), admin.into()]);
    Ok(Planned {
        spec: ProcessSpec {
            id: "caddy".into(),
            program: installed.exe("caddy"),
            args: config_args("run"),
            env: vec![
                (OsString::from("XDG_DATA_HOME"), state.clone().into()),
                (OsString::from("XDG_CONFIG_HOME"), state.into()),
            ],
            cwd: Some(installed.dir.clone()),
            log: layout.logs().join("caddy.log"),
            restart: Restart::OnFailure,
            graceful_stop: None,
        },
        reload: Some(Invocation {
            program: installed.exe("caddy"),
            args: reload_args,
            timeout: Duration::from_secs(10),
        }),
    })
}
