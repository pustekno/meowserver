// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Importing from other local server tools (PRD F27): Laragon, XAMPP,
//! WampServer, and FlyEnv are found automatically. Their project folders
//! are served where they are (parked or added, never moved) and their
//! MySQL/MariaDB data is copied (default) or used in place. The other
//! tool's files are only read.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{self, EXE_SUFFIX, Os, os};
use crate::service::datadir::{self, Found};
use crate::service::{self, Kind};
use crate::site::{self, manage};
use crate::version::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Laragon,
    Xampp,
    Wamp,
    FlyEnv,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Laragon, Tool::Xampp, Tool::Wamp, Tool::FlyEnv];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Laragon => "laragon",
            Tool::Xampp => "xampp",
            Tool::Wamp => "wamp",
            Tool::FlyEnv => "flyenv",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Laragon => "Laragon",
            Tool::Xampp => "XAMPP",
            Tool::Wamp => "WampServer",
            Tool::FlyEnv => "FlyEnv",
        }
    }

    pub fn parse(id: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.id() == id || t.name().eq_ignore_ascii_case(id))
    }
}

/// What was found of one tool.
#[derive(Debug, Serialize)]
pub struct Source {
    pub tool: Tool,
    pub root: PathBuf,
    /// Folders whose subfolders are all sites (Laragon's `www`).
    pub parked: Vec<PathBuf>,
    /// Individual project folders (XAMPP `htdocs`, Wamp `www`, FlyEnv sites).
    pub projects: Vec<Project>,
    pub databases: Vec<Database>,
}

#[derive(Debug, Serialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    /// PHP line the other tool used for it, when it records one.
    pub php: Option<String>,
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
    /// Copy data into `data/` and leave the other tool's untouched (default).
    Copy,
    /// Point MeowServer at the other tool's folder. Faster, but both apps
    /// then share one copy; never run them at the same time.
    InPlace,
}

impl Source {
    pub fn is_empty(&self) -> bool {
        self.parked.is_empty() && self.projects.is_empty() && self.databases.is_empty()
    }
}

/// Every supported tool found on this computer, with something to import.
pub fn detect_all() -> Vec<Source> {
    Tool::ALL.into_iter().filter_map(|t| detect(t, None)).filter(|s| !s.is_empty()).collect()
}

/// Looks for `tool` at `root`, or in its usual places on every drive.
pub fn detect(tool: Tool, root: Option<&Path>) -> Option<Source> {
    let roots: Vec<PathBuf> = match root {
        Some(r) => vec![r.to_path_buf()],
        None => candidates(tool),
    };
    roots.into_iter().find_map(|r| match tool {
        Tool::Laragon => laragon(&r),
        Tool::Xampp => xampp(&r),
        Tool::Wamp => wamp(&r),
        Tool::FlyEnv => flyenv(&r),
    })
}

fn candidates(tool: Tool) -> Vec<PathBuf> {
    let on_drives = |folders: &[&str]| -> Vec<PathBuf> {
        if os() != Os::Windows {
            return Vec::new();
        }
        ('C'..='Z')
            .flat_map(|d| folders.iter().map(move |f| PathBuf::from(format!("{d}:\\{f}"))))
            .filter(|p| p.is_dir())
            .collect()
    };
    match tool {
        Tool::Laragon => on_drives(&["laragon"]),
        Tool::Wamp => on_drives(&["wamp64", "wamp"]),
        Tool::Xampp => {
            let mut v = on_drives(&["xampp"]);
            v.extend(
                ["/opt/lampp", "/Applications/XAMPP/xamppfiles"]
                    .iter()
                    .map(PathBuf::from)
                    .filter(|p| p.is_dir()),
            );
            v
        }
        Tool::FlyEnv => flyenv_data_dirs(),
    }
}

fn laragon(root: &Path) -> Option<Source> {
    if !root.join("laragon.exe").exists() && !root.join("usr").join("laragon.ini").exists() {
        return None;
    }
    let ini = std::fs::read_to_string(root.join("usr").join("laragon.ini")).unwrap_or_default();
    let mut parked: Vec<PathBuf> = ini
        .lines()
        .find_map(|l| l.strip_prefix("DocumentRootList="))
        .map(|list| {
            list.split('|').filter(|p| !p.trim().is_empty()).map(|p| PathBuf::from(p.trim())).collect()
        })
        .unwrap_or_default();
    if parked.is_empty() {
        parked.push(root.join("www"));
    }
    parked.retain(|p| p.is_dir());
    let data_root = ini
        .lines()
        .find_map(|l| l.strip_prefix("DataDir="))
        .map(|p| PathBuf::from(p.trim()))
        .unwrap_or_else(|| root.join("data"));
    let databases = subdirs(&data_root).iter().filter_map(|d| database(d, None)).collect();
    Some(Source { tool: Tool::Laragon, root: root.to_path_buf(), parked, projects: Vec::new(), databases })
}

fn xampp(root: &Path) -> Option<Source> {
    let htdocs = root.join("htdocs");
    if !htdocs.is_dir() || !(root.join("xampp-control.exe").exists() || root.join("mysql").is_dir()) {
        return None;
    }
    // Folders XAMPP ships in htdocs; not the user's projects.
    let skip = ["dashboard", "img", "webalizer", "xampp", "forbidden", "restricted"];
    let projects = projects_in(&htdocs, &skip);
    let server = root.join("mysql").join("bin").join(format!("mysqld{EXE_SUFFIX}"));
    let databases = database(&root.join("mysql").join("data"), Some(&server)).into_iter().collect();
    Some(Source { tool: Tool::Xampp, root: root.to_path_buf(), parked: Vec::new(), projects, databases })
}

fn wamp(root: &Path) -> Option<Source> {
    let www = root.join("www");
    if !www.is_dir() || !(root.join("wampmanager.exe").exists() || root.join("bin").is_dir()) {
        return None;
    }
    let projects = projects_in(&www, &["wamplangues", "wampthemes"]);
    // bin/mysql/mysql8.3.0/data, bin/mariadb/mariadb11.3.2/data
    let mut databases = Vec::new();
    for family in ["mysql", "mariadb"] {
        for dir in subdirs(&root.join("bin").join(family)) {
            let server = dir.join("bin").join(format!("mysqld{EXE_SUFFIX}"));
            databases.extend(database(&dir.join("data"), Some(&server)));
        }
    }
    Some(Source { tool: Tool::Wamp, root: root.to_path_buf(), parked: Vec::new(), projects, databases })
}

/// FlyEnv keeps its data in `FlyEnv-Data` (older: `PhpWebStudy-Data`) two
/// levels above `FlyEnv.exe` on Windows, `~/Library/FlyEnv` on macOS, and
/// `~/.config/FlyEnv` on Linux; `FLYENV_DATA_ROOT` overrides it.
fn flyenv_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(root) = std::env::var_os("FLYENV_DATA_ROOT") {
        dirs.push(PathBuf::from(root));
    }
    match os() {
        Os::Windows => {
            let mut installs: Vec<PathBuf> = Vec::new();
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                installs.push(PathBuf::from(local).join("Programs").join("FlyEnv"));
            }
            for var in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Some(p) = std::env::var_os(var) {
                    installs.push(PathBuf::from(p).join("FlyEnv"));
                }
            }
            for install in installs {
                if let Some(parent) = install.parent() {
                    dirs.push(parent.join("FlyEnv-Data"));
                    dirs.push(parent.join("PhpWebStudy-Data"));
                }
            }
        }
        Os::MacOs | Os::Linux => {
            if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
                dirs.push(home.join("Library").join("FlyEnv"));
                dirs.push(home.join("Library").join("PhpWebStudy"));
                dirs.push(home.join(".config").join("FlyEnv"));
            }
        }
    }
    dirs.retain(|d| d.join("server").is_dir());
    dirs
}

fn flyenv(root: &Path) -> Option<Source> {
    let server = root.join("server");
    if !server.is_dir() {
        return None;
    }
    let mut projects = Vec::new();
    if let Ok(text) = std::fs::read_to_string(server.join("host.json")) {
        let hosts: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
        for h in hosts {
            let Some(docroot) = h.get("root").and_then(|r| r.as_str()).map(PathBuf::from) else { continue };
            // FlyEnv stores the document root; serve the project above
            // `public/` and let detection find the root again.
            let path = match docroot.file_name().and_then(|n| n.to_str()) {
                Some("public" | "web" | "webroot") => {
                    docroot.parent().map(Path::to_path_buf).unwrap_or(docroot)
                }
                _ => docroot,
            };
            if !path.is_dir() {
                continue;
            }
            let host = h.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            let label = host.split('.').next().filter(|l| !l.is_empty()).map(str::to_owned);
            let Some(name) = label.or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            else {
                continue;
            };
            let php = h.get("phpVersion").and_then(flyenv_php);
            projects.push(Project { name, path, php });
        }
    }
    let mut databases = Vec::new();
    for family in ["mysql", "mariadb"] {
        for dir in subdirs(&server.join(family)) {
            if dir.file_name().is_some_and(|n| n.to_string_lossy().starts_with("data-")) {
                databases.extend(database(&dir, None));
            }
        }
    }
    Some(Source { tool: Tool::FlyEnv, root: root.to_path_buf(), parked: Vec::new(), projects, databases })
}

/// FlyEnv records PHP as a number: `84` or `8.4`.
fn flyenv_php(v: &serde_json::Value) -> Option<String> {
    let n = v.as_f64()?;
    let (major, minor) = if n >= 10.0 {
        ((n as u32) / 10, (n as u32) % 10)
    } else {
        (n as u32, ((n * 10.0).round() as u32) % 10)
    };
    (major >= 5).then(|| format!("{major}.{minor}"))
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

fn projects_in(dir: &Path, skip: &[&str]) -> Vec<Project> {
    subdirs(dir)
        .into_iter()
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            !skip.contains(&name.as_str()) && !name.starts_with('.')
        })
        .map(|path| Project {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            path,
            php: None,
        })
        .collect()
}

/// A MySQL/MariaDB data folder, recognised from its own files, else from
/// the server binary next to it (`mysqld --version`), else from the folder
/// name (`mysql8.3.0`, `data-8.4`).
fn database(dir: &Path, server: Option<&Path>) -> Option<Database> {
    if !dir.join("mysql").is_dir() {
        return None;
    }
    let (kind, version) = match datadir::detect(dir) {
        Found::Data { kind: kind @ (Kind::Mysql | Kind::Mariadb), version } => {
            (kind, version.or_else(|| server.and_then(binary_version).map(|(_, v)| v)))
        }
        Found::Data { .. } | Found::Empty => return None,
        Found::Unknown => match server.and_then(binary_version) {
            Some((kind, version)) => (kind, Some(version)),
            None => {
                let label =
                    dir.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_lowercase());
                let named = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                let kind = if label.as_deref().is_some_and(|l| l.contains("mariadb")) {
                    Kind::Mariadb
                } else {
                    Kind::Mysql
                };
                let digits =
                    |s: &str| s.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<Version>().ok();
                (kind, digits(&named).or_else(|| label.as_deref().and_then(digits)))
            }
        },
    };
    Some(Database {
        kind,
        version: version.map(|v| v.to_string()),
        path: dir.to_path_buf(),
        bytes: size(dir),
    })
}

/// `mysqld --version` → "… Ver 10.4.32-MariaDB …" or "… Ver 8.0.31 for …".
fn binary_version(server: &Path) -> Option<(Kind, Version)> {
    let out =
        Command::new(server).arg("--version").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let after = text.split("Ver ").nth(1)?;
    let raw = after.split_whitespace().next()?;
    let kind = if text.contains("MariaDB") { Kind::Mariadb } else { Kind::Mysql };
    Some((kind, raw.split('-').next()?.parse().ok()?))
}

/// A database server of `source` is running, so its data files are in use
/// and a copy would be torn. Only matters when there is data to import.
pub fn in_use(source: &Source) -> bool {
    let root = source.root.to_string_lossy().to_lowercase();
    !source.databases.is_empty() && platform::processes_under(&root, &["mysqld", "mariadbd", "postgres"])
}

/// What [`apply`] would do, as lines for the user to confirm.
pub fn describe(layout: &Layout, source: &Source, mode: Mode) -> Vec<String> {
    let mut out = Vec::new();
    for dir in &source.parked {
        out.push(format!("Serve every folder in {} as a site (where it is, nothing moved).", dir.display()));
    }
    for p in &source.projects {
        let php = p.php.as_deref().map(|v| format!(", PHP {v}")).unwrap_or_default();
        out.push(format!("Serve {} as {}{php} (where it is, nothing moved).", p.path.display(), p.name));
    }
    for db in &source.databases {
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
                "Use {} data in place: {} ({version}). Never run {} and MeowServer at the same time.",
                db.kind.id(),
                db.path.display(),
                source.tool.name()
            )),
        }
    }
    out
}

/// Applies the import. `progress` receives (bytes copied, total bytes).
pub fn apply(
    layout: &Layout,
    config: &mut Config,
    source: &Source,
    mode: Mode,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<String>> {
    if in_use(source) {
        bail!(
            "a database server from {} is running. Quit {} first so its data files are not in use.",
            source.root.display(),
            source.tool.name()
        );
    }
    let mut notes = Vec::new();
    for dir in &source.parked {
        match manage::park(config, dir) {
            Ok(_) => notes.push(format!("serving every folder in {}", dir.display())),
            Err(e) => notes.push(format!("skipped {}: {e:#}", dir.display())),
        }
    }
    for p in &source.projects {
        match manage::link(layout, config, &p.path, Some(&p.name)) {
            Ok(name) => {
                if let Some(php) = &p.php {
                    manage::settings(config, &name).php = Some(php.clone());
                }
                notes.push(format!("{} → {name}.{}", p.path.display(), config.general.suffix.as_str()));
            }
            Err(e) => notes.push(format!("skipped {}: {e:#}", p.path.display())),
        }
    }
    let total: u64 = source.databases.iter().map(|d| d.bytes).sum();
    let mut done = 0u64;
    for db in &source.databases {
        let line = db.version.as_deref().and_then(|v| v.parse::<Version>().ok()).map(|v| v.line());
        match mode {
            Mode::Copy => {
                let dest = target(layout, db);
                if dest.exists() {
                    notes.push(format!("skipped {}: {} already exists", db.path.display(), dest.display()));
                    continue;
                }
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
    let _ = site::discover(layout, config);
    Ok(notes)
}

fn target(layout: &Layout, db: &Database) -> PathBuf {
    let line = db.version.as_deref().and_then(|v| v.parse::<Version>().ok()).map(|v| v.line());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("meow-import-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn xampp_projects_skip_bundled_folders() {
        let root = tmp("xampp");
        for f in ["htdocs/dashboard", "htdocs/img", "htdocs/shop", "htdocs/blog", "mysql/data/mysql"] {
            std::fs::create_dir_all(root.join(f)).unwrap();
        }
        std::fs::write(root.join("mysql/data/mysql_upgrade_info"), "10.4.32-MariaDB").unwrap();
        let s = xampp(&root).unwrap();
        let names: Vec<&str> = s.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["blog", "shop"]);
        assert_eq!(s.databases.len(), 1);
        assert_eq!(s.databases[0].kind, Kind::Mariadb);
        assert_eq!(s.databases[0].version.as_deref(), Some("10.4.32"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wamp_versions_come_from_folder_names() {
        let root = tmp("wamp");
        for f in [
            "www/app",
            "www/wampthemes",
            "bin/mysql/mysql8.3.0/data/mysql",
            "bin/mariadb/mariadb11.3.2/data/mysql",
        ] {
            std::fs::create_dir_all(root.join(f)).unwrap();
        }
        let s = wamp(&root).unwrap();
        assert_eq!(s.projects.len(), 1);
        let dbs: Vec<(Kind, Option<&str>)> =
            s.databases.iter().map(|d| (d.kind, d.version.as_deref())).collect();
        assert!(dbs.contains(&(Kind::Mysql, Some("8.3.0"))));
        assert!(dbs.contains(&(Kind::Mariadb, Some("11.3.2"))));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn flyenv_sites_and_php_versions() {
        let root = tmp("flyenv");
        let project = root.join("projects").join("shop");
        std::fs::create_dir_all(project.join("public")).unwrap();
        std::fs::create_dir_all(root.join("server/mysql/data-8.4/mysql")).unwrap();
        let docroot = project.join("public").display().to_string().replace('\\', "/");
        let hosts = format!(r#"[{{"name": "shop.test", "root": "{docroot}", "phpVersion": 84}}]"#);
        std::fs::write(root.join("server/host.json"), hosts).unwrap();
        let s = flyenv(&root).unwrap();
        assert_eq!(s.projects[0].name, "shop");
        assert_eq!(s.projects[0].php.as_deref(), Some("8.4"));
        assert!(s.projects[0].path.ends_with("shop"), "served from the project, not public/");
        assert_eq!(s.databases[0].version.as_deref(), Some("8.4"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn flyenv_php_number_forms() {
        assert_eq!(flyenv_php(&serde_json::json!(84)).as_deref(), Some("8.4"));
        assert_eq!(flyenv_php(&serde_json::json!(8.4)).as_deref(), Some("8.4"));
        assert_eq!(flyenv_php(&serde_json::json!(74)).as_deref(), Some("7.4"));
    }
}
