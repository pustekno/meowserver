// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Slow work started from the window: downloads, imports, service changes.
//! Each runs on its own thread; progress and results go back to the UI
//! through `Ui`.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use meow_core::config::Config;
use meow_core::download::{self, Progress};
use meow_core::http::Http;
use meow_core::import;
use meow_core::ipc::{self, Client, Request};
use meow_core::layout::Layout;
use meow_core::provider::{self, Release};
use meow_core::service::{self, Kind};

use crate::Ui;
use crate::tr::t;

/// Downloads `selector` (or the newest) of `kind`, reporting progress.
pub fn install(layout: &Layout, ui: &Ui, kind: Kind, selector: Option<&str>) -> Result<String> {
    let name = crate::snapshot::title(kind);
    ui.progress(format!("{name}: {}", t("looking for versions…")));
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
            Some(total) if total > 0 => {
                format!("{name} {version}: {}% {} {:.0} MB", got * 100 / total, t("of"), total as f64 / 1e6)
            }
            _ => format!("{name} {version}: {:.1} MB", got as f64 / 1e6),
        };
        ui.progress(text);
    };
    layout.ensure()?;
    service::pin_before_install(layout, kind)?;
    download::install(
        layout,
        &http,
        kind.id(),
        release,
        Progress { on_asset: &mut on_asset, on_bytes: &mut on_bytes },
    )?;
    ui.progress(format!("{name} {version} {}", t("installed.")));
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
    ui.progress(t("Starting…"));
    ipc::ensure_running(layout, false)?.call(Request::StartAll)?;
    ui.progress(String::new());
    Ok(())
}

/// What importing `source` will do, in the window's language, for the user
/// to confirm before anything is copied.
pub fn import_plan(layout: &Layout, source: &import::Source) -> String {
    let fill = |template: &str, values: &[&str]| {
        values.iter().fold(t(template), |text, v| text.replacen("{}", v, 1))
    };
    let suffix = Config::load(layout).unwrap_or_default().general.suffix.as_str().to_owned();
    let mut lines =
        vec![fill("Import from {} ({}):", &[source.tool.name(), &source.root.display().to_string()])];
    for dir in &source.parked {
        lines.push(format!("• {}", fill("serve every folder in {}", &[&dir.display().to_string()])));
    }
    for p in &source.projects {
        lines.push(format!("• {} → {}.{suffix}", p.path.display(), p.name));
    }
    for db in &source.databases {
        let version = db.version.clone().unwrap_or_default();
        let what = format!("{} {version}", crate::snapshot::title(db.kind));
        lines.push(format!(
            "• {}",
            fill("copy the {} databases ({})", &[what.trim(), &import::human(db.bytes)])
        ));
    }
    lines.push(String::new());
    lines.push(fill("Project folders stay where they are. Nothing in {} is changed.", &[source.tool.name()]));
    if import::in_use(source) {
        lines.push(fill(
            "{} is running. Quit it first so its databases can be copied safely.",
            &[source.tool.name()],
        ));
    }
    lines.join("\n")
}

pub fn import_from(layout: &Layout, ui: &Ui, tool: import::Tool, root: &Path) -> Result<String> {
    let source = import::detect(tool, Some(root))
        .with_context(|| t("{} was not found.").replacen("{}", tool.name(), 1))?;
    if import::in_use(&source) {
        bail!(
            "{}",
            t("{} is running. Quit it first so its databases can be copied safely.").replacen(
                "{}",
                tool.name(),
                1
            )
        );
    }
    let mut config = Config::load(layout)?;
    let notes = import::apply(layout, &mut config, &source, import::Mode::Copy, &mut |done, total| {
        ui.progress(format!(
            "{}: {} {} {}",
            t("Copying databases"),
            import::human(done),
            t("of"),
            import::human(total)
        ));
    })?;
    config.save(layout)?;
    reload(layout);
    ui.progress(String::new());
    Ok(format!("{}\n{}", t("Imported."), notes.join("\n")))
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

/// Stops every service and exits `meowd`, waiting until it is gone.
pub fn quit(layout: &Layout) {
    let Ok(mut client) = Client::connect(layout) else { return };
    let _ = client.call(Request::Shutdown);
    drop(client);
    for _ in 0..300 {
        if Client::connect(layout).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
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
