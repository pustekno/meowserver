// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Slow work started from the window: downloads, imports, service changes.
//! Each runs on its own thread; progress and results go back to the UI
//! through `Ui`.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use meow_core::config::Config;
use meow_core::download::{self, Progress};
use meow_core::http::Http;
use meow_core::ipc::{self, Client, Request};
use meow_core::layout::Layout;
use meow_core::provider::{self, Release};
use meow_core::service::{self, Kind};

use crate::Ui;

/// Downloads `selector` (or the newest) of `kind`, reporting progress.
pub fn install(layout: &Layout, ui: &Ui, kind: Kind, selector: Option<&str>) -> Result<String> {
    let name = crate::snapshot::title(kind);
    ui.progress(format!("{name}: looking for versions…"));
    let provider = provider::for_service(kind).context("this service cannot be downloaded on this system")?;
    let http = Http::new();
    let releases = provider.releases(&http)?;
    let release: &Release = releases
        .iter()
        .find(|r| selector.is_none_or(|s| r.version.matches(s)))
        .with_context(|| format!("no {name} release matches {}", selector.unwrap_or("latest")))?;
    if service::installed(layout, kind).iter().any(|i| i.version == release.version) {
        return Ok(release.version.to_string());
    }
    let version = release.version.to_string();
    let mut on_asset = |_: &provider::Asset| {};
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut on_bytes = |got: u64, total: Option<u64>| {
        if last.elapsed() < Duration::from_millis(250) {
            return;
        }
        last = Instant::now();
        let text = match total {
            Some(t) if t > 0 => format!("{name} {version}: {}% of {:.0} MB", got * 100 / t, t as f64 / 1e6),
            _ => format!("{name} {version}: {:.1} MB", got as f64 / 1e6),
        };
        ui.progress(text);
    };
    layout.ensure()?;
    download::install(
        layout,
        &http,
        kind.id(),
        release,
        Progress { on_asset: &mut on_asset, on_bytes: &mut on_bytes },
    )?;
    ui.progress(format!("{name} {version} installed."));
    Ok(version)
}

/// The first-run stack (PRD §6.1): Caddy, the newest PHP, MySQL 8.4,
/// Composer; then start everything.
pub fn quick_setup(layout: &Layout, ui: &Ui) -> Result<()> {
    install(layout, ui, Kind::Caddy, None)?;
    install(layout, ui, Kind::Php, None)?;
    install(layout, ui, Kind::Mysql, Some("8.4"))?;
    if let Err(e) = install(layout, ui, Kind::Composer, None) {
        log_warn(&format!("Composer: {e:#}"));
    }
    let mut config = Config::load(layout)?;
    config.versions.insert("mysql".into(), "8.4".into());
    config.save(layout)?;
    ui.progress("Starting…".into());
    ipc::ensure_running(layout, false)?.call(Request::StartAll)?;
    ui.progress(String::new());
    Ok(())
}

pub fn import_laragon(layout: &Layout, ui: &Ui) -> Result<String> {
    use meow_core::import;
    let laragon = import::detect(None).context("Laragon was not found in C:\\laragon")?;
    let mut config = Config::load(layout)?;
    let notes = import::apply(layout, &mut config, &laragon, import::Mode::Copy, &mut |done, total| {
        ui.progress(format!("Copying databases: {} of {}", import::human(done), import::human(total)));
    })?;
    config.save(layout)?;
    reload(layout);
    ui.progress(String::new());
    Ok(notes.join("\n"))
}

/// Tells a running `meowd` to apply `meow.toml` again; quietly does
/// nothing if it is not running.
pub fn reload(layout: &Layout) {
    if let Ok(mut client) = Client::connect(layout) {
        if let Err(e) = client.call(Request::Reload) {
            log_warn(&format!("reload: {e:#}"));
        }
    }
}

pub fn call(layout: &Layout, request: Request) -> Result<()> {
    ipc::ensure_running(layout, false)?.call(request).map(drop)
}

pub fn uninstall(layout: &Layout, kind: Kind, version: &str) -> Result<()> {
    let config = Config::load(layout)?;
    let active = service::select(layout, kind, config.version_selector(kind.id()));
    if active.as_ref().is_some_and(|a| a.version.to_string() == version) {
        bail!("{version} is the active version; choose another first");
    }
    let found = service::installed(layout, kind)
        .into_iter()
        .find(|i| i.version.to_string() == version)
        .context("that version is not installed")?;
    std::fs::remove_dir_all(&found.dir)?;
    Ok(())
}

pub fn log_warn(text: &str) {
    eprintln!("meowserver: {text}");
}
