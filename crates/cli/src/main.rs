// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `meow`, the MeowServer command line (PRD §13).

use std::io::Write;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use meow_core::config::Config;
use meow_core::download::{self, Progress};
use meow_core::engine::Status;
use meow_core::http::Http;
use meow_core::ipc::{Client, Request};
use meow_core::layout::Layout;
use meow_core::provider;
use meow_core::service::{self, Kind};
use meow_core::site;
use meow_core::supervisor::State;

#[derive(Parser)]
#[command(name = "meow", version, about = "MeowServer: a lightweight local web development environment")]
struct Cli {
    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start MeowServer and the services listed in `general.autostart`.
    Up {
        /// Run without a tray icon.
        #[arg(long)]
        headless: bool,
    },
    /// Stop all services. MeowServer itself keeps running.
    Down,
    /// Stop all services and exit MeowServer.
    Quit,
    /// Re-read meow.toml and project folders, then apply the changes.
    Reload,
    /// Show services and sites.
    Status,
    /// List all sites.
    Sites,
    /// List installed versions of a service, or downloadable ones.
    List {
        service: String,
        #[arg(long)]
        available: bool,
    },
    /// Download a service from its official source.
    Install {
        service: String,
        /// A version or prefix such as `8.4`. Defaults to the newest.
        version: Option<String>,
    },
    /// Set the active version of a service, e.g. `meow use php 8.3`.
    Use { service: String, version: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let layout = Layout::discover()?;
    let json = cli.json;
    match cli.command {
        Cmd::Up { headless } => {
            let mut client = ensure_daemon(&layout, headless)?;
            client.call(Request::StartAll)?;
            print_status(&client.call(Request::Status)?, json)
        }
        Cmd::Down => call(&layout, Request::StopAll).map(drop),
        Cmd::Quit => match Client::connect(&layout) {
            Ok(mut client) => client.call(Request::Shutdown).map(drop),
            Err(_) => Ok(()),
        },
        Cmd::Reload => call(&layout, Request::Reload).map(drop),
        Cmd::Status => match Client::connect(&layout) {
            Ok(mut client) => print_status(&client.call(Request::Status)?, json),
            Err(_) => {
                println!("MeowServer is not running. Start it with `meow up`.");
                Ok(())
            }
        },
        Cmd::Sites => {
            let config = Config::load(&layout)?;
            let discovery = site::discover(&layout, &config);
            if json {
                println!("{}", serde_json::to_string_pretty(&discovery)?);
            } else {
                print_sites(&discovery.sites, &discovery.warnings);
            }
            Ok(())
        }
        Cmd::List { service, available } => list(&layout, &service, available, json),
        Cmd::Install { service, version } => install(&layout, &service, version.as_deref()),
        Cmd::Use { service, version } => use_version(&layout, &service, &version),
    }
}

fn call(layout: &Layout, request: Request) -> Result<serde_json::Value> {
    Client::connect(layout)?.call(request)
}

/// Connects to `meowd`, starting it first if needed.
fn ensure_daemon(layout: &Layout, headless: bool) -> Result<Client> {
    if let Ok(client) = Client::connect(layout) {
        return Ok(client);
    }
    let exe = std::env::current_exe()?;
    let meowd = exe.with_file_name(format!("meowd{}", std::env::consts::EXE_SUFFIX));
    let mut cmd = Command::new(&meowd);
    if headless {
        cmd.arg("--headless");
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    meow_core::platform::spawn_detached(&mut cmd)
        .with_context(|| format!("cannot start {}", meowd.display()))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(client) = Client::connect(layout) {
            return Ok(client);
        }
        if Instant::now() > deadline {
            bail!("MeowServer did not start; see {}", layout.logs().join("meowd.log").display());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn parse_kind(service: &str) -> Result<Kind> {
    Kind::from_id(service).with_context(|| {
        let known: Vec<&str> = Kind::ALL.iter().map(|k| k.id()).collect();
        format!("unknown service {service:?}; known services: {}", known.join(", "))
    })
}

fn list(layout: &Layout, service: &str, available: bool, json: bool) -> Result<()> {
    let kind = parse_kind(service)?;
    if available {
        let provider = provider::for_service(kind)
            .with_context(|| format!("{service} cannot be downloaded on this platform yet"))?;
        let releases = provider.releases(&Http::new())?;
        if json {
            println!("{}", serde_json::to_string_pretty(&releases)?);
            return Ok(());
        }
        let installed = service::installed(layout, kind);
        for rel in releases {
            let mark = if installed.iter().any(|i| i.version == rel.version) { "  (installed)" } else { "" };
            let checksum =
                if rel.assets.iter().all(|a| a.sha256.is_some()) { "" } else { "  [no official checksum]" };
            println!("{}{mark}{checksum}", rel.version);
        }
        return Ok(());
    }
    let installed = service::installed(layout, kind);
    if json {
        println!("{}", serde_json::to_string_pretty(&installed)?);
        return Ok(());
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

fn install(layout: &Layout, service: &str, selector: Option<&str>) -> Result<()> {
    let kind = parse_kind(service)?;
    let provider = provider::for_service(kind)
        .with_context(|| format!("{service} cannot be downloaded on this platform yet"))?;
    let http = Http::new();
    let releases = provider.releases(&http)?;
    let release = releases
        .iter()
        .find(|r| selector.is_none_or(|s| r.version.matches(s)))
        .with_context(|| format!("no {service} release matches {:?}", selector.unwrap_or("latest")))?;
    layout.ensure()?;

    let mut on_asset = |asset: &provider::Asset| {
        println!("Downloading {}", asset.url);
        if asset.sha256.is_none() {
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

fn use_version(layout: &Layout, service: &str, selector: &str) -> Result<()> {
    let kind = parse_kind(service)?;
    let Some(found) = service::select(layout, kind, Some(selector)) else {
        bail!("{service} {selector} is not installed. Run `meow install {service} {selector}`.");
    };
    let mut config = Config::load(layout)?;
    config.versions.insert(kind.id().to_owned(), selector.to_owned());
    config.save(layout)?;
    println!("{service} {selector} is now active ({}).", found.version);
    if let Ok(mut client) = Client::connect(layout) {
        client.call(Request::Reload)?;
    }
    Ok(())
}

fn print_status(value: &serde_json::Value, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
        return Ok(());
    }
    let status: Status = serde_json::from_value(value.clone())?;
    if status.processes.is_empty() {
        println!("No services running.");
    }
    for p in &status.processes {
        let state = match &p.state {
            State::Starting => "starting".to_owned(),
            State::Running { pid } => format!("running (pid {pid})"),
            State::Stopped => "stopped".to_owned(),
            State::Failed { reason } => format!("FAILED: {reason}"),
        };
        let restarts = if p.restarts > 0 { format!(", {} restarts", p.restarts) } else { String::new() };
        println!("{:<14} {state}{restarts}", p.id);
    }
    println!();
    print_sites(&status.sites, &status.warnings);
    Ok(())
}

fn print_sites(sites: &[site::Site], warnings: &[String]) {
    if sites.is_empty() {
        println!("No sites yet. Add a folder to www/.");
    }
    for s in sites {
        let target = s.proxy.clone().unwrap_or_else(|| s.docroot.display().to_string());
        let php = s.php.as_deref().map(|v| format!("  php {v}")).unwrap_or_default();
        println!("http://{:<30} {target}{php}", s.host);
    }
    for w in warnings {
        println!("warning: {w}");
    }
}
