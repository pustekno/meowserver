// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Database chores: backup and restore with the servers' own tools (PRD
//! F26), moving data to a newer version with a backup first (F25), and
//! opening a database GUI (F28).

use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{EXE_SUFFIX, Os, os};
use crate::service::{self, Installed, Kind, datadir, mysql, postgres};

fn active(layout: &Layout, config: &Config, kind: Kind) -> Result<Installed> {
    if !matches!(kind, Kind::Mysql | Kind::Mariadb | Kind::Postgres) {
        bail!("{} is not a database server", kind.id());
    }
    service::select(layout, kind, config.version_selector(kind.id()))
        .with_context(|| format!("{} is not installed", kind.id()))
}

fn tool(installed: &Installed, names: &[&str]) -> Result<PathBuf> {
    names
        .iter()
        .map(|n| installed.dir.join("bin").join(format!("{n}{EXE_SUFFIX}")))
        .find(|p| p.exists())
        .with_context(|| format!("{} not found in {}", names[0], installed.dir.display()))
}

fn port(config: &Config, kind: Kind) -> u16 {
    match kind {
        Kind::Postgres => config.port("postgres", postgres::DEFAULT_PORT),
        other => config.port(other.id(), mysql::DEFAULT_PORT),
    }
}

/// Runs an installed MySQL or MariaDB line next to the default one, on
/// `port` or the next free port after the default's (PRD F24). Takes
/// effect on the next start. Returns the port.
pub fn enable_version(layout: &Layout, kind: Kind, line: &str, port: Option<u16>) -> Result<u16> {
    if !matches!(kind, Kind::Mysql | Kind::Mariadb) {
        bail!("only MySQL and MariaDB can run several versions at once");
    }
    let mut config = Config::load(layout)?;
    let default = active(layout, &config, kind)?;
    let installed = service::select(layout, kind, Some(line)).with_context(|| {
        format!("{} {line} is not installed: meow install {} {line}", kind.id(), kind.id())
    })?;
    let line = datadir::line(kind, &installed.version);
    if line == datadir::line(kind, &default.version) {
        bail!("{} {line} is already the default version", kind.id());
    }
    let port = match port {
        Some(p) => p,
        None => crate::engine::free_port(&config, config.port(kind.id(), mysql::DEFAULT_PORT) + 1)
            .context("no free port found")?,
    };
    config.services.entry(kind.id().into()).or_default().extra.insert(line, port);
    config.save(layout)?;
    Ok(port)
}

/// Stops running `line` next to the default version. Its data stays.
pub fn disable_version(layout: &Layout, kind: Kind, line: &str) -> Result<()> {
    let mut config = Config::load(layout)?;
    let removed = config.services.get_mut(kind.id()).and_then(|s| s.extra.remove(line));
    if removed.is_none() {
        bail!("{} {line} is not set to run next to the default version", kind.id());
    }
    config.save(layout)
}

/// Dumps one database, or all of them, into `backups/` as gzip-compressed
/// SQL. The server must be running. Returns the file written.
pub fn backup(layout: &Layout, kind: Kind, database: Option<&str>) -> Result<PathBuf> {
    let config = Config::load(layout)?;
    let installed = active(layout, &config, kind)?;
    let port = port(&config, kind).to_string();
    let dir = layout.root().join("backups");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("{}-{}-{}.sql.gz", kind.id(), database.unwrap_or("all"), stamp()));

    let mut cmd = if kind == Kind::Postgres {
        let mut c = match database {
            Some(db) => {
                let mut c = Command::new(tool(&installed, &["pg_dump"])?);
                c.args(["--create", db]);
                c
            }
            None => Command::new(tool(&installed, &["pg_dumpall"])?),
        };
        c.args(["-h", "127.0.0.1", "-p", &port, "-U", "postgres"]);
        c
    } else {
        let mut c = Command::new(tool(&installed, &["mariadb-dump", "mysqldump"])?);
        c.args(["-u", "root", "-h", "127.0.0.1", "-P", &port]);
        c.args(["--single-transaction", "--routines", "--triggers", "--events", "--hex-blob"]);
        match database {
            Some(db) => c.args(["--databases", db]),
            None => c.arg("--all-databases"),
        };
        c
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("cannot start the dump tool")?;
    let mut out = GzEncoder::new(File::create(&file)?, Compression::default());
    std::io::copy(&mut child.stdout.take().unwrap(), &mut out)?;
    out.finish()?;
    let mut errors = String::new();
    child.stderr.take().unwrap().read_to_string(&mut errors)?;
    let status = child.wait()?;
    if !status.success() {
        let _ = std::fs::remove_file(&file);
        bail!("backup failed: {}", errors.trim());
    }
    Ok(file)
}

/// Loads a dump made by [`backup`] (or any `.sql` / `.sql.gz`). Dumps from
/// `backup` recreate their own databases.
pub fn restore(layout: &Layout, kind: Kind, file: &Path) -> Result<()> {
    let config = Config::load(layout)?;
    let installed = active(layout, &config, kind)?;
    let port = port(&config, kind).to_string();
    let mut cmd = if kind == Kind::Postgres {
        let mut c = Command::new(tool(&installed, &["psql"])?);
        c.args([
            "-h",
            "127.0.0.1",
            "-p",
            &port,
            "-U",
            "postgres",
            "-d",
            "postgres",
            "-q",
            "-v",
            "ON_ERROR_STOP=1",
        ]);
        c
    } else {
        let mut c = Command::new(tool(&installed, &["mariadb", "mysql"])?);
        c.args(["-u", "root", "-h", "127.0.0.1", "-P", &port]);
        c
    };
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("cannot start the database client")?;
    let mut input: Box<dyn Read> = if file.extension().is_some_and(|e| e == "gz") {
        Box::new(GzDecoder::new(BufReader::new(File::open(file)?)))
    } else {
        Box::new(BufReader::new(File::open(file)?))
    };
    let mut stdin = child.stdin.take().unwrap();
    std::io::copy(&mut input, &mut stdin)?;
    stdin.flush()?;
    drop(stdin);
    let mut errors = String::new();
    child.stderr.take().unwrap().read_to_string(&mut errors)?;
    if !child.wait()?.success() {
        bail!("restore failed: {}", errors.trim());
    }
    Ok(())
}

/// Moves `kind`'s data to version line `to` (PRD F25): copies the current
/// data folder to `backups/` first, then hands it to the new version. MySQL
/// upgrades its system tables on first start; MariaDB needs
/// `mariadb-upgrade` afterwards, which `meowd` runs when it starts it.
/// The server must be stopped. Returns the backup folder.
pub fn upgrade(layout: &Layout, kind: Kind, to: &str, progress: &mut dyn FnMut(u64)) -> Result<PathBuf> {
    let mut config = Config::load(layout)?;
    let current = active(layout, &config, kind)?;
    let target = service::select(layout, kind, Some(to)).with_context(|| {
        format!("{} {to} is not installed; run `meow install {} {to}`", kind.id(), kind.id())
    })?;
    if target.version <= current.version {
        bail!("{} {} is not newer than the active {}", kind.id(), target.version, current.version);
    }
    let from_dir = datadir::path(layout, &config, kind, &current.version);
    if !from_dir.is_dir() {
        bail!("no data at {}", from_dir.display());
    }
    let backup = layout.root().join("backups").join(format!(
        "{}-{}-data-{}",
        kind.id(),
        datadir::line(kind, &current.version),
        stamp()
    ));
    copy_dir(&from_dir, &backup, progress).context("backing up the data folder")?;

    // Data used in place (e.g. Laragon's) stays where it is; otherwise it
    // moves to the new version's folder.
    let in_place = config.services.get(kind.id()).is_some_and(|s| s.data_dir.is_some());
    let to_dir = if in_place {
        from_dir.clone()
    } else {
        let dest = layout.data().join(format!("{}-{}", kind.id(), datadir::line(kind, &target.version)));
        if dest.exists() {
            bail!("{} already exists; move it away first", dest.display());
        }
        std::fs::rename(&from_dir, &dest)?;
        dest
    };
    datadir::write_marker(&to_dir, kind, &target.version)?;
    if kind == Kind::Mariadb {
        std::fs::write(to_dir.join(datadir::PENDING_UPGRADE), target.version.to_string())?;
    }
    config.versions.insert(kind.id().into(), datadir::line(kind, &target.version));
    config.save(layout)?;
    Ok(backup)
}

/// Opens a database GUI connected to `kind` (PRD F28): the command in
/// `general.db_tool` if set (with `{host}`, `{port}`, `{user}`), else
/// HeidiSQL when it is installed, else the OS default for the tool.
pub fn open_tool(layout: &Layout, kind: Kind) -> Result<()> {
    let config = Config::load(layout)?;
    let port = port(&config, kind).to_string();
    let user = if kind == Kind::Postgres { "postgres" } else { "root" };
    if let Some(template) = &config.general.db_tool {
        let line = template.replace("{host}", "127.0.0.1").replace("{port}", &port).replace("{user}", user);
        let mut parts = line.split_whitespace();
        let program = parts.next().context("general.db_tool is empty")?;
        let mut cmd = Command::new(program);
        cmd.args(parts);
        crate::platform::spawn_detached(&mut cmd)?;
        return Ok(());
    }
    if os() == Os::Windows {
        if let Some(heidi) = heidisql() {
            // HeidiSQL: -n = network type (0 MySQL/MariaDB, 3 PostgreSQL).
            let net = if kind == Kind::Postgres { "3" } else { "0" };
            let mut cmd = Command::new(heidi);
            cmd.args([
                format!("-n={net}"),
                "-h=127.0.0.1".into(),
                format!("-P={port}"),
                format!("-u={user}"),
            ]);
            crate::platform::spawn_detached(&mut cmd)?;
            return Ok(());
        }
    }
    bail!("no database tool found; set general.db_tool in meow.toml, e.g. \"dbeaver\"")
}

/// A desktop tool is set up or HeidiSQL is installed (Laragon's counts).
pub fn tool_available(config: &Config) -> bool {
    config.general.db_tool.is_some() || (os() == Os::Windows && heidisql().is_some())
}

fn heidisql() -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from(r"C:\Program Files\HeidiSQL\heidisql.exe"),
        PathBuf::from(r"C:\laragon\bin\heidisql\heidisql.exe"),
    ];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join(r"Programs\HeidiSQL\heidisql.exe"));
    }
    candidates.into_iter().find(|p| p.exists())
}

fn copy_dir(from: &Path, to: &Path, progress: &mut dyn FnMut(u64)) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest, progress)?;
        } else {
            progress(std::fs::copy(entry.path(), &dest)?);
        }
    }
    Ok(())
}

/// `YYYYMMDD-HHMMSS` in UTC, for file names.
fn stamp() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}
