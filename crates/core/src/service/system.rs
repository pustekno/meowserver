// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Versions installed outside MeowServer: on `PATH`, or in the folders the
//! official installers use. They show as "System" next to MeowServer's own.
//! MeowServer never changes or removes them. Projects can run on a system
//! Node.js, Python, or Go (PRD F43); PHP and databases are only listed,
//! since MeowServer runs its own copies of those.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use super::{Installed, Kind};
use crate::layout::Layout;
use crate::platform::{EXE_SUFFIX, Os, is_inside, os};
use crate::version::Version;

/// Versions of `kind` found on this computer outside MeowServer's folder,
/// newest first.
pub fn found(layout: &Layout, kind: Kind) -> Vec<Installed> {
    let Some(probe) = probe(kind) else { return Vec::new() };
    let mut out: Vec<Installed> = Vec::new();
    for folder in candidates(kind) {
        let exe = folder.join(format!("{}{EXE_SUFFIX}", probe.program));
        if !exe.is_file() || is_inside(&exe.to_string_lossy(), layout.root()) {
            continue;
        }
        let Some(output) = version_output(&exe, probe.args) else { continue };
        // MariaDB's server is also `mysqld`; it says so in its version.
        let mariadb = output.contains("MariaDB");
        if (kind == Kind::Mysql && mariadb) || (kind == Kind::Mariadb && !mariadb) {
            continue;
        }
        let Some(version) = parse_version(&output) else { continue };
        let dir =
            if probe.in_bin { folder.parent().map_or(folder.clone(), Path::to_path_buf) } else { folder };
        if !out.iter().any(|i| same_dir(&i.dir, &dir)) {
            out.push(Installed { kind, version, dir });
        }
    }
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

struct Probe {
    program: &'static str,
    args: &'static [&'static str],
    /// The program sits in `<dir>/bin`, as with MeowServer's own copies.
    in_bin: bool,
}

fn probe(kind: Kind) -> Option<Probe> {
    let windows = os() == Os::Windows;
    let (program, args, in_bin): (_, &[&str], _) = match kind {
        // Node and Python on Windows keep the program at the top, like the
        // archives MeowServer downloads; elsewhere it is in bin.
        Kind::Node => ("node", &["--version"], !windows),
        Kind::Python => (if windows { "python" } else { "python3" }, &["--version"], !windows),
        Kind::Go => ("go", &["version"], true),
        Kind::Php => ("php", &["-n", "-v"], false),
        Kind::Mysql | Kind::Mariadb => ("mysqld", &["--version"], true),
        Kind::Postgres => ("postgres", &["--version"], true),
        _ => return None,
    };
    Some(Probe { program, args, in_bin })
}

/// Folders that may hold the program: `PATH`, then where the official
/// Windows installers and the usual hand or package-manager setups put it.
fn candidates(kind: Kind) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if os() == Os::Windows {
        let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let program_files = env("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into());
        let local = env("LOCALAPPDATA");
        match kind {
            Kind::Node => dirs.push(program_files.join("nodejs")),
            Kind::Go => dirs.push(program_files.join("Go").join("bin")),
            Kind::Python => {
                if let Some(local) = &local {
                    dirs.extend(subfolders(&local.join("Programs").join("Python"), "Python3"));
                }
                dirs.extend(subfolders(&program_files, "Python3"));
            }
            Kind::Mysql => {
                let base = program_files.join("MySQL");
                dirs.extend(subfolders(&base, "MySQL Server").into_iter().map(|d| d.join("bin")));
            }
            Kind::Mariadb => {
                dirs.extend(subfolders(&program_files, "MariaDB").into_iter().map(|d| d.join("bin")));
            }
            Kind::Postgres => {
                let base = program_files.join("PostgreSQL");
                dirs.extend(subfolders(&base, "").into_iter().map(|d| d.join("bin")));
            }
            _ => {}
        }
        // Unpacked by hand (`C:\php84`), by Chocolatey (`C:\tools\php84`),
        // or by Scoop (`~\scoop\apps\php84\current`).
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let drive = PathBuf::from(format!(r"{drive}\"));
        let scoop = env("SCOOP").or_else(|| env("USERPROFILE").map(|h| h.join("scoop")));
        let in_bin = probe(kind).is_some_and(|p| p.in_bin);
        for prefix in folder_prefixes(kind) {
            let mut found = subfolders(&drive, prefix);
            found.extend(subfolders(&drive.join("tools"), prefix));
            if let Some(scoop) = &scoop {
                found.extend(subfolders(&scoop.join("apps"), prefix).into_iter().map(|d| d.join("current")));
            }
            dirs.extend(found.into_iter().map(|d| if in_bin { d.join("bin") } else { d }));
        }
    }
    // Windows' `python.exe` in WindowsApps opens the Store instead of
    // running anything.
    dirs.retain(|d| !d.to_string_lossy().to_lowercase().contains("windowsapps"));
    let mut unique: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        if !unique.iter().any(|u| same_dir(u, &dir)) {
            unique.push(dir);
        }
    }
    unique
}

/// How folders holding `kind` are usually named when unpacked by hand.
fn folder_prefixes(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Php => &["php"],
        Kind::Mysql => &["mysql"],
        Kind::Mariadb => &["mariadb"],
        Kind::Postgres => &["pgsql", "postgres"],
        Kind::Node => &["node"],
        Kind::Python => &["python"],
        Kind::Go => &["go"],
        _ => &[],
    }
}

/// Folders in `dir` whose name starts with `prefix`, ignoring case.
fn subfolders(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let prefix = prefix.to_lowercase();
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| e.file_name().to_string_lossy().to_lowercase().starts_with(&prefix))
        .map(|e| e.path())
        .collect()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let s = p.to_string_lossy().trim_end_matches(['\\', '/']).to_owned();
        if os() == Os::Windows { s.to_lowercase() } else { s }
    };
    norm(a) == norm(b)
}

/// What `exe args` prints, remembered until the program file changes:
/// pages and site scans ask often, and starting a program each time is slow.
fn version_output(exe: &Path, args: &[&str]) -> Option<String> {
    type Seen = HashMap<PathBuf, (SystemTime, Option<String>)>;
    static SEEN: Mutex<Option<Seen>> = Mutex::new(None);
    let modified = std::fs::metadata(exe).and_then(|m| m.modified()).ok()?;
    if let Some((when, output)) = SEEN.lock().ok()?.get_or_insert_with(HashMap::new).get(exe)
        && *when == modified
    {
        return output.clone();
    }
    let output = run(exe, args);
    SEEN.lock().ok()?.get_or_insert_with(HashMap::new).insert(exe.to_path_buf(), (modified, output.clone()));
    output
}

/// Runs `exe args` without a window and gives up after a few seconds.
fn run(exe: &Path, args: &[&str]) -> Option<String> {
    let mut child = crate::platform::command(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let start = Instant::now();
    while child.try_wait().ok()?.is_none() {
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().ok()?;
    // Python 2 printed its version to stderr.
    let text =
        String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr);
    Some(text)
}

/// The first dotted version in a program's first line of output:
/// `v24.13.0`, `Python 3.14.3`, `go version go1.23.4 windows/amd64`,
/// `PHP 8.3.33 (cli)`, `mysqld  Ver 10.11.6-MariaDB for Win64`,
/// `postgres (PostgreSQL) 17.2`.
fn parse_version(output: &str) -> Option<Version> {
    let line = output.lines().find(|l| !l.trim().is_empty())?;
    line.split_whitespace().find_map(|token| {
        let token = token.trim_start_matches("go").trim_start_matches('v');
        let end = token.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(token.len());
        let number = token[..end].trim_end_matches('.');
        if !number.contains('.') {
            return None;
        }
        number.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Option<Version> {
        Some(s.parse().unwrap())
    }

    #[test]
    fn reads_each_programs_version() {
        assert_eq!(parse_version("v24.13.0\n"), v("24.13.0"));
        assert_eq!(parse_version("Python 3.14.3\r\n"), v("3.14.3"));
        assert_eq!(parse_version("go version go1.23.4 windows/amd64"), v("1.23.4"));
        assert_eq!(parse_version("PHP 8.3.33 (cli) (built: Jul 28 2026)\nCopyright"), v("8.3.33"));
        assert_eq!(parse_version("C:\\mysql\\bin\\mysqld.exe  Ver 8.0.46 for Win64"), v("8.0.46"));
        assert_eq!(parse_version("mysqld  Ver 10.11.6-MariaDB for Win64 (AMD64)"), v("10.11.6"));
        assert_eq!(parse_version("postgres (PostgreSQL) 17.2"), v("17.2"));
        assert_eq!(parse_version("not a version"), None);
    }
}
