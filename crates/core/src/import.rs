// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Importing from other local server tools (PRD F27): Laragon, XAMPP,
//! WampServer, and FlyEnv are found automatically. Their project folders
//! are served where they are (parked or added, never moved) and their
//! MySQL/MariaDB data is copied (default) or used in place. The other
//! tool's files are only read; its database servers are stopped only when
//! the user asks (`stop_databases`).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config::{Config, Suffix};
use crate::ipc::{Client, Request};
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
    /// Folders outside the tool whose subfolders are all sites, parked as
    /// they are. The tool's own `www` is taken apart into `projects`
    /// instead, so nothing depends on the tool's folder afterwards.
    pub parked: Vec<PathBuf>,
    /// Individual projects (Laragon's and Wamp's `www`, XAMPP's `htdocs`,
    /// FlyEnv's sites).
    pub projects: Vec<Project>,
    pub databases: Vec<Database>,
    /// The PHP line the tool runs, so projects keep the same version.
    pub php: Option<String>,
    /// Links in the tool's `www` whose target is gone, as text to show.
    pub broken: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Project {
    pub name: String,
    /// The real folder: a link in the tool's `www` is followed.
    pub path: PathBuf,
    /// PHP line the other tool used for it, when it records one.
    pub php: Option<String>,
    /// Stored inside the tool's folder, so it is copied into MeowServer's
    /// `www` (uninstalling the tool would delete it). Otherwise it is
    /// served where it is.
    pub copy: bool,
    pub bytes: u64,
    /// Domain ending the tool gave it (`.test` for Laragon), kept so URLs
    /// in the project's settings and database still work.
    pub suffix: Option<Suffix>,
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

    fn new(tool: Tool, root: &Path) -> Self {
        Source {
            tool,
            root: root.to_path_buf(),
            parked: Vec::new(),
            projects: Vec::new(),
            databases: Vec::new(),
            php: None,
            broken: Vec::new(),
        }
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
    let mut source = Source::new(Tool::Laragon, root);
    // Laragon names every site `<folder>.test` unless told otherwise.
    for dir in parked {
        if platform::is_inside(&dir.to_string_lossy(), root) {
            projects_in(&mut source, &dir, &[], Some(Suffix::Test));
        } else {
            source.parked.push(dir);
        }
    }
    source.databases = subdirs(&data_root).iter().filter_map(|d| database(d, None)).collect();
    source.php = laragon_php(root, &ini);
    Some(source)
}

/// The PHP line Laragon serves sites with: `[php] Version=` when set, else
/// the build its Apache loads, else the newest folder in `bin\php`. (PHP
/// processes seen running may be command-line ones from an editor.)
fn laragon_php(root: &Path, ini: &str) -> Option<String> {
    let line_of = |name: &str| {
        let digits = name.trim_start_matches(|c: char| !c.is_ascii_digit());
        digits.split(['-', '_']).next()?.parse::<Version>().ok().map(|v| v.line())
    };
    let mut in_php = false;
    for l in ini.lines().map(str::trim) {
        if l.starts_with('[') {
            in_php = l.eq_ignore_ascii_case("[php]");
        } else if in_php && let Some(v) = l.strip_prefix("Version=") {
            return line_of(v);
        }
    }
    // The build Apache loads: LoadModule php_module "C:/laragon/bin/php/php-8.5.10-…/…dll"
    let module =
        std::fs::read_to_string(root.join("etc").join("apache2").join("mod_php.conf")).unwrap_or_default();
    if let Some(line) = module
        .lines()
        .filter(|l| l.trim_start().starts_with("LoadModule"))
        .find_map(|l| l.split(['/', '\\']).find(|part| part.starts_with("php-")).and_then(line_of))
    {
        return Some(line);
    }
    subdirs(&root.join("bin").join("php"))
        .iter()
        .filter_map(|d| line_of(&d.file_name()?.to_string_lossy()))
        .max_by_key(|l| l.parse::<Version>().ok())
}

fn xampp(root: &Path) -> Option<Source> {
    let htdocs = root.join("htdocs");
    if !htdocs.is_dir() || !(root.join("xampp-control.exe").exists() || root.join("mysql").is_dir()) {
        return None;
    }
    // Folders XAMPP ships in htdocs; not the user's projects.
    let skip = ["dashboard", "img", "webalizer", "xampp", "forbidden", "restricted"];
    let mut source = Source::new(Tool::Xampp, root);
    projects_in(&mut source, &htdocs, &skip, None);
    let server = root.join("mysql").join("bin").join(format!("mysqld{EXE_SUFFIX}"));
    let databases = database(&root.join("mysql").join("data"), Some(&server)).into_iter().collect();
    source.databases = databases;
    Some(source)
}

fn wamp(root: &Path) -> Option<Source> {
    let www = root.join("www");
    if !www.is_dir() || !(root.join("wampmanager.exe").exists() || root.join("bin").is_dir()) {
        return None;
    }
    let mut source = Source::new(Tool::Wamp, root);
    projects_in(&mut source, &www, &["wamplangues", "wampthemes"], None);
    // bin/mysql/mysql8.3.0/data, bin/mariadb/mariadb11.3.2/data
    let mut databases = Vec::new();
    for family in ["mysql", "mariadb"] {
        for dir in subdirs(&root.join("bin").join(family)) {
            let server = dir.join("bin").join(format!("mysqld{EXE_SUFFIX}"));
            databases.extend(database(&dir.join("data"), Some(&server)));
        }
    }
    source.databases = databases;
    Some(source)
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
            let suffix = host.ends_with(".test").then_some(Suffix::Test);
            projects.push(Project { name, path, php, copy: false, bytes: 0, suffix });
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
    let mut source = Source::new(Tool::FlyEnv, root);
    source.projects = projects;
    source.databases = databases;
    Some(source)
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
        .map(|e| e.path())
        .filter(|p| p.is_dir()) // follows junctions and symbolic links
        .collect();
    out.sort();
    out
}

/// The projects in a tool's `www`. A folder stored there is copied on
/// import; a link (how Laragon users bring in projects from another drive)
/// is followed and served where it points; a broken link is reported.
fn projects_in(source: &mut Source, dir: &Path, skip: &[&str], suffix: Option<Suffix>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<std::fs::DirEntry> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_lowercase();
        if skip.contains(&lower.as_str()) || lower.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let link = entry.file_type().is_ok_and(|t| t.is_symlink());
        if !path.is_dir() {
            if link {
                let target = std::fs::read_link(&path).unwrap_or_default();
                source.broken.push(format!("{} -> {}", path.display(), target.display()));
            }
            continue;
        }
        let (path, copy) = if link { (real_path(&path), false) } else { (path, true) };
        let bytes = if copy { size(&path) } else { 0 };
        source.projects.push(Project { name, path, php: None, copy, bytes, suffix });
    }
}

/// Where a link really points, without the `\\?\` prefix Windows adds.
fn real_path(path: &Path) -> PathBuf {
    let Ok(real) = std::fs::canonicalize(path) else { return path.to_path_buf() };
    match real.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => real,
    }
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
    let out = crate::platform::command(server)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let after = text.split("Ver ").nth(1)?;
    let raw = after.split_whitespace().next()?;
    let kind = if text.contains("MariaDB") { Kind::Mariadb } else { Kind::Mysql };
    Some((kind, raw.split('-').next()?.parse().ok()?))
}

/// A database server of `source` is running, so its data files are in use
/// and a copy would be torn. Only matters when there is data to import.
pub fn in_use(source: &Source) -> bool {
    !source.databases.is_empty() && platform::processes_under(&source.root, DATABASE_SERVERS)
}

const DATABASE_SERVERS: &[&str] = &["mysqld", "mariadbd", "postgres"];

/// Stops the database servers running from `source`'s folder, so their
/// data can be copied. Closing Laragon leaves its MySQL running, for one.
/// MySQL and MariaDB on the standard port are asked to shut down first
/// (`mysqladmin`, as root without a password, the tools' default); what is
/// still running after that is ended. InnoDB and PostgreSQL recover on
/// their next start, as after a power cut. Nothing else is touched.
pub fn stop_databases(source: &Source) -> Vec<String> {
    let running = platform::processes_in(&source.root, DATABASE_SERVERS);
    if running.is_empty() {
        return Vec::new();
    }
    let mut notes = Vec::new();
    let mut asked = false;
    let on_standard_port: Vec<u32> = platform::port_owners(3306).into_iter().map(|(pid, _)| pid).collect();
    for (pid, path) in &running {
        let admin = Path::new(path).with_file_name(format!("mysqladmin{EXE_SUFFIX}"));
        if on_standard_port.contains(pid) && admin.is_file() {
            let ok = crate::platform::command(&admin)
                .args(["-u", "root", "-h", "127.0.0.1", "-P", "3306", "--connect-timeout=5", "shutdown"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if ok {
                asked = true;
                notes.push(format!("asked {path} to shut down"));
            }
        }
    }
    let gone = |deadline: Duration| {
        let until = Instant::now() + deadline;
        while running.iter().any(|(pid, _)| platform::is_alive(*pid)) {
            if Instant::now() > until {
                return false;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        true
    };
    // A clean shutdown flushes InnoDB and can take a while; give it time.
    if !gone(Duration::from_secs(if asked { 30 } else { 0 })) {
        for (pid, path) in running.iter().filter(|(pid, _)| platform::is_alive(*pid)) {
            platform::terminate(*pid);
            notes.push(format!("ended {path} (pid {pid})"));
        }
        gone(Duration::from_secs(10));
    }
    notes
}

/// What [`apply`] would do, as lines for the user to confirm.
pub fn describe(layout: &Layout, source: &Source, mode: Mode) -> Vec<String> {
    let mut out = Vec::new();
    for dir in &source.parked {
        out.push(format!("Serve every folder in {} as a site (where it is, nothing moved).", dir.display()));
    }
    let www = layout.root().join("www");
    for p in &source.projects {
        let php = p.php.as_deref().map(|v| format!(", PHP {v}")).unwrap_or_default();
        let host = format!(
            "{}.{}",
            site::slugify(&p.name).unwrap_or_default(),
            p.suffix.unwrap_or(Suffix::Localhost).as_str()
        );
        if p.copy {
            out.push(format!(
                "Copy {} ({}) into {} and serve it as {host}{php}.",
                p.path.display(),
                human(p.bytes),
                www.join(&p.name).display()
            ));
        } else {
            out.push(format!("Serve {} as {host}{php} (where it is, nothing moved).", p.path.display()));
        }
    }
    for link in &source.broken {
        out.push(format!("Skip {link}: the link's target does not exist."));
    }
    if let Some(php) = &source.php {
        out.push(format!("Make PHP {php} the default, as in {}.", source.tool.name()));
    }
    for db in &source.databases {
        let version = db.version.clone().unwrap_or_else(|| "unknown version".into());
        match mode {
            Mode::Copy => out.push(match existing(layout, db) {
                Existing::Databases(names) => format!(
                    "Skip {} data {} ({version}): {} already has databases ({}).",
                    db.kind.id(),
                    db.path.display(),
                    target(layout, db).display(),
                    names.join(", ")
                ),
                found => format!(
                    "Copy {} data {} ({version}, {}) into {}{}.",
                    db.kind.id(),
                    db.path.display(),
                    human(db.bytes),
                    target(layout, db).display(),
                    if matches!(found, Existing::Empty) {
                        ", setting MeowServer's empty one aside"
                    } else {
                        ""
                    }
                ),
            }),
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
    // The tool's own www, parked by an earlier import: its projects are
    // now added one by one, so they must not show up twice.
    let inside: Vec<PathBuf> = config
        .general
        .parked
        .iter()
        .filter(|p| platform::is_inside(&p.to_string_lossy(), &source.root))
        .cloned()
        .collect();
    for dir in inside {
        if manage::unpark(config, &dir).is_ok() {
            notes.push(format!("{} is no longer parked; its projects are added one by one", dir.display()));
        }
    }
    let copies_data =
        |db: &&Database| mode == Mode::Copy && !matches!(existing(layout, db), Existing::Databases(_));
    let total: u64 = source.databases.iter().filter(copies_data).map(|d| d.bytes).sum::<u64>()
        + source
            .projects
            .iter()
            .filter(|p| p.copy && !www_of(layout, config).join(&p.name).exists())
            .map(|p| p.bytes)
            .sum::<u64>();
    let mut done = 0u64;
    let www = layout.resolve(&config.general.www);
    for p in &source.projects {
        let name = if p.copy {
            let dest = www.join(&p.name);
            if dest.exists() {
                notes.push(format!("skipped {}: {} already exists", p.path.display(), dest.display()));
                continue;
            }
            let staging = sibling(&dest, ".importing");
            let _ = std::fs::remove_dir_all(&staging);
            let copied = copy_project(&p.path, &staging, &mut |n| {
                done += n;
                progress(done, total);
            })
            .and_then(|left_out| {
                std::fs::rename(&staging, &dest)?;
                Ok(left_out)
            });
            let left_out = match copied {
                Ok(left_out) => left_out,
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&staging);
                    return Err(e.context(format!("copying {}", p.path.display())));
                }
            };
            notes.push(format!("copied {} to {}", p.path.display(), dest.display()));
            for file in left_out {
                notes.push(format!(
                    "not copied (this account may not read it; copy it yourself): {}",
                    file.display()
                ));
            }
            match site::slugify(&p.name) {
                Some(name) => name,
                None => continue,
            }
        } else {
            match manage::link(layout, config, &p.path, Some(&p.name)) {
                Ok(name) => name,
                Err(e) => {
                    notes.push(format!("skipped {}: {e:#}", p.path.display()));
                    continue;
                }
            }
        };
        let settings = manage::settings(config, &name);
        if let Some(php) = &p.php {
            settings.php = Some(php.clone());
        }
        if let Some(suffix) = p.suffix {
            settings.suffix = Some(suffix);
        }
        let suffix = p.suffix.unwrap_or(config.general.suffix);
        notes.push(format!("{} → {name}.{}", p.path.display(), suffix.as_str()));
    }
    if let Some(line) = &source.php {
        if service::select(layout, Kind::Php, Some(line)).is_some() {
            config.versions.insert("php".into(), line.clone());
            notes.push(format!("PHP {line} is now the default, as in {}", source.tool.name()));
        } else {
            notes.push(format!(
                "install PHP {line}, the version {} used: meow install php {line}",
                source.tool.name()
            ));
        }
    }
    // MeowServer's own database servers stopped to swap their data.
    let mut restart: Vec<&str> = Vec::new();
    for db in &source.databases {
        let mut aside: Option<PathBuf> = None;
        let line = db.version.as_deref().and_then(|v| v.parse::<Version>().ok()).map(|v| v.line());
        match mode {
            Mode::Copy => {
                let dest = target(layout, db);
                match existing(layout, db) {
                    Existing::None => {}
                    Existing::Databases(names) => {
                        notes.push(format!(
                            "skipped {}: MeowServer's {} already has databases ({})",
                            db.path.display(),
                            dest.display(),
                            names.join(", ")
                        ));
                        continue;
                    }
                    // A fresh server with only its system schemas: set it
                    // aside (not deleted: it may hold accounts) and take
                    // the imported data instead. Its files must be closed.
                    Existing::Empty => {
                        if let Ok(mut client) = Client::connect(layout)
                            && client.call(Request::Stop { unit: db.kind.id().into() }).is_ok()
                        {
                            restart.push(db.kind.id());
                        }
                        let target = unused(&sibling(&dest, ".before-import"));
                        std::fs::rename(&dest, &target).with_context(|| {
                            format!(
                                "setting aside {} (is MeowServer's {} still running?)",
                                dest.display(),
                                db.kind.id()
                            )
                        })?;
                        aside = Some(target);
                    }
                }
                let staging = sibling(&dest, ".importing");
                let _ = std::fs::remove_dir_all(&staging);
                let copied = copy_dir(&db.path, &staging, &mut |n| {
                    done += n;
                    progress(done, total);
                })
                .and_then(|()| Ok(std::fs::rename(&staging, &dest)?));
                if let Err(e) = copied {
                    // Put MeowServer's own data back as it was.
                    let _ = std::fs::remove_dir_all(&staging);
                    if let Some(aside) = aside.take() {
                        let _ = std::fs::rename(&aside, &dest);
                    }
                    restart_units(layout, &restart);
                    return Err(e.context(format!("copying {}", db.path.display())));
                }
                if let Some(aside) = aside.take() {
                    notes.push(format!(
                        "MeowServer's empty {} data was set aside as {}",
                        db.kind.id(),
                        aside.display()
                    ));
                }
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
                notes.push(format!(
                    "install the matching server. {}",
                    crate::provider::install_advice(db.kind, Some(&line), layout)
                ));
            }
        }
    }
    restart_units(layout, &restart);
    let _ = site::discover(layout, config);
    Ok(notes)
}

fn restart_units(layout: &Layout, units: &[&str]) {
    if units.is_empty() {
        return;
    }
    if let Ok(mut client) = Client::connect(layout) {
        for unit in units {
            if let Err(e) = client.call(Request::Start { unit: (*unit).into() }) {
                log::warn!("import: starting {unit} again: {e:#}");
            }
        }
    }
}

/// Where a copy of `db` goes: `data/<kind>-<line>`.
pub fn target(layout: &Layout, db: &Database) -> PathBuf {
    let line = db.version.as_deref().and_then(|v| v.parse::<Version>().ok()).map(|v| v.line());
    layout.data().join(format!("{}-{}", db.kind.id(), line.unwrap_or_else(|| "imported".into())))
}

/// What is already at [`target`].
pub enum Existing {
    None,
    /// A server that was started but never used: only its own schemas.
    Empty,
    /// Holds these databases; an import must not replace it.
    Databases(Vec<String>),
}

pub fn existing(layout: &Layout, db: &Database) -> Existing {
    let dest = target(layout, db);
    if !dest.is_dir() {
        return Existing::None;
    }
    // Every MySQL/MariaDB database is a folder; these come with the server.
    const SYSTEM: [&str; 4] = ["mysql", "performance_schema", "sys", "information_schema"];
    let names: Vec<String> = subdirs(&dest)
        .iter()
        .filter_map(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .filter(|n| !n.starts_with('#') && !SYSTEM.contains(&n.as_str()))
        .collect();
    if names.is_empty() { Existing::Empty } else { Existing::Databases(names) }
}

/// `path` plus a suffix (not `with_extension`: `mysql-8.4` has no
/// extension, but `with_extension` would replace its `.4`).
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// `path`, or `path-2`, `path-3`, … if it is taken.
fn unused(path: &Path) -> PathBuf {
    (1..)
        .map(|n| if n == 1 { path.to_path_buf() } else { sibling(path, &format!("-{n}")) })
        .find(|p| !p.exists())
        .unwrap()
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

fn www_of(layout: &Layout, config: &Config) -> PathBuf {
    layout.resolve(&config.general.www)
}

/// Copies a database folder: every file or nothing.
fn copy_dir(from: &Path, to: &Path, progress: &mut dyn FnMut(u64)) -> Result<()> {
    copy_tree(from, to, (from, to), progress, &mut None)
}

/// Copies a project folder. A file this account may not read (a private
/// key locked down by its owner) is left out and listed, instead of
/// stopping the whole copy.
fn copy_project(from: &Path, to: &Path, progress: &mut dyn FnMut(u64)) -> Result<Vec<PathBuf>> {
    let mut skipped = Some(Vec::new());
    copy_tree(from, to, (from, to), progress, &mut skipped)?;
    Ok(skipped.unwrap_or_default())
}

/// Copies `from` into `to`; `roots` maps a link pointing inside the copied
/// tree to the same place in the copy. With `skipped`, files that cannot
/// be read for lack of permission are listed there instead of failing.
fn copy_tree(
    from: &Path,
    to: &Path,
    roots: (&Path, &Path),
    progress: &mut dyn FnMut(u64),
    skipped: &mut Option<Vec<PathBuf>>,
) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            copy_link(&entry.path(), &dest, roots, progress)?;
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &dest, roots, progress, skipped)?;
        } else {
            match std::fs::copy(entry.path(), &dest) {
                Ok(n) => progress(n),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && skipped.is_some() => {
                    skipped.as_mut().unwrap().push(entry.path());
                }
                Err(e) => {
                    return Err(anyhow::Error::from(e).context(format!("copying {}", entry.path().display())));
                }
            }
        }
    }
    Ok(())
}

/// A link inside a copied project, such as Laravel's `public/storage`:
/// pointing into the project, it is made again pointing into the copy;
/// pointing elsewhere, at the same place. A file link becomes a copy of
/// the file; a broken link is left out.
fn copy_link(
    link: &Path,
    dest: &Path,
    (src, dst): (&Path, &Path),
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let Ok(target) = std::fs::read_link(link) else { return Ok(()) };
    let target =
        real_path(&if target.is_absolute() { target } else { link.parent().unwrap_or(src).join(target) });
    let src = real_path(src);
    let mapped = target.strip_prefix(&src).map(|rest| dst.join(rest)).unwrap_or_else(|_| target.clone());
    if target.is_dir() {
        if !make_dir_link(&mapped, dest) {
            log::warn!("import: could not recreate the link {} -> {}", dest.display(), mapped.display());
        }
    } else if target.is_file() {
        progress(std::fs::copy(&target, dest)?);
    }
    Ok(())
}

/// A directory link: a junction on Windows (no administrator rights
/// needed; the target may not exist yet), a symbolic link elsewhere.
fn make_dir_link(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    {
        // `mklink` takes backslashes only.
        let native = |p: &Path| p.to_string_lossy().replace('/', "\\");
        crate::platform::command("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(native(link))
            .arg(native(target))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
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
    fn empty_meowserver_data_is_set_aside_but_used_data_is_kept() {
        let root = tmp("swap");
        let xampp_root = root.join("xampp");
        for f in ["htdocs", "mysql/data/mysql", "mysql/data/shop"] {
            std::fs::create_dir_all(xampp_root.join(f)).unwrap();
        }
        std::fs::write(xampp_root.join("mysql/data/mysql_upgrade_info"), "10.4.32-MariaDB").unwrap();
        std::fs::write(xampp_root.join("mysql/data/shop/orders.ibd"), "rows").unwrap();
        let layout = Layout::at(root.join("meow"));
        let source = xampp(&xampp_root).unwrap();
        let dest = layout.data().join("mariadb-10.4");

        // A server that was started once: only its own schemas.
        for f in ["mysql", "sys", "#innodb_temp"] {
            std::fs::create_dir_all(dest.join(f)).unwrap();
        }
        assert!(matches!(existing(&layout, &source.databases[0]), Existing::Empty));
        let mut config = Config::default();
        apply(&layout, &mut config, &source, Mode::Copy, &mut |_, _| {}).unwrap();
        assert!(dest.join("shop").join("orders.ibd").is_file(), "imported");
        assert!(layout.data().join("mariadb-10.4.before-import").join("mysql").is_dir(), "kept aside");

        // Now it holds `shop`: a second import must not replace it.
        std::fs::write(dest.join("shop").join("orders.ibd"), "newer rows").unwrap();
        assert!(matches!(existing(&layout, &source.databases[0]), Existing::Databases(_)));
        let notes = apply(&layout, &mut config, &source, Mode::Copy, &mut |_, _| {}).unwrap();
        assert!(notes.iter().any(|n| n.contains("already has databases")), "{notes:?}");
        assert_eq!(std::fs::read_to_string(dest.join("shop").join("orders.ibd")).unwrap(), "newer rows");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copied_project_links_point_into_the_copy() {
        let root = tmp("links");
        let project = root.join("shop");
        std::fs::create_dir_all(project.join("storage/app/public")).unwrap();
        std::fs::create_dir_all(project.join("public")).unwrap();
        std::fs::write(project.join("storage/app/public/logo.png"), "png").unwrap();
        // Laravel's `php artisan storage:link`.
        assert!(make_dir_link(&project.join("storage/app/public"), &project.join("public/storage")));

        let copy = root.join("copy");
        copy_dir(&project, &copy, &mut |_| {}).unwrap();
        let link = copy.join("public/storage");
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "still a link");
        assert_eq!(real_path(&link), real_path(&copy.join("storage/app/public")), "into the copy");
        assert!(link.join("logo.png").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn flyenv_php_number_forms() {
        assert_eq!(flyenv_php(&serde_json::json!(84)).as_deref(), Some("8.4"));
        assert_eq!(flyenv_php(&serde_json::json!(8.4)).as_deref(), Some("8.4"));
        assert_eq!(flyenv_php(&serde_json::json!(74)).as_deref(), Some("7.4"));
    }
}
