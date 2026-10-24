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

#[derive(Subcommand)]
pub enum ImportFrom {
    /// Reuse Laragon's projects and MySQL/MariaDB data.
    Laragon {
        /// Laragon's folder (default C:\laragon).
        #[arg(long)]
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
    },
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

pub fn import(layout: &Layout, from: ImportFrom) -> Result<()> {
    let ImportFrom::Laragon { path, in_place, dry_run, yes } = from;
    let laragon = import::detect(path.as_deref()).context("no Laragon installation found; pass --path")?;
    let mode = if in_place { import::Mode::InPlace } else { import::Mode::Copy };
    println!("Found Laragon in {}.\n", laragon.root.display());
    for step in import::describe(layout, &laragon, mode) {
        println!("  - {step}");
    }
    if dry_run {
        return Ok(());
    }
    if !yes && !confirm("\nProceed?")? {
        println!("Nothing was changed.");
        return Ok(());
    }
    let mut config = Config::load(layout)?;
    let mut last = 0u64;
    let notes = import::apply(layout, &mut config, &laragon, mode, &mut |done, total| {
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
