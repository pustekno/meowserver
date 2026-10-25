// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Microsoft Defender exclusions (PRD F39). Real-time scanning reads every
//! file PHP opens; a Laravel or WordPress project opens thousands, so the
//! first request after a restart can take tens of seconds. Excluding the
//! MeowServer folder and project folders is opt-in, needs one elevation
//! prompt, and is undone by "Undo system changes" (PRD F36).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{Os, os};
use crate::site::{self, Source};

/// What we excluded, so it can be removed again exactly.
#[derive(Default, Serialize, Deserialize)]
pub struct Record {
    pub paths: Vec<PathBuf>,
}

fn record_file(layout: &Layout) -> PathBuf {
    layout.data().join("defender-exclusions.json")
}

pub fn recorded(layout: &Layout) -> Record {
    std::fs::read(record_file(layout)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// The MeowServer folder (binaries and data) and every project folder.
pub fn wanted(layout: &Layout, config: &Config) -> Vec<PathBuf> {
    let mut paths = vec![layout.root().to_path_buf(), layout.resolve(&config.general.www)];
    paths.extend(config.general.parked.iter().cloned());
    for s in site::discover(layout, config).sites {
        if s.path.as_os_str().is_empty() {
            continue;
        }
        // Defender sees the real path behind a junction or symbolic link,
        // so a linked project (www\blog -> D:\blog) needs its target.
        if let Some(target) = link_target(&s.path) {
            paths.push(target);
        }
        if s.source == Source::Added {
            paths.push(s.path);
        }
    }
    // Folders inside an excluded folder are covered already.
    paths.sort();
    paths.dedup();
    let all = paths.clone();
    paths.retain(|p| !all.iter().any(|other| other != p && p.starts_with(other)));
    paths
}

/// Where `path` really is, when it is itself a link. Without the `\\?\`
/// prefix `canonicalize` adds on Windows, which Defender does not accept.
fn link_target(path: &Path) -> Option<PathBuf> {
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return None;
    }
    let real = std::fs::canonicalize(path).ok()?;
    let text = real.to_string_lossy();
    Some(match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => real,
    })
}

/// Folders that should be excluded but are not yet.
pub fn missing(layout: &Layout, config: &Config) -> Vec<PathBuf> {
    let done = recorded(layout).paths;
    wanted(layout, config).into_iter().filter(|p| !done.iter().any(|d| p.starts_with(d))).collect()
}

/// Whether Defender's real-time protection is on. `None` when it cannot be
/// read (not Windows, another antivirus, policy).
pub fn realtime_enabled() -> Option<bool> {
    if os() != Os::Windows {
        return None;
    }
    let out = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-MpComputerStatus).RealTimeProtectionEnabled",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    match String::from_utf8_lossy(&out.stdout).trim() {
        "True" => Some(true),
        "False" => Some(false),
        _ => None,
    }
}

/// Adds or removes exclusions. Must run elevated.
pub fn apply(layout: &Layout, paths: &[PathBuf], add: bool) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let list: Vec<String> =
        paths.iter().map(|p| format!("'{}'", p.display().to_string().replace('\'', "''"))).collect();
    let verb = if add { "Add-MpPreference" } else { "Remove-MpPreference" };
    let script = format!("{verb} -ExclusionPath {}", list.join(","));
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::null())
        .status()?;
    if !status.success() {
        bail!("{verb} failed ({status})");
    }
    let mut record = recorded(layout);
    if add {
        record.paths.extend(paths.iter().cloned());
        record.paths.sort();
        record.paths.dedup();
    } else {
        record.paths.retain(|p| !paths.contains(p));
    }
    std::fs::write(record_file(layout), serde_json::to_vec_pretty(&record)?)?;
    Ok(())
}
