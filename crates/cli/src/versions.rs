// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `list`, `install`, `uninstall`, `use`.

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use meow_core::config::{Config, WebServer};
use meow_core::download::{self, Progress};
use meow_core::http::Http;
use meow_core::layout::Layout;
use meow_core::provider;
use meow_core::service::{self, Kind};

use crate::{daemon, print};

pub fn parse_kind(service: &str) -> Result<Kind> {
    let alias = match service {
        "postgresql" | "pgsql" => "postgres",
        "httpd" => "apache",
        "nodejs" => "node",
        other => other,
    };
    Kind::from_id(alias).with_context(|| {
        let known: Vec<&str> = Kind::ALL.iter().map(|k| k.id()).collect();
        format!("unknown service {service:?}; known services: {}", known.join(", "))
    })
}

pub fn list(layout: &Layout, service: &str, available: bool, json: bool) -> Result<()> {
    let kind = parse_kind(service)?;
    let installed = service::installed(layout, kind);
    if available {
        let provider = provider::for_service(kind).with_context(|| {
            format!("{service} cannot be downloaded on this platform; install it with your package manager")
        })?;
        let releases = provider.releases(&Http::new())?;
        if json {
            return print::json(&releases);
        }
        for rel in releases {
            let mark = if installed.iter().any(|i| i.version == rel.version) { "  (installed)" } else { "" };
            let checksum =
                if rel.assets.iter().all(|a| a.has_checksum()) { "" } else { "  [no official checksum]" };
            println!("{}{mark}{checksum}", rel.version);
        }
        return Ok(());
    }
    if json {
        return print::json(&installed);
    }
    if installed.is_empty() {
        println!("No {service} versions installed. See `meow list {service} --available`.");
    }
    let config = Config::load(layout)?;
    let active = service::select(layout, kind, config.version_selector(kind.id())).map(|i| i.version);
    for i in installed {
        let mark = if Some(&i.version) == active.as_ref() { "  (active)" } else { "" };
        println!("{}{mark}", i.version);
    }
    Ok(())
}

pub fn install(layout: &Layout, service: &str, selector: Option<&str>) -> Result<()> {
    let kind = parse_kind(service)?;
    let provider = provider::for_service(kind).with_context(|| {
        format!("{service} cannot be downloaded on this platform; install it with your package manager")
    })?;
    let http = Http::new();
    let releases = provider.releases(&http)?;
    let release = releases
        .iter()
        .find(|r| selector.is_none_or(|s| r.version.matches(s)))
        .with_context(|| format!("no {service} release matches {:?}", selector.unwrap_or("latest")))?;
    layout.ensure()?;
    service::pin_before_install(layout, kind)?;

    let mut on_asset = |asset: &provider::Asset| {
        println!("Downloading {}", asset.url);
        if !asset.has_checksum() {
            println!("  note: upstream publishes no checksum for this file");
        }
    };
    let mut last = Instant::now();
    let mut on_bytes = |got: u64, total: Option<u64>| {
        if last.elapsed() < Duration::from_millis(200) {
            return;
        }
        last = Instant::now();
        match total {
            Some(t) if t > 0 => print!("\r  {:>3}%  {:.1} MB", got * 100 / t, got as f64 / 1e6),
            _ => print!("\r  {:.1} MB", got as f64 / 1e6),
        }
        let _ = std::io::stdout().flush();
    };
    let dir = download::install(
        layout,
        &http,
        kind.id(),
        release,
        Progress { on_asset: &mut on_asset, on_bytes: &mut on_bytes },
    )?;
    println!("\rInstalled {service} {} in {}", release.version, dir.display());
    Ok(())
}

pub fn uninstall(layout: &Layout, service: &str, selector: &str) -> Result<()> {
    let kind = parse_kind(service)?;
    let Some(found) =
        service::installed(layout, kind).into_iter().find(|i| i.version.to_string() == selector)
    else {
        bail!("{service} {selector} is not installed (give the full version, see `meow list {service}`)");
    };
    if meow_core::ipc::Client::connect(layout).is_ok() {
        let config = Config::load(layout)?;
        if service::select(layout, kind, config.version_selector(kind.id()))
            .is_some_and(|a| a.version == found.version)
        {
            bail!("{service} {selector} is the active version; stop MeowServer or switch versions first");
        }
    }
    std::fs::remove_dir_all(&found.dir).with_context(|| format!("cannot remove {}", found.dir.display()))?;
    println!("Removed {service} {selector}.");
    Ok(())
}

pub fn use_version(layout: &Layout, service: &str, selector: &str) -> Result<()> {
    let mut config = Config::load(layout)?;
    if service == "web" {
        config.general.web_server = match selector {
            "caddy" => WebServer::Caddy,
            "nginx" => WebServer::Nginx,
            "apache" | "httpd" => WebServer::Apache,
            other => bail!("unknown web server {other:?}; choose caddy, nginx, or apache"),
        };
        config.save(layout)?;
        println!("Web server is now {selector}.");
        return daemon::reload_if_running(layout);
    }
    let kind = parse_kind(service)?;
    let Some(found) = service::select(layout, kind, Some(selector)) else {
        bail!("{service} {selector} is not installed. Run `meow install {service} {selector}`.");
    };
    config.versions.insert(kind.id().to_owned(), selector.to_owned());
    config.save(layout)?;
    println!("{service} {selector} is now active ({}).", found.version);
    if matches!(kind, Kind::Php | Kind::Caddy | Kind::Nginx | Kind::Apache) {
        daemon::reload_if_running(layout)
    } else {
        println!("Restart it to switch: meow restart {}", kind.id());
        Ok(())
    }
}
