// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Apache httpd (PRD §9.8). PHP goes through `mod_proxy_fcgi` and a
//! balancer, not `mod_php`, so per-site PHP versions work like elsewhere.

use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::Serialize;

use super::web::{self, Planned, TemplatePool, TemplateSite};
use super::{Kind, php, write_generated};
use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{Os, os};
use crate::site::Site;
use crate::supervisor::{ProcessSpec, Restart};
use crate::template;

/// Loaded when present in the build's `modules/` folder.
const MODULES: &[&str] = &[
    "access_compat",
    "actions",
    "alias",
    "allowmethods",
    "auth_basic",
    "authn_core",
    "authn_file",
    "authz_core",
    "authz_groupfile",
    "authz_host",
    "authz_user",
    "autoindex",
    "deflate",
    "dir",
    "env",
    "expires",
    "filter",
    "headers",
    "include",
    "log_config",
    "mime",
    "negotiation",
    "proxy",
    "proxy_balancer",
    "proxy_fcgi",
    "proxy_http",
    "proxy_wstunnel",
    "lbmethod_byrequests",
    "rewrite",
    "setenvif",
    "slotmem_shm",
    "socache_shmcb",
    "ssl",
];

#[derive(Serialize)]
struct Context {
    install_dir: PathBuf,
    custom_dir: PathBuf,
    error_log: PathBuf,
    pid: PathBuf,
    http_port: u16,
    listen: Vec<&'static str>,
    modules: Vec<&'static str>,
    windows: bool,
    index_dir: PathBuf,
    pools: Vec<TemplatePool>,
    sites: Vec<TemplateSite>,
    tls: Option<web::Tls>,
}

pub fn plan(layout: &Layout, config: &Config, sites: &[Site], php: &php::Plan) -> Result<Planned> {
    let Some(installed) = super::select(layout, Kind::Apache, config.version_selector("apache")) else {
        bail!("Apache is not installed. Run `meow install apache`.");
    };
    let state = layout.data().join("apache");
    let custom = layout.custom().join("apache");
    web::ensure_dirs(&[&state, &custom.join("conf")])?;
    let modules_dir = installed.dir.join("modules");
    let ctx = Context {
        install_dir: installed.dir.clone(),
        custom_dir: custom,
        error_log: layout.logs().join("apache-error.log"),
        pid: state.join("httpd.pid"),
        http_port: config.port("web", 80),
        listen: web::listen(config, &["0.0.0.0", "[::]"]),
        modules: MODULES
            .iter()
            .copied()
            .filter(|m| modules_dir.join(format!("mod_{m}.so")).exists())
            .collect(),
        windows: os() == Os::Windows,
        index_dir: web::write_index(layout, config, sites, php)?,
        pools: web::pools(php),
        sites: web::sites(layout, config, Kind::Apache, sites, php),
        tls: web::tls(layout, config, sites),
    };
    let conf = layout.generated().join("apache").join("httpd.conf");
    write_generated(&conf, &template::render("apache/httpd.conf", ctx)?)?;

    Ok(Planned {
        spec: ProcessSpec {
            id: "apache".into(),
            program: installed.dir.join("bin").join(format!("httpd{}", crate::platform::EXE_SUFFIX)),
            args: vec!["-f".into(), conf.into(), "-d".into(), installed.dir.clone().into()],
            env: Vec::new(),
            cwd: Some(installed.dir.clone()),
            log: layout.logs().join("apache.log"),
            restart: Restart::OnFailure,
            graceful_stop: None,
        },
        // Apache on Windows has no reload outside of service mode; restart.
        reload: None,
    })
}
