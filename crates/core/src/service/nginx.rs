// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Nginx (PRD §9.8). Runs with its own prefix under `data/nginx` so temp
//! files and the pid file stay inside the MeowServer folder.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::Serialize;

use super::web::{self, Planned, TemplatePool, TemplateSite};
use super::{Kind, php, write_generated};
use crate::config::Config;
use crate::layout::Layout;
use crate::site::Site;
use crate::supervisor::{Invocation, ProcessSpec, Restart};
use crate::template;

#[derive(Serialize)]
struct Context {
    install_dir: PathBuf,
    custom_dir: PathBuf,
    temp_dir: PathBuf,
    error_log: PathBuf,
    pid: PathBuf,
    http_port: u16,
    listen: Vec<&'static str>,
    index_dir: PathBuf,
    pools: Vec<TemplatePool>,
    sites: Vec<TemplateSite>,
}

pub fn plan(layout: &Layout, config: &Config, sites: &[Site], php: &php::Plan) -> Result<Planned> {
    let Some(installed) = super::select(layout, Kind::Nginx, config.version_selector("nginx")) else {
        bail!("Nginx is not installed. Run `meow install nginx`.");
    };
    let prefix = layout.data().join("nginx");
    let temp = prefix.join("temp");
    let custom = layout.custom().join("nginx");
    web::ensure_dirs(&[&prefix.join("logs"), &temp, &custom.join("http")])?;

    let ctx = Context {
        install_dir: installed.dir.clone(),
        custom_dir: custom,
        temp_dir: temp,
        error_log: layout.logs().join("nginx-error.log"),
        pid: prefix.join("nginx.pid"),
        http_port: config.port("web", 80),
        listen: web::listen(config, &["0.0.0.0", "[::]"]),
        index_dir: web::write_index(layout, config, sites, php)?,
        pools: web::pools(php),
        sites: web::sites(layout, Kind::Nginx, sites, php),
    };
    let conf = layout.generated().join("nginx").join("nginx.conf");
    write_generated(&conf, &template::render("nginx/nginx.conf", ctx)?)?;

    let base_args: Vec<OsString> = vec!["-p".into(), prefix.clone().into(), "-c".into(), conf.into()];
    let mut reload_args = base_args.clone();
    reload_args.extend(["-s".into(), "reload".into()]);
    Ok(Planned {
        spec: ProcessSpec {
            id: "nginx".into(),
            program: installed.exe("nginx"),
            args: base_args,
            env: Vec::new(),
            cwd: Some(installed.dir.clone()),
            log: layout.logs().join("nginx.log"),
            restart: Restart::OnFailure,
            graceful_stop: None,
        },
        reload: Some(Invocation {
            program: installed.exe("nginx"),
            args: reload_args,
            timeout: Duration::from_secs(10),
        }),
    })
}
