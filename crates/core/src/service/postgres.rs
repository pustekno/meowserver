// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! PostgreSQL (PRD S4). User `postgres` with trust authentication, loopback
//! only (PRD §9.10).

use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::{Installed, Kind, datadir};
use crate::config::Config;
use crate::layout::Layout;
use crate::platform::EXE_SUFFIX;
use crate::supervisor::{Invocation, ProcessSpec, Restart};

pub const DEFAULT_PORT: u16 = 5432;

pub fn plan(layout: &Layout, config: &Config) -> Result<ProcessSpec> {
    let Some(installed) = super::select(layout, Kind::Postgres, config.version_selector("postgres")) else {
        bail!("PostgreSQL is not installed. Run `meow install postgres`.");
    };
    let data = datadir::path(layout, config, Kind::Postgres, &installed.version);
    if datadir::check(&data, Kind::Postgres, &installed.version)? {
        initialize(layout, &installed, &data)?;
    }
    let bin = |name: &str| installed.dir.join("bin").join(format!("{name}{EXE_SUFFIX}"));
    let port = config.port("postgres", DEFAULT_PORT).to_string();
    Ok(ProcessSpec {
        id: "postgres".into(),
        program: bin("postgres"),
        args: vec![
            "-D".into(),
            data.clone().into(),
            "-p".into(),
            port.into(),
            "-c".into(),
            "listen_addresses=127.0.0.1,::1".into(),
        ],
        env: Vec::new(),
        cwd: Some(installed.dir.clone()),
        log: layout.logs().join("postgres.log"),
        restart: Restart::OnFailure,
        graceful_stop: Some(Invocation {
            program: bin("pg_ctl"),
            args: vec!["stop".into(), "-D".into(), data.into(), "-m".into(), "fast".into(), "-w".into()],
            timeout: Duration::from_secs(30),
        }),
    })
}

fn initialize(layout: &Layout, installed: &Installed, data: &Path) -> Result<()> {
    std::fs::create_dir_all(data)?;
    let log_path = layout.logs().join("postgres-init.log");
    let log = crate::logging::open_rotated(&log_path)?;
    log::info!("initializing PostgreSQL data in {}", data.display());
    let mut cmd = crate::platform::command(installed.dir.join("bin").join(format!("initdb{EXE_SUFFIX}")));
    cmd.args(["-U", "postgres", "-A", "trust", "-E", "UTF8", "--locale=C", "-D"])
        .arg(data)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    let (mut child, _tree) = crate::platform::spawn_service(&mut cmd).context("cannot run initdb")?;
    let status = child.wait()?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(data);
        bail!("initdb failed ({status}); see {}", log_path.display());
    }
    // User overrides live outside the data folder (PRD §9.9).
    let custom = layout.custom().join("postgres").join("postgresql.conf");
    std::fs::create_dir_all(custom.parent().unwrap())?;
    let mut conf = std::fs::OpenOptions::new().append(true).open(data.join("postgresql.conf"))?;
    writeln!(
        conf,
        "\n# Added by MeowServer\ninclude_if_exists '{}'",
        custom.display().to_string().replace('\\', "/")
    )?;
    datadir::write_marker(data, Kind::Postgres, &installed.version)
}
