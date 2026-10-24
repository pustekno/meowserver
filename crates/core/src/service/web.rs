// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! What the three web servers share: the site list as templates see it, the
//! addresses to listen on, and the `http://localhost` index page (PRD F14).

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use super::{Kind, php, write_generated};
use crate::config::{Config, WebServer};
use crate::layout::Layout;
use crate::site::Site;
use crate::supervisor::{Invocation, ProcessSpec};
use crate::template;

/// A web server ready to start, and how to make it re-read its config
/// without dropping connections, if it can.
pub struct Planned {
    pub spec: ProcessSpec,
    pub reload: Option<Invocation>,
}

#[derive(Serialize)]
pub struct TemplateSite {
    pub name: String,
    pub host: String,
    pub docroot: PathBuf,
    pub proxy: Option<String>,
    /// Pool name, for servers that refer to upstream groups by name.
    pub pool: Option<String>,
    /// Pool addresses, for servers that list them inline (Caddy).
    pub upstreams: Option<Vec<String>>,
    #[serde(rename = "override")]
    pub override_file: Option<PathBuf>,
}

#[derive(Serialize)]
pub struct TemplatePool {
    pub name: String,
    pub upstreams: Vec<String>,
}

pub fn kind(config: &Config) -> Kind {
    match config.general.web_server {
        WebServer::Caddy => Kind::Caddy,
        WebServer::Nginx => Kind::Nginx,
        WebServer::Apache => Kind::Apache,
    }
}

/// Sites as every web server template sees them. `override_ext` is the
/// file extension user overrides use for this server (`caddy` or `conf`).
pub fn sites(layout: &Layout, server: Kind, sites: &[Site], php: &php::Plan) -> Vec<TemplateSite> {
    let overrides = layout.custom().join(server.id()).join("sites");
    let _ = std::fs::create_dir_all(&overrides);
    let ext = if server == Kind::Caddy { "caddy" } else { "conf" };
    sites
        .iter()
        .map(|site| {
            let pool = php.pool_for(site);
            let override_file = overrides.join(format!("{}.{ext}", site.name));
            TemplateSite {
                name: site.name.clone(),
                host: site.host.clone(),
                docroot: site.docroot.clone(),
                proxy: site.proxy.clone(),
                pool: pool.map(php::Pool::name),
                upstreams: pool.map(|p| p.upstreams.clone()),
                override_file: override_file.exists().then_some(override_file),
            }
        })
        .collect()
}

pub fn pools(php: &php::Plan) -> Vec<TemplatePool> {
    php.pools.iter().map(|p| TemplatePool { name: p.name(), upstreams: p.upstreams.clone() }).collect()
}

/// Loopback only unless LAN access is on (PRD F19). `any` is how this
/// server spells "all addresses".
pub fn listen(config: &Config, any: &[&'static str]) -> Vec<&'static str> {
    if config.general.lan_access { any.to_vec() } else { vec!["127.0.0.1", "[::1]"] }
}

#[derive(Serialize)]
struct IndexContext {
    web_server: &'static str,
    php_version: String,
    services: Vec<String>,
    www: PathBuf,
    sites: Vec<IndexSite>,
}

#[derive(Serialize)]
struct IndexSite {
    host: String,
    url: String,
    target: String,
    php: String,
}

/// Writes the `http://localhost` page and returns its folder.
pub fn write_index(layout: &Layout, config: &Config, sites: &[Site], php: &php::Plan) -> Result<PathBuf> {
    let dir = layout.generated().join("index");
    let port = config.port("web", 80);
    let port_suffix = if port == 80 { String::new() } else { format!(":{port}") };
    let ctx = IndexContext {
        web_server: match config.general.web_server {
            WebServer::Caddy => "Caddy",
            WebServer::Nginx => "Nginx",
            WebServer::Apache => "Apache",
        },
        php_version: php.default_pool().map(|p| p.installed.version.to_string()).unwrap_or_default(),
        services: config
            .general
            .autostart
            .iter()
            .filter(|a| !matches!(a.as_str(), "web" | "php"))
            .cloned()
            .collect(),
        www: layout.resolve(&config.general.www),
        sites: sites
            .iter()
            .map(|s| IndexSite {
                host: s.host.clone(),
                url: format!("http://{}{port_suffix}/", s.host),
                target: s.proxy.clone().unwrap_or_else(|| s.path.display().to_string()),
                php: php.pool_for(s).map(|p| p.installed.version.line()).unwrap_or_default(),
            })
            .collect(),
    };
    write_generated(&dir.join("index.html"), &template::render("index/index.html", ctx)?)?;
    Ok(dir)
}

pub(super) fn ensure_dirs(dirs: &[&Path]) -> Result<()> {
    for dir in dirs {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}
