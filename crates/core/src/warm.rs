// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! OPcache warm-up (PRD F47). The first visit to a Laravel site compiles
//! thousands of `vendor` files (24 s measured, against 1.3 s warmed). So
//! when PHP starts, and when sites are added, MeowServer compiles each
//! site's PHP files into OPcache's file cache in the background, one site
//! at a time and at low priority, with the same `php.ini` the web pool
//! uses so the pool can load the result. Nothing of the project runs: the
//! files are only compiled. A site is done again only when its PHP version
//! or its Composer packages change.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::layout::Layout;
use crate::service::{self, Installed, Kind, php};
use crate::site::{Framework, Site};

const SCRIPT: &str = include_str!("warm.php");

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Warms every PHP site that needs it, in a background thread. Does
/// nothing if a warm-up is already running; the next start or reload
/// picks up what it missed.
pub fn spawn(layout: Layout, config: Config, sites: Vec<Site>) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        for site in sites.iter().filter(|s| s.proxy.is_none() && s.framework != Framework::Static) {
            match warm_site(&layout, &config, site) {
                Ok(Some(report)) => log::info!("warm-up {}: {report}", site.name),
                Ok(None) => {}
                Err(e) => log::warn!("warm-up {}: {e:#}", site.name),
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
    });
}

/// Compiles one site, unless it was done for the same PHP and packages.
/// Returns what the script reported, or `None` when nothing was needed.
fn warm_site(layout: &Layout, config: &Config, site: &Site) -> Result<Option<String>> {
    let selector = site.php.as_deref().or(config.version_selector("php"));
    let Some(installed) = service::select(layout, Kind::Php, selector) else { return Ok(None) };
    let ini = layout.generated().join("php").join(installed.version.to_string()).join("php.ini");
    if !ini.is_file() {
        return Ok(None); // the pool for this version has not started yet
    }
    let stamp_file = stamp_dir(layout).join(format!("{}.txt", site.name));
    let stamp = stamp(&installed, &site.path);
    if std::fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp) {
        return Ok(None);
    }
    let script = layout.tmp().join("warm.php");
    std::fs::create_dir_all(layout.tmp())?;
    std::fs::write(&script, SCRIPT)?;

    let scan_dirs = php::scan_dirs(layout, &installed)?;
    // A file that declares a function another file declared ends PHP with
    // a fatal error; start again after it. What was compiled stays cached.
    let mut from = 0usize;
    let mut stopped_at = Vec::new();
    for _ in 0..MAX_RESTARTS {
        let mut cmd = Command::new(installed.exe("php"));
        cmd.arg("-c")
            .arg(&ini)
            .args(["-d", "opcache.enable_cli=1", "-d", "opcache.file_cache_only=1"])
            .arg(&script)
            .arg(&site.path)
            .arg(from.to_string())
            .env("PHP_INI_SCAN_DIR", &scan_dirs)
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        low_priority(&mut cmd);
        let out = cmd.output().context("cannot run PHP")?;
        let text = String::from_utf8_lossy(&out.stdout);
        if text.lines().any(|l| l == "no-opcache") {
            anyhow::bail!("PHP {} has no OPcache; its sites compile on the first visit", installed.version);
        }
        if let Some(summary) = text.lines().find_map(|l| l.strip_prefix("done ")) {
            let mut report = summary.to_owned();
            if !stopped_at.is_empty() {
                report.push_str(&format!(", {} files restarted around", stopped_at.len()));
            }
            std::fs::create_dir_all(stamp_dir(layout))?;
            std::fs::write(&stamp_file, stamp)?;
            return Ok(Some(report));
        }
        // "@<n>" is printed before file n: the last one is where it stopped.
        let last = text.lines().rev().find_map(|l| l.strip_prefix('@')?.trim().parse::<usize>().ok());
        match last {
            Some(n) if n >= from => {
                stopped_at.push(n);
                from = n + 1;
            }
            _ => anyhow::bail!("PHP stopped before compiling anything: {}", text.trim()),
        }
    }
    anyhow::bail!("stopped {MAX_RESTARTS} times; tried again on the next start")
}

/// Enough for packages that ship one copy of their functions per PHP
/// version, without looping forever on a broken project.
const MAX_RESTARTS: usize = 200;

fn stamp_dir(layout: &Layout) -> PathBuf {
    layout.data().join("php").join("opcache").join("warmed")
}

/// What decides whether a site needs compiling again: the PHP build (a
/// reinstall of the same version is another build), and when its Composer
/// packages last changed. Edited app files need nothing: OPcache recompiles
/// a changed file on its next use.
fn stamp(installed: &Installed, project: &Path) -> String {
    let modified = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    };
    // Extracting keeps the archive's dates, so the size tells builds apart too.
    let php = installed.exe("php");
    let size = std::fs::metadata(&php).map_or(0, |m| m.len());
    format!(
        "{} {} {size}\n{}\n{}",
        installed.version,
        modified(&php),
        modified(&project.join("composer.lock")),
        modified(&project.join("vendor/composer/installed.json"))
    )
}

/// Below normal, so a warm-up never slows down what the user is doing.
fn low_priority(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(BELOW_NORMAL_PRIORITY_CLASS | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::nice(10);
                Ok(())
            });
        }
    }
}
