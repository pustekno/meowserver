// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `logs`, `doctor`, `hosts`, `import`, `report`, and the elevated helper.

use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use meow_core::config::Config;
use meow_core::layout::Layout;
use meow_core::{defender, doctor, hosts, import, logs, platform, report, system, tls};

use crate::{daemon, print};

#[derive(Subcommand)]
pub enum HostsAction {
    /// Write entries for every site (asks for administrator rights).
    Sync,
    /// Remove all MeowServer entries.
    Clear,
    /// Show the entries MeowServer wants.
    Show,
}

#[derive(Subcommand)]
pub enum DbAction {
    /// Dump one database, or all, into backups/ (server must be running).
    Backup { service: String, database: Option<String> },
    /// Load a .sql or .sql.gz file.
    Restore { service: String, file: PathBuf },
    /// Move data to a newer installed version, backing it up first.
    Upgrade { service: String, version: String },
    /// Open a database GUI (HeidiSQL, or general.db_tool).
    Open { service: String },
}

#[derive(Subcommand)]
pub enum SslAction {
    /// Add the CA to your trust store, so HTTPS sites show no warning.
    Trust,
    /// Remove the CA from your trust store.
    Untrust,
    /// Show where the CA is and whether it is trusted.
    Status,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

#[derive(Subcommand)]
pub enum DefenderAction {
    /// Add exclusions for the MeowServer folder and every project folder.
    Exclude,
    /// Remove the exclusions MeowServer added.
    Remove,
    /// Show what would be excluded.
    Show,
}

#[derive(clap::Args)]
pub struct ImportArgs {
    /// laragon, xampp, wamp, or flyenv. Without it, the one tool found is
    /// used; when several are found they are listed.
    tool: Option<String>,
    /// The tool's folder, when it is not in the usual place.
    #[arg(long, requires = "tool")]
    path: Option<PathBuf>,
    /// Use the data folder where it is instead of copying it.
    #[arg(long)]
    in_place: bool,
    /// Only show what would happen.
    #[arg(long)]
    dry_run: bool,
    /// Do not ask for confirmation.
    #[arg(long, short)]
    yes: bool,
}

#[derive(Subcommand)]
pub enum Privileged {
    HostsSync {
        #[arg(long)]
        home: PathBuf,
    },
    DefenderExclude {
        #[arg(long)]
        home: PathBuf,
    },
    DefenderRemove {
        #[arg(long)]
        home: PathBuf,
    },
    CaInstall {
        #[arg(long)]
        home: PathBuf,
    },
    CaRemove {
        #[arg(long)]
        home: PathBuf,
    },
}

pub fn logs(layout: &Layout, service: &str, follow: bool, lines: usize) -> Result<()> {
    let files = logs::find(layout, service);
    let Some(newest) = files.first() else {
        bail!("no logs for {service:?} in {}", layout.logs().display());
    };
    for file in files.iter().rev() {
        println!("==> {} <==", file.display());
        for line in logs::tail(file, lines) {
            println!("{line}");
        }
    }
    if !follow {
        return Ok(());
    }
    let mut f = std::fs::File::open(newest)?;
    f.seek(SeekFrom::End(0))?;
    let mut reader = std::io::BufReader::new(f);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            std::thread::sleep(Duration::from_millis(300));
            continue;
        }
        print!("{line}");
        std::io::stdout().flush()?;
    }
}

pub fn shell(layout: &Layout) -> Result<()> {
    let config = Config::load(layout)?;
    let status = meow_core::terminal::shell(layout, &config)?.status()?;
    std::process::exit(status.code().unwrap_or(1));
}

pub fn run_command(layout: &Layout, command: &[String]) -> Result<()> {
    let config = Config::load(layout)?;
    let vars = meow_core::terminal::env(layout, &config)?;
    let path = vars.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone());
    let program = resolve_program(&command[0], path.as_deref());
    let status = std::process::Command::new(&program)
        .args(&command[1..])
        .envs(vars)
        .status()
        .with_context(|| format!("cannot run {}", command[0]))?;
    std::process::exit(status.code().unwrap_or(1));
}

/// `Command` looks programs up in our own PATH, not the one we pass to
/// the child, so find it in MeowServer's PATH first (with `.bat`/`.cmd`
/// shims on Windows).
fn resolve_program(name: &str, path: Option<&std::ffi::OsStr>) -> PathBuf {
    let Some(path) = path else { return PathBuf::from(name) };
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".bat", ".cmd", ""] } else { &[""] };
    for dir in std::env::split_paths(path) {
        for ext in exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(name)
}

pub fn doctor(layout: &Layout, json: bool) -> Result<()> {
    let checks = doctor::run(layout);
    if json {
        return print::json(&checks);
    }
    let problems = checks.iter().filter(|c| !c.ok).count();
    for c in &checks {
        let mark = if c.ok { "ok " } else { "!! " };
        println!("{mark} {:<22} {}", c.name, c.detail);
        if let Some(fix) = &c.fix {
            println!("    fix: {fix}");
        }
    }
    println!();
    if problems == 0 {
        println!("No problems found.");
    } else {
        println!("{problems} problem(s) found.");
    }
    Ok(())
}

pub fn hosts(layout: &Layout, action: HostsAction) -> Result<()> {
    let mut config = Config::load(layout)?;
    match action {
        HostsAction::Show => {
            for host in hosts::wanted(layout, &config) {
                println!("127.0.0.1 {host}");
            }
            Ok(())
        }
        HostsAction::Sync | HostsAction::Clear => {
            if matches!(action, HostsAction::Clear) {
                config.general.manage_hosts = false;
            }
            let meow = std::env::current_exe()?;
            match hosts::sync(layout, &config, &meow)? {
                hosts::Sync::UpToDate => println!("The hosts file is already up to date."),
                hosts::Sync::Written => println!("Updated {}.", hosts::path().display()),
                hosts::Sync::Declined => println!("Not updated: administrator rights were declined."),
            }
            Ok(())
        }
    }
}

pub fn db(layout: &Layout, action: DbAction) -> Result<()> {
    use meow_core::database;
    let kind = |s: &str| crate::versions::parse_kind(s);
    match action {
        DbAction::Backup { service, database: db } => {
            let file = database::backup(layout, kind(&service)?, db.as_deref())?;
            println!("Saved {}", file.display());
        }
        DbAction::Restore { service, file } => {
            database::restore(layout, kind(&service)?, &file)?;
            println!("Restored {}", file.display());
        }
        DbAction::Upgrade { service, version } => {
            let k = kind(&service)?;
            if meow_core::ipc::Client::connect(layout)
                .and_then(|mut c| c.call(meow_core::ipc::Request::Status))
                .is_ok_and(|s| s["units"].as_array().is_some_and(|u| u.iter().any(|x| x == k.id())))
            {
                bail!("stop {} first: meow stop {}", k.id(), k.id());
            }
            let mut copied = 0u64;
            let backup = database::upgrade(layout, k, &version, &mut |n| {
                copied += n;
                print!("\r  backing up {}", import::human(copied));
                let _ = std::io::stdout().flush();
            })?;
            println!("\nBackup in {}.", backup.display());
            println!("{} {version} is now active. Start it with `meow start {}`.", k.id(), k.id());
        }
        DbAction::Open { service } => database::open_tool(layout, kind(&service)?)?,
    }
    Ok(())
}

/// Runs cloudflared or ngrok in the foreground, pointed at one site.
pub fn share(layout: &Layout, name: &str) -> Result<()> {
    let config = Config::load(layout)?;
    let site = meow_core::site::discover(layout, &config)
        .sites
        .into_iter()
        .find(|s| s.name == name)
        .with_context(|| format!("no site named {name}"))?;
    let port = config.port("web", 80);
    let target = format!("http://127.0.0.1:{port}");
    let found = |exe: &str| {
        std::process::Command::new(exe)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    };
    let mut cmd = if found("cloudflared") {
        let mut c = std::process::Command::new("cloudflared");
        c.args(["tunnel", "--url", &target, "--http-host-header", &site.host]);
        c
    } else if found("ngrok") {
        let mut c = std::process::Command::new("ngrok");
        c.args(["http", &target, &format!("--host-header={}", site.host)]);
        c
    } else {
        bail!("install cloudflared or ngrok first; MeowServer only starts them");
    };
    println!("Sharing {} — press Ctrl+C to stop.", site.host);
    let status = cmd.status()?;
    std::process::exit(status.code().unwrap_or(0));
}

pub fn ssl(layout: &Layout, action: SslAction) -> Result<()> {
    match action {
        SslAction::Trust => {
            println!("Adding {} to your trust store.", tls::ca_cert_path(layout).display());
            println!(
                "Windows and macOS ask you to confirm; the CA only works for .localhost, .test, and loopback."
            );
            tls::trust(layout)?;
            println!("Trusted. HTTPS sites open without a warning (restart the browser if it was open).");
        }
        SslAction::Untrust => {
            tls::untrust(layout)?;
            println!("Removed from the trust store.");
        }
        SslAction::Status => {
            println!("CA certificate: {}", tls::ca_cert_path(layout).display());
            println!(
                "Trusted: {}",
                if tls::is_trusted(layout) { "yes" } else { "no (run `meow ssl trust`)" }
            );
        }
    }
    Ok(())
}

pub fn xdebug(layout: &Layout, state: OnOff) -> Result<()> {
    use meow_core::service::{self, Kind, php};
    let on = matches!(state, OnOff::On);
    if on {
        let http = meow_core::http::Http::new();
        let missing: Vec<_> = service::installed(layout, Kind::Php)
            .into_iter()
            .filter(|i| !php::has_xdebug(i) && i.dir.join("ext").is_dir())
            .collect();
        let index =
            if missing.is_empty() { String::new() } else { meow_core::provider::xdebug::index(&http)? };
        for installed in missing {
            match php::install_xdebug(&http, &index, &installed) {
                Ok(v) => println!("Xdebug {v} installed for PHP {}", installed.version),
                Err(e) => println!("PHP {}: {e:#}", installed.version),
            }
        }
    }
    let mut config = Config::load(layout)?;
    config.php.xdebug = on;
    config.save(layout)?;
    if let Ok(mut client) = meow_core::ipc::Client::connect(layout) {
        let _ = client.call(meow_core::ipc::Request::Restart { unit: "php".into() });
    }
    println!(
        "{}",
        if on {
            "Xdebug is on. Start a debug session with ?XDEBUG_TRIGGER=1 or a browser extension; your IDE listens on port 9003."
        } else {
            "Xdebug is off."
        }
    );
    Ok(())
}

pub fn login(layout: &Layout, state: OnOff) -> Result<()> {
    let on = matches!(state, OnOff::On);
    system::set_login_item(layout, on)?;
    let mut config = Config::load(layout)?;
    config.general.start_on_login = on;
    config.save(layout)?;
    println!(
        "{}",
        if on { "MeowServer starts when you log in." } else { "MeowServer no longer starts at login." }
    );
    Ok(())
}

pub fn undo(layout: &Layout) -> Result<()> {
    let meow = std::env::current_exe()?;
    for note in system::undo(layout, &meow) {
        println!("{note}");
    }
    Ok(())
}

pub fn defender(layout: &Layout, action: DefenderAction) -> Result<()> {
    let config = Config::load(layout)?;
    let (verb, paths) = match action {
        DefenderAction::Show => {
            let state = match defender::realtime_enabled() {
                Some(true) => "on",
                Some(false) => "off",
                None => "unknown",
            };
            println!("Real-time protection: {state}");
            let done = defender::recorded(layout).paths;
            for p in defender::wanted(layout, &config) {
                let excluded = done.iter().any(|d| p.starts_with(d));
                println!("  {} {}", if excluded { "excluded" } else { "scanned " }, p.display());
            }
            return Ok(());
        }
        DefenderAction::Exclude => ("defender-exclude", defender::missing(layout, &config)),
        DefenderAction::Remove => ("defender-remove", defender::recorded(layout).paths),
    };
    if paths.is_empty() {
        println!("Nothing to do.");
        return Ok(());
    }
    for p in &paths {
        println!("  {}", p.display());
    }
    let meow = std::env::current_exe()?;
    let args = vec!["privileged".into(), verb.into(), "--home".into(), layout.root().display().to_string()];
    match platform::run_elevated(&meow, &args) {
        Ok(0) => println!("Done."),
        Ok(code) => bail!("the change failed (exit code {code})"),
        Err(_) => println!("Not changed: administrator rights were declined."),
    }
    Ok(())
}

pub fn import(layout: &Layout, args: ImportArgs) -> Result<()> {
    let ImportArgs { tool, path, in_place, dry_run, yes } = args;
    let source = match tool.as_deref() {
        None | Some("auto") => {
            let mut found = import::detect_all();
            match found.len() {
                0 => {
                    println!(
                        "No Laragon, XAMPP, WampServer, or FlyEnv with projects or databases was found."
                    );
                    println!("If one is in an unusual place: meow import <tool> --path <folder>");
                    return Ok(());
                }
                1 => found.remove(0),
                _ => {
                    println!("Found:");
                    for s in &found {
                        println!("  {:<8} {}", s.tool.id(), s.root.display());
                    }
                    println!("\nChoose one: meow import <tool>");
                    return Ok(());
                }
            }
        }
        Some(id) => {
            let tool = import::Tool::parse(id)
                .with_context(|| format!("unknown tool {id:?}; use laragon, xampp, wamp, or flyenv"))?;
            import::detect(tool, path.as_deref())
                .with_context(|| format!("{} was not found; pass --path <folder>", tool.name()))?
        }
    };
    let mode = if in_place { import::Mode::InPlace } else { import::Mode::Copy };
    println!("Found {} in {}.\n", source.tool.name(), source.root.display());
    let steps = import::describe(layout, &source, mode);
    if steps.is_empty() {
        println!("It has no projects or databases to import.");
        return Ok(());
    }
    for step in steps {
        println!("  - {step}");
    }
    let running = import::in_use(&source).then(|| {
        format!("{} is running; quit it first so its databases can be copied safely", source.tool.name())
    });
    if dry_run {
        if let Some(warning) = running {
            println!("\nNote: {warning}.");
        }
        return Ok(());
    }
    if let Some(warning) = running {
        bail!("{warning}");
    }
    if !yes && !confirm("\nProceed?")? {
        println!("Nothing was changed.");
        return Ok(());
    }
    let mut config = Config::load(layout)?;
    let mut last = 0u64;
    let notes = import::apply(layout, &mut config, &source, mode, &mut |done, total| {
        if done - last > 64 << 20 || done == total {
            last = done;
            print!("\r  copying {} of {}", import::human(done), import::human(total));
            let _ = std::io::stdout().flush();
        }
    })?;
    config.save(layout)?;
    println!();
    for note in notes {
        println!("{note}");
    }
    daemon::reload_if_running(layout)
}

pub fn report(layout: &Layout, open: bool) -> Result<()> {
    let text = report::build(layout);
    println!("{text}");
    println!("\n----\nThis is everything the report contains. Nothing has been sent.");
    let url = report::issue_url("Problem report", &text);
    if open {
        platform::open(url.as_ref())?;
        report::acknowledge(layout);
        println!("Opened a pre-filled GitHub issue in your browser. Submit it there if you want to send it.");
    } else {
        println!("Run `meow report --open` to open it as a pre-filled GitHub issue.");
    }
    Ok(())
}

/// Runs elevated; must stay tiny and only do fixed, validated work.
pub fn privileged(action: Privileged) -> Result<()> {
    match action {
        Privileged::HostsSync { home } => {
            let layout = Layout::at(home);
            let config = Config::load(&layout)?;
            let wanted =
                if config.general.manage_hosts { hosts::wanted(&layout, &config) } else { Vec::new() };
            hosts::write(&wanted)
        }
        // The folders are recomputed here from meow.toml, never taken from
        // the command line.
        Privileged::DefenderExclude { home } => {
            let layout = Layout::at(home);
            let config = Config::load(&layout)?;
            defender::apply(&layout, &defender::missing(&layout, &config), true)
        }
        Privileged::DefenderRemove { home } => {
            let layout = Layout::at(home);
            defender::apply(&layout, &defender::recorded(&layout).paths, false)
        }
        Privileged::CaInstall { home } => tls::install_system_ca(&Layout::at(home), true),
        Privileged::CaRemove { home } => tls::install_system_ca(&Layout::at(home), false),
    }
}

fn confirm(question: &str) -> Result<bool> {
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().take(16).read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}
