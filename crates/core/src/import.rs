// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Importing a Laragon setup (PRD F27): its `www` becomes a parked folder,
//! its MySQL/MariaDB data is copied (default) or used in place. Laragon's
//! own files are only read.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::Config;
use crate::layout::Layout;
use crate::service::datadir::{self, Found};
use crate::service::{self, Kind};
use crate::{platform, site};

#[derive(Debug, Serialize)]
pub struct Laragon {
    pub root: PathBuf,
    pub www: Vec<PathBuf>,
    pub databases: Vec<Database>,
}

#[derive(Debug, Serialize)]
pub struct Database {
    pub kind: Kind,
    pub version: Option<String>,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Copy data into `data/` and leave Laragon's untouched (default).
    Copy,
    /// Point MeowServer at Laragon's folder. Faster, but both apps then
    /// share one copy; never run them at the same time.
    InPlace,
}

/// Looks for Laragon at `root`, or `C:\laragon` by default.
pub fn detect(root: Option<&Path>) -> Option<Laragon> {
    let root = root.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(r"C:\laragon"));
    if !root.join("laragon.exe").exists() && !root.join("usr").join("laragon.ini").exists() {
        return None;
    }
    let ini = std::fs::read_to_string(root.join("usr").join("laragon.ini")).unwrap_or_default();
    let mut www: Vec<PathBuf> = ini
        .lines()
        .find_map(|l| l.strip_prefix("DocumentRootList="))
        .map(|list| {
            list.split('|').filter(|p| !p.trim().is_empty()).map(|p| PathBuf::from(p.trim())).collect()
        })
        .unwrap_or_default();
    if www.is_empty() {
        www.push(root.join("www"));
    }
    www.retain(|p| p.is_dir());

    let data_root = ini
        .lines()
        .find_map(|l| l.strip_prefix("DataDir="))
        .map(|p| PathBuf::from(p.trim()))
        .unwrap_or_else(|| root.join("data"));
    let mut databases = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&data_root) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if let Found::Data { kind: kind @ (Kind::Mysql | Kind::Mariadb), version } =
                datadir::detect(&path)
            {
                databases.push(Database {
                    kind,
                    version: version.map(|v| v.to_string()),
                    bytes: size(&path),
                    path,
                });
            }
        }
    }
    databases.sort_by(|a, b| a.path.cmp(&b.path));
    Some(Laragon { root, www, databases })
}

/// What [`apply`] would do, as lines for the user to confirm.
pub fn describe(layout: &Layout, laragon: &Laragon, mode: Mode) -> Vec<String> {
    let mut out = Vec::new();
    for dir in &laragon.www {
        out.push(format!("Serve every folder in {} as a site (parked folder).", dir.display()));
    }
    for db in &laragon.databases {
        let version = db.version.clone().unwrap_or_else(|| "unknown version".into());
        match mode {
            Mode::Copy => out.push(format!(
                "Copy {} data {} ({version}, {}) into {}.",
                db.kind.id(),
                db.path.display(),
                human(db.bytes),
                target(layout, db).display()
            )),
            Mode::InPlace => out.push(format!(
                "Use {} data in place: {} ({version}). Never run Laragon and MeowServer at the same time.",
                db.kind.id(),
                db.path.display()
            )),
        }
    }
    out
}

/// Applies the import. `progress` receives (bytes copied, total bytes).
pub fn apply(
    layout: &Layout,
    config: &mut Config,
    laragon: &Laragon,
    mode: Mode,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<String>> {
    if platform::is_running("laragon.exe") {
        bail!("Laragon is running. Quit it first so its database files are not in use.");
    }
    let mut notes = Vec::new();
    for dir in &laragon.www {
        match site::manage::park(config, dir) {
            Ok(_) => notes.push(format!("parked {}", dir.display())),
            Err(e) => notes.push(format!("skipped {}: {e:#}", dir.display())),
        }
    }
    for db in &laragon.databases {
        let line =
            db.version.as_deref().and_then(|v| v.parse::<crate::version::Version>().ok()).map(|v| v.line());
        match mode {
            Mode::Copy => {
                let dest = target(layout, db);
                if dest.exists() {
                    bail!("{} already exists; move it away or use --in-place", dest.display());
                }
                let total = db.bytes;
                let mut done = 0u64;
                let staging = dest.with_extension("importing");
                let _ = std::fs::remove_dir_all(&staging);
                copy_dir(&db.path, &staging, &mut |n| {
                    done += n;
                    progress(done, total);
                })
                .with_context(|| format!("copying {}", db.path.display()))?;
                std::fs::rename(&staging, &dest)?;
                notes.push(format!("copied {} data to {}", db.kind.id(), dest.display()));
            }
            Mode::InPlace => {
                config.services.entry(db.kind.id().into()).or_default().data_dir = Some(db.path.clone());
                notes.push(format!("{} now uses {}", db.kind.id(), db.path.display()));
            }
        }
        if let Some(line) = line {
            config.versions.insert(db.kind.id().into(), line.clone());
            if !config.general.autostart.iter().any(|a| a == db.kind.id()) {
                config.general.autostart.push(db.kind.id().into());
            }
            if service::select(layout, db.kind, Some(&line)).is_none() {
                notes.push(format!("install the matching server: meow install {} {line}", db.kind.id()));
            }
        }
    }
    Ok(notes)
}

fn target(layout: &Layout, db: &Database) -> PathBuf {
    let line =
        db.version.as_deref().and_then(|v| v.parse::<crate::version::Version>().ok()).map(|v| v.line());
    layout.data().join(format!("{}-{}", db.kind.id(), line.unwrap_or_else(|| "imported".into())))
}

fn size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .filter_map(Result::ok)
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

fn copy_dir(from: &Path, to: &Path, progress: &mut dyn FnMut(u64)) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest, progress)?;
        } else {
            let n = std::fs::copy(entry.path(), &dest)?;
            progress(n);
        }
    }
    Ok(())
}

pub fn human(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KB", b / 1024),
    }
}
