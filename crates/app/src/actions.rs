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
/// Starts `unit` from its switch on Home, downloading its program first
/// when none is installed yet (MySQL as 8.4, like the recommended stack).
pub fn start_unit(layout: &Layout, ui: &Ui, unit: &str) -> Result<String> {
    let config = Config::load(layout)?;
    let kind = match unit {
        "web" => service::web::kind(&config),
        id => Kind::from_id(id).with_context(|| format!("unknown service {id:?}"))?,
    };
    let mut note = String::new();
    if service::select(layout, kind, config.version_selector(kind.id())).is_none() {
        let pinned = config.version_selector(kind.id()).map(str::to_owned);
        let selector = pinned.clone().or_else(|| (kind == Kind::Mysql).then(|| "8.4".to_owned()));
        let version = install(layout, ui, kind, selector.as_deref())?;
        if pinned.is_none()
            && let Some(line) = selector
        {
            let mut config = Config::load(layout)?;
            config.versions.insert(kind.id().into(), line);
            config.save(layout)?;
        }
        note = format!("{} {version} {}", crate::snapshot::title(kind), t("installed and started."));
        ui.progress(t("Starting…"));
    }
    let result = call(layout, Request::Start { unit: unit.into() });
    ui.progress(String::new());
    result.map(|()| note)
}

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

const RUNNING: &str = "{}'s database server is still running (closing {} does not always stop it). \
                       Stop it so its databases can be copied safely.";

/// Stops the database servers of the tool being imported (PRD F27), then
/// returns the plan again and whether something is still running.
pub fn stop_import_blocker(
    layout: &Layout,
    ui: &Ui,
    tool: import::Tool,
    root: &Path,
) -> Result<(String, bool)> {
    let source = import::detect(tool, Some(root))
        .with_context(|| t("{} was not found.").replacen("{}", tool.name(), 1))?;
    ui.progress(t("Stopping {}'s database…").replace("{}", tool.name()));
    for note in import::stop_databases(&source) {
        log_warn(&format!("import: {note}"));
    }
    ui.progress(String::new());
    Ok((import_plan(layout, &source), import::in_use(&source)))
}

/// What importing `source` will do, in the window's language, for the user
/// to confirm before anything is copied.
pub fn import_plan(layout: &Layout, source: &import::Source) -> String {
    let fill = |template: &str, values: &[&str]| {
        values.iter().fold(t(template), |text, v| text.replacen("{}", v, 1))
    };
    let config = Config::load(layout).unwrap_or_default();
    let www = layout.resolve(&config.general.www);
    let mut lines =
        vec![fill("Import from {} ({}):", &[source.tool.name(), &source.root.display().to_string()])];
    for dir in &source.parked {
        lines.push(format!("• {}", fill("serve every folder in {}", &[&dir.display().to_string()])));
    }
    for p in &source.projects {
        let slug = meow_core::site::slugify(&p.name).unwrap_or_default();
        let host = format!("{slug}.{}", p.suffix.unwrap_or(config.general.suffix).as_str());
        let line = if p.copy {
            fill(
                "copy {} ({}) to {}, as {}",
                &[
                    &p.path.display().to_string(),
                    &import::human(p.bytes),
                    &www.join(&p.name).display().to_string(),
                    &host,
                ],
            )
        } else {
            fill("serve {} where it is, as {}", &[&p.path.display().to_string(), &host])
        };
        lines.push(format!("• {line}"));
    }
    if let Some(php) = &source.php {
        let installed = meow_core::service::select(layout, Kind::Php, Some(php)).is_some();
        let template =
            if installed { "make PHP {} the default" } else { "install PHP {} and make it the default" };
        lines.push(format!("• {}", fill(template, &[php])));
    }
    for db in &source.databases {
        let version = db.version.clone().unwrap_or_default();
        let what = format!("{} {version}", crate::snapshot::title(db.kind));
        let size = import::human(db.bytes);
        let line = match import::existing(layout, db) {
            import::Existing::None => fill("copy the {} databases ({})", &[what.trim(), &size]),
            import::Existing::Empty => {
                fill("copy the {} databases ({}); MeowServer's empty one is set aside", &[what.trim(), &size])
            }
            import::Existing::Databases(names) => fill(
                "skip the {} databases: MeowServer's already has databases ({})",
                &[what.trim(), &names.join(", ")],
            ),
        };
        lines.push(format!("• {line}"));
    }
    for link in &source.broken {
        lines.push(format!("• {}", fill("skip {}: the link's target does not exist", &[link])));
    }
    lines.push(String::new());
    lines.push(fill(
        "Projects stored in {} are copied, linked ones are served where they are, and nothing in {} is changed, so it can be removed afterwards.",
        &[source.tool.name(), source.tool.name()],
    ));
    if import::in_use(source) {
        lines.push(fill(RUNNING, &[source.tool.name(), source.tool.name()]));
    }
    lines.join("\n")
}

pub fn import_from(layout: &Layout, ui: &Ui, tool: import::Tool, root: &Path) -> Result<String> {
    let source = import::detect(tool, Some(root))
        .with_context(|| t("{} was not found.").replacen("{}", tool.name(), 1))?;
    if import::in_use(&source) {
        bail!("{}", t(RUNNING).replace("{}", tool.name()));
    }
    // What the tool ran, so projects find the same versions: its PHP line
    // and its database lines.
    let mut wanted: Vec<(Kind, String)> = source.php.iter().map(|l| (Kind::Php, l.clone())).collect();
    for db in &source.databases {
        if let Some(line) = db.version.as_deref().and_then(|v| v.parse::<meow_core::version::Version>().ok())
        {
            wanted.push((db.kind, line.line()));
        }
    }
    for (kind, line) in wanted {
        if service::select(layout, kind, Some(&line)).is_none() {
            install(layout, ui, kind, Some(&line))?;
        }
    }
    let mut config = Config::load(layout)?;
    let notes = import::apply(layout, &mut config, &source, import::Mode::Copy, &mut |done, total| {
        ui.progress(format!(
            "{}: {} {} {}",
            t("Copying projects and databases"),
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

/// Runs another installed version of a database next to the default one,
/// or stops doing so (PRD F24).
pub fn db_version(layout: &Layout, unit: &str, line: &str, enable: bool) -> Result<String> {
    use meow_core::database;
    let kind = Kind::from_id(unit).context("unknown database")?;
    let name = crate::snapshot::title(kind);
    if enable {
        let port = database::enable_version(layout, kind, line, None)?;
        call(layout, Request::Start { unit: unit.into() })?;
        Ok(t("{} {} runs on port {}.").replacen("{}", name, 1).replacen("{}", line, 1).replacen(
            "{}",
            &port.to_string(),
            1,
        ))
    } else {
        database::disable_version(layout, kind, line)?;
        call(layout, Request::Restart { unit: unit.into() })?;
        Ok(t("{} {} stopped. Its data is kept.").replacen("{}", name, 1).replacen("{}", line, 1))
    }
}

/// Moves MeowServer's `kind` server to `port`, the port projects connect to
/// (Laragon's 3306, say), so they all work without editing their settings.
/// A database server another tool left running there (Laragon's MySQL,
/// which closing Laragon does not stop) is stopped first, the way import
/// does; any other program in the way is named instead.
pub fn use_port(layout: &Layout, ui: &Ui, kind: Kind, port: u16) -> Result<String> {
    use meow_core::platform;
    let foreign = |port: u16| -> Vec<String> {
        platform::port_owners(port)
            .into_iter()
            .map(|(_, exe)| exe)
            .filter(|exe| !platform::is_inside(exe, layout.root()))
            .collect()
    };
    if !foreign(port).is_empty() {
        ui.progress(t("Stopping the database that holds port {}…").replace("{}", &port.to_string()));
        for source in import::detect_all() {
            if foreign(port).iter().any(|exe| platform::is_inside(exe, &source.root)) {
                for note in import::stop_databases(&source) {
                    log_warn(&format!("use port {port}: {note}"));
                }
            }
        }
        ui.progress(String::new());
        if let Some(exe) = foreign(port).first() {
            bail!(
                "{}",
                t("Port {} is used by {}. Close that program first.")
                    .replacen("{}", &port.to_string(), 1)
                    .replacen("{}", exe, 1)
            );
        }
    }
    let mut config = Config::load(layout)?;
    // Another of MeowServer's own servers set to that port.
    for other in [Kind::Mysql, Kind::Mariadb, Kind::Postgres].into_iter().filter(|k| *k != kind) {
        let default = if other == Kind::Postgres { 5432 } else { 3306 };
        let installed = service::select(layout, other, config.version_selector(other.id())).is_some();
        if installed && config.port(other.id(), default) == port {
            bail!(
                "{}",
                t("Port {} is used by MeowServer's {}.").replacen("{}", &port.to_string(), 1).replacen(
                    "{}",
                    crate::snapshot::title(other),
                    1
                )
            );
        }
    }
    let default = if kind == Kind::Postgres { 5432 } else { 3306 };
    let settings = config.services.entry(kind.id().into()).or_default();
    settings.port = (port != default).then_some(port);
    settings.moved_from.remove("port");
    config.save(layout)?;
    call(layout, Request::Restart { unit: kind.id().into() })?;
    Ok(t("{} now uses port {}.").replacen("{}", crate::snapshot::title(kind), 1).replacen(
        "{}",
        &port.to_string(),
        1,
    ))
}

/// Downloads a PECL extension for one PHP version, turns it on, and
/// restarts PHP if it runs.
pub fn install_extension(layout: &Layout, ui: &Ui, php: &service::Installed, name: &str) -> Result<String> {
    let http = Http::new();
    ui.progress(t("Downloading {}…").replace("{}", name));
    let (added, version) = meow_core::pecl::install(layout, &http, php, name, &mut |got, total| {
        let total = total.map(|t| format!(" / {}", import::human(t))).unwrap_or_default();
        ui.progress(format!("{name}: {}{total}", import::human(got)));
    })?;
    ui.progress(String::new());
    let mut config = Config::load(layout)?;
    for ext in &added {
        if !config.php.extensions.contains(ext) {
            config.php.extensions.push(ext.clone());
        }
    }
    config.save(layout)?;
    if Client::connect(layout).is_ok() {
        let _ = call(layout, Request::Restart { unit: "php".into() });
    }
    Ok(t("{} {} installed for PHP {} and turned on.")
        .replacen("{}", &added.join(", "), 1)
        .replacen("{}", &version, 1)
        .replacen("{}", &php.version.to_string(), 1))
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
