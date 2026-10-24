// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Caddy, the default web server (PRD §9.8).

use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::Serialize;

use super::{Kind, php, write_generated};
use crate::config::Config;
use crate::layout::Layout;
use crate::site::Site;
use crate::supervisor::{ProcessSpec, Restart};
use crate::template;

/// Caddy's admin API, on loopback only, at the top of MeowServer's range.
const ADMIN: &str = "127.0.0.1:19999";

#[derive(Serialize)]
struct Context {
    admin: &'static str,
    http_port: u16,
    https_port: u16,
    storage: PathBuf,
    custom_dir: PathBuf,
    global_override: Option<PathBuf>,
    bind: Option<&'static str>,
    sites: Vec<SiteContext>,
}

#[derive(Serialize)]
struct SiteContext {
    host: String,
    docroot: PathBuf,
    proxy: Option<String>,
    php: Option<Vec<String>>,
    #[serde(rename = "override")]
    override_file: Option<PathBuf>,
}

/// Writes the Caddyfile and returns the process to run.
pub fn plan(layout: &Layout, config: &Config, sites: &[Site], php: &php::Plan) -> Result<ProcessSpec> {
    let Some(installed) = super::select(layout, Kind::Caddy, config.version_selector("caddy")) else {
        bail!("Caddy is not installed. Run `meow install caddy`.");
    };
    let custom = layout.custom().join("caddy");
    std::fs::create_dir_all(custom.join("sites"))?;
    let existing = |path: PathBuf| path.exists().then_some(path);

    let ctx = Context {
        admin: ADMIN,
        http_port: config.port("web", 80),
        https_port: config.tls_port("web", 443),
        storage: layout.data().join("caddy"),
        custom_dir: custom.clone(),
        global_override: existing(custom.join("global.caddy")),
        bind: (!config.general.lan_access).then_some("127.0.0.1 [::1]"),
        sites: sites
            .iter()
            .map(|site| SiteContext {
                host: site.host.clone(),
                docroot: site.docroot.clone(),
                proxy: site.proxy.clone(),
                php: php.upstreams(site).map(<[String]>::to_vec),
                override_file: existing(custom.join("sites").join(format!("{}.caddy", site.name))),
            })
            .collect(),
    };
    let caddyfile = layout.generated().join("caddy").join("Caddyfile");
    write_generated(&caddyfile, &template::render("caddy/Caddyfile", ctx)?)?;

    // Keep Caddy's own state inside the MeowServer folder (PRD §3, portable).
    let state = layout.data().join("caddy");
    Ok(ProcessSpec {
        id: "caddy".into(),
        program: installed.exe("caddy"),
        args: vec!["run".into(), "--config".into(), caddyfile.into(), "--adapter".into(), "caddyfile".into()],
        env: vec![
            (OsString::from("XDG_DATA_HOME"), state.clone().into()),
            (OsString::from("XDG_CONFIG_HOME"), state.into()),
        ],
        cwd: Some(installed.dir.clone()),
        log: layout.logs().join("caddy.log"),
        restart: Restart::OnFailure,
        graceful_stop: None,
    })
}
