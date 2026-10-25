// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! A terminal whose `PATH` starts with the active versions, so `php`,
//! `composer`, `mysql`, `node`, `python`, and `go` are MeowServer's (PRD F29).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{Os, os};
use crate::service::{self, Kind};

/// Folders to put first on `PATH`, most important first.
pub fn bin_dirs(layout: &Layout, config: &Config) -> Result<Vec<PathBuf>> {
    let mut dirs = vec![shims(layout, config)?];
    for kind in [
        Kind::Php,
        Kind::Node,
        Kind::Python,
        Kind::Go,
        Kind::Mysql,
        Kind::Mariadb,
        Kind::Postgres,
        Kind::Redis,
        Kind::Caddy,
        Kind::Nginx,
    ] {
        let Some(installed) = service::select(layout, kind, config.version_selector(kind.id())) else {
            continue;
        };
        if matches!(kind, Kind::Python | Kind::Go) {
            dirs.extend(crate::app::bin_dirs(kind, &installed));
            continue;
        }
        let bin = installed.dir.join("bin");
        // Node on macOS/Linux and the databases keep executables in bin/.
        dirs.push(if bin.is_dir() { bin } else { installed.dir });
    }
    Ok(dirs)
}

pub fn path_var(layout: &Layout, config: &Config) -> Result<OsString> {
    let mut dirs = bin_dirs(layout, config)?;
    if let Some(existing) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&existing));
    }
    Ok(std::env::join_paths(dirs)?)
}

/// Environment for anything run "inside" MeowServer: `PATH` first, then the
/// active PHP's generated `php.ini` so the CLI matches the web pool.
pub fn env(layout: &Layout, config: &Config) -> Result<Vec<(OsString, OsString)>> {
    let mut vars = vec![
        (OsString::from("PATH"), path_var(layout, config)?),
        (OsString::from("MEOWSERVER"), layout.root().as_os_str().to_owned()),
    ];
    if let Some(php) = service::select(layout, Kind::Php, config.version_selector("php")) {
        vars.push(("PHPRC".into(), crate::service::php::write_ini(layout, config, &php)?.into()));
        vars.push(("PHP_INI_SCAN_DIR".into(), crate::service::php::scan_dirs(layout, &php)?));
    }
    Ok(vars)
}

/// Small launchers for tools that are not executables themselves:
/// `composer` runs `composer.phar` with the active PHP.
fn shims(layout: &Layout, config: &Config) -> Result<PathBuf> {
    let dir = layout.tmp().join("shims");
    std::fs::create_dir_all(&dir)?;
    if let Some(composer) = service::select(layout, Kind::Composer, config.version_selector("composer")) {
        let phar = composer.dir.join("composer.phar");
        if os() == Os::Windows {
            std::fs::write(dir.join("composer.bat"), format!("@php \"{}\" %*\r\n", phar.display()))?;
        } else {
            let script = dir.join("composer");
            std::fs::write(&script, format!("#!/bin/sh\nexec php \"{}\" \"$@\"\n", phar.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
            }
        }
    }
    Ok(dir)
}

/// The user's shell, ready to run in the current console (`meow shell`).
pub fn shell(layout: &Layout, config: &Config) -> Result<Command> {
    let program = match os() {
        Os::Windows => "powershell.exe".to_owned(),
        _ => std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
    };
    let mut cmd = Command::new(program);
    if os() == Os::Windows {
        cmd.arg("-NoLogo");
    }
    cmd.envs(env(layout, config)?);
    Ok(cmd)
}

/// Opens a new terminal window in `cwd` (default: `www/`).
pub fn open(layout: &Layout, config: &Config, cwd: Option<&Path>) -> Result<()> {
    let cwd = cwd.map(Path::to_path_buf).unwrap_or_else(|| layout.resolve(&config.general.www));
    let vars = env(layout, config)?;
    let path = path_var(layout, config)?;
    match os() {
        Os::Windows => {
            let mut cmd = Command::new("powershell.exe");
            cmd.args(["-NoLogo", "-NoExit"]).current_dir(&cwd).envs(vars);
            crate::platform::spawn_console(&mut cmd).context("cannot open a terminal")?;
        }
        Os::MacOs => {
            // Terminal.app does not inherit our environment; run a script.
            let script = layout.tmp().join("meow-terminal.command");
            let body = format!(
                "#!/bin/sh\ncd \"{}\"\nexport PATH=\"{}\"\nexec \"${{SHELL:-/bin/zsh}}\" -l\n",
                cwd.display(),
                path.to_string_lossy()
            );
            std::fs::write(&script, body)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
            }
            crate::platform::open(script.as_os_str())?;
        }
        Os::Linux => {
            let mut cmd = Command::new("x-terminal-emulator");
            cmd.current_dir(&cwd).envs(vars);
            crate::platform::spawn_detached(&mut cmd)
                .context("cannot open a terminal (x-terminal-emulator)")?;
        }
    }
    Ok(())
}
