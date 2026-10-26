// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! MySQL and MariaDB (PRD S3, F23, F24). Same shape, different tools.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::{Installed, Kind, datadir, write_generated};
use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{EXE_SUFFIX, Os, os};
use crate::supervisor::{Invocation, ProcessSpec, Restart};
use crate::template;
use crate::version::Version;

pub const DEFAULT_PORT: u16 = 3306;

#[derive(Serialize)]
struct IniContext {
    product: &'static str,
    version: String,
    basedir: PathBuf,
    datadir: PathBuf,
    port: u16,
    bind: &'static str,
    mysqlx_off: bool,
    error_log: PathBuf,
    pid: PathBuf,
    collation: &'static str,
    custom_dir: PathBuf,
}

/// One server of a MySQL or MariaDB unit: the default version, or one of
/// `[services.<kind>.extra]`.
pub struct Instance {
    pub installed: Installed,
    pub port: u16,
    pub data: PathBuf,
    /// Supervisor id: `mysql` for the default, `mysql-5.7` for the others.
    pub id: String,
}

/// Every server of `kind` that should run, default first. A missing extra
/// version becomes a warning, so it never stops the default one.
pub fn instances(layout: &Layout, config: &Config, kind: Kind) -> Result<(Vec<Instance>, Vec<String>)> {
    let name = product(kind);
    let Some(installed) = super::select(layout, kind, config.version_selector(kind.id())) else {
        bail!("{name} is not installed. Run `meow install {}`.", kind.id());
    };
    let default_line = datadir::line(kind, &installed.version);
    let mut out = vec![Instance {
        port: config.port(kind.id(), DEFAULT_PORT),
        data: datadir::path(layout, config, kind, &installed.version),
        id: kind.id().into(),
        installed,
    }];
    let mut warnings = Vec::new();
    let extra = config.services.get(kind.id()).map(|s| s.extra.clone()).unwrap_or_default();
    for (line, port) in extra {
        if line == default_line {
            continue;
        }
        match super::select(layout, kind, Some(&line)) {
            Some(installed) => out.push(Instance {
                port,
                data: layout.data().join(format!("{}-{line}", kind.id())),
                id: format!("{}-{line}", kind.id()),
                installed,
            }),
            None => warnings.push(format!(
                "{name} {line} is set to run on port {port} but is not installed: meow install {} {line}",
                kind.id()
            )),
        }
    }
    Ok((out, warnings))
}

fn product(kind: Kind) -> &'static str {
    if kind == Kind::Mysql { "MySQL" } else { "MariaDB" }
}

/// The servers of a MySQL or MariaDB unit, plus warnings.
pub fn plan(layout: &Layout, config: &Config, kind: Kind) -> Result<(Vec<ProcessSpec>, Vec<String>)> {
    debug_assert!(matches!(kind, Kind::Mysql | Kind::Mariadb));
    let (instances, warnings) = instances(layout, config, kind)?;
    let specs = instances.iter().map(|i| plan_one(layout, kind, i)).collect::<Result<_>>()?;
    Ok((specs, warnings))
}

fn plan_one(layout: &Layout, kind: Kind, instance: &Instance) -> Result<ProcessSpec> {
    let name = product(kind);
    let installed = &instance.installed;
    let version = &installed.version;
    let line = datadir::line(kind, version);
    let port = instance.port;
    let data = instance.data.clone();
    let needs_init = datadir::check(&data, kind, version)?;

    let custom = layout.custom().join(kind.id());
    std::fs::create_dir_all(&custom)?;
    let ctx = IniContext {
        product: name,
        version: version.to_string(),
        basedir: installed.dir.clone(),
        datadir: data.clone(),
        port,
        bind: bind_address(kind, version),
        mysqlx_off: kind == Kind::Mysql && version.major() >= 8,
        error_log: layout.logs().join(format!("{}-{line}.err.log", kind.id())),
        pid: layout.tmp().join(format!("{}-{line}.pid", kind.id())),
        collation: if kind == Kind::Mysql && version.major() >= 8 {
            "utf8mb4_0900_ai_ci"
        } else {
            "utf8mb4_unicode_ci"
        },
        custom_dir: custom,
    };
    let ini = layout.generated().join(kind.id()).join(&line).join("my.ini");
    write_generated(&ini, &template::render("mysql/my.ini", ctx)?)?;

    if needs_init {
        initialize(layout, kind, installed, &ini, &data)?;
    }

    let defaults = format!("--defaults-file={}", ini.display());
    let server = server_exe(kind, installed);
    let admin = tool(installed, if kind == Kind::Mysql { "mysqladmin" } else { "mariadb-admin" });
    Ok(ProcessSpec {
        id: instance.id.clone(),
        program: server,
        args: vec![defaults.into()],
        env: Vec::new(),
        cwd: Some(installed.dir.clone()),
        log: layout.logs().join(format!("{}.log", instance.id)),
        restart: Restart::OnFailure,
        graceful_stop: Some(Invocation {
            program: admin,
            args: vec![
                "-u".into(),
                "root".into(),
                "-h".into(),
                "127.0.0.1".into(),
                "-P".into(),
                port.to_string().into(),
                "shutdown".into(),
            ],
            timeout: Duration::from_secs(30),
        }),
    })
}

/// After `meow db upgrade` moves MariaDB data to a newer version, the
/// system tables need `mariadb-upgrade` (MySQL 8.0.16+ does this itself).
/// Runs in the background once the server answers, only when the upgrade
/// left its flag in the data folder.
pub fn after_start(layout: &Layout, config: &Config, kind: Kind) {
    if kind != Kind::Mariadb {
        return;
    }
    let Ok((instances, _)) = instances(layout, config, kind) else { return };
    for instance in instances {
        upgrade_when_flagged(instance);
    }
}

fn upgrade_when_flagged(instance: Instance) {
    let flag = instance.data.join(datadir::PENDING_UPGRADE);
    if !flag.exists() {
        return;
    }
    let port = instance.port.to_string();
    let upgrade = tool(&instance.installed, "mariadb-upgrade");
    let client = tool(&instance.installed, "mariadb-admin");
    std::thread::spawn(move || {
        let quiet = |cmd: &mut Command| {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        for _ in 0..60 {
            if quiet(crate::platform::command(&client).args([
                "-u",
                "root",
                "-h",
                "127.0.0.1",
                "-P",
                &port,
                "ping",
            ])) {
                if quiet(crate::platform::command(&upgrade).args([
                    "-u",
                    "root",
                    "-h",
                    "127.0.0.1",
                    "-P",
                    &port,
                ])) {
                    let _ = std::fs::remove_file(&flag);
                    log::info!("mariadb-upgrade finished");
                } else {
                    log::warn!("mariadb-upgrade failed; run it by hand: {}", upgrade.display());
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

/// Both servers accept a list from these versions on; older ones take one
/// address.
fn bind_address(kind: Kind, version: &Version) -> &'static str {
    let multi: Version =
        if kind == Kind::Mysql { "8.0.13".parse().unwrap() } else { "10.11".parse().unwrap() };
    if *version >= multi { "127.0.0.1,::1" } else { "127.0.0.1" }
}

fn tool(installed: &Installed, name: &str) -> PathBuf {
    installed.dir.join("bin").join(format!("{name}{EXE_SUFFIX}"))
}

fn server_exe(kind: Kind, installed: &Installed) -> PathBuf {
    let mariadbd = tool(installed, "mariadbd");
    if kind == Kind::Mariadb && mariadbd.exists() { mariadbd } else { tool(installed, "mysqld") }
}

/// Creates the system tables with `root` and no password, reachable from
/// loopback only (PRD F23, §9.10).
fn initialize(layout: &Layout, kind: Kind, installed: &Installed, ini: &Path, data: &Path) -> Result<()> {
    std::fs::create_dir_all(data)?;
    let log_path = layout.logs().join(format!("{}-init.log", kind.id()));
    let log = crate::logging::open_rotated(&log_path)?;
    log::info!("initializing {} data in {}", kind.id(), data.display());

    let mut cmd = match (kind, os()) {
        (Kind::Mysql, _) => {
            let mut c = crate::platform::command(tool(installed, "mysqld"));
            c.arg(format!("--defaults-file={}", ini.display())).arg("--initialize-insecure");
            c
        }
        (_, Os::Windows) => {
            let mut c = crate::platform::command(tool(installed, "mariadb-install-db"));
            c.arg(format!("--datadir={}", data.display()));
            c
        }
        _ => {
            let mut c = crate::platform::command(installed.dir.join("scripts").join("mariadb-install-db"));
            c.arg("--no-defaults")
                .arg(format!("--basedir={}", installed.dir.display()))
                .arg(format!("--datadir={}", data.display()))
                .arg("--auth-root-authentication-method=normal")
                .arg("--skip-test-db");
            c
        }
    };
    cmd.current_dir(&installed.dir).stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
    let (mut child, _tree) = crate::platform::spawn_service(&mut cmd)
        .with_context(|| format!("cannot run the {} initializer", kind.id()))?;
    let status = child.wait()?;
    if !status.success() {
        // Leave nothing half-made behind, so the next start tries again.
        let _ = std::fs::remove_dir_all(data);
        bail!("initializing {} data failed ({status}); see {}", kind.id(), log_path.display());
    }
    // mariadb-install-db on Windows writes its own my.ini, which we do not use.
    let _ = std::fs::remove_file(data.join("my.ini"));
    datadir::write_marker(data, kind, &installed.version)
}
