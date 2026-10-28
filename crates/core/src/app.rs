// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Projects that run their own development server (PRD F43): Next, Nuxt,
//! Vite, and other Node apps; Django, Flask, and FastAPI; Go. MeowServer
//! starts the server when asked (never on its own: they use a lot of
//! memory), with the project's runtime version first on `PATH` and a port
//! of its own, and the site's name forwards to that port.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{EXE_SUFFIX, Os, os};
use crate::service::{self, Installed, Kind};
use crate::site::needs::runtime_name;
use crate::site::{Framework, Site};
use crate::supervisor::{ProcessSpec, Restart};

/// Supervisor id of a site's development server.
pub fn process_id(site: &str) -> String {
    format!("app-{site}")
}

pub fn log_file(layout: &Layout, site: &str) -> PathBuf {
    layout.logs().join(format!("{}.log", process_id(site)))
}

/// How to start `site`'s development server.
pub fn plan(layout: &Layout, config: &Config, site: &Site) -> Result<ProcessSpec> {
    let Some(kind) = site.framework.runtime() else {
        bail!("{} is not a project with its own server", site.name);
    };
    let Some(line) = &site.runtime else {
        let wanted = site.needs.runtime.as_ref().map(|r| r.suggest()).unwrap_or_default();
        bail!("no installed {} fits {}: meow install {} {wanted}", runtime_name(kind), site.name, kind.id());
    };
    let installed = service::select_runtime(layout, kind, line)
        .with_context(|| format!("{} {line} is not installed", runtime_name(kind)))?;
    let port = site.app_port.context("no port for this site")?.to_string();

    // The project's runtime first on PATH, then everything MeowServer has.
    let mut dirs = bin_dirs(kind, &installed);
    if let Ok(path) = crate::terminal::path_var(layout, config) {
        dirs.extend(std::env::split_paths(&path));
    }
    let mut env: Vec<(OsString, OsString)> = vec![
        ("PATH".into(), std::env::join_paths(dirs)?),
        ("PORT".into(), port.clone().into()),
        ("MEOWSERVER".into(), layout.root().as_os_str().to_owned()),
    ];

    let dir = &site.path;
    let (program, args): (PathBuf, Vec<String>) = match kind {
        Kind::Node => node_command(&installed, dir, site.framework, &port)?,
        Kind::Python => {
            env.push(("PYTHONUNBUFFERED".into(), "1".into()));
            python_command(&installed, dir, site.framework, &port)
        }
        Kind::Go => {
            (installed.dir.join("bin").join(format!("go{EXE_SUFFIX}")), vec!["run".into(), ".".into()])
        }
        _ => unreachable!("runtime() only names Node, Python, and Go"),
    };
    Ok(ProcessSpec {
        id: process_id(&site.name),
        program,
        args: args.into_iter().map(OsString::from).collect(),
        env,
        cwd: Some(dir.clone()),
        log: log_file(layout, &site.name),
        restart: Restart::Never,
        graceful_stop: None,
    })
}

/// The folders holding a runtime's programs.
pub fn bin_dirs(kind: Kind, installed: &Installed) -> Vec<PathBuf> {
    let dir = &installed.dir;
    match (kind, os()) {
        // Python on Windows: python.exe at the top, pip and friends in Scripts.
        (Kind::Python, Os::Windows) => vec![dir.clone(), dir.join("Scripts")],
        // Node on Windows keeps node.exe and npm.cmd at the top.
        (Kind::Node, Os::Windows) => vec![dir.clone()],
        _ => vec![dir.join("bin")],
    }
}

/// `npm run dev` (or `start`), with the port passed the way each tool
/// takes it; pnpm and yarn projects through Corepack when Node has it.
fn node_command(
    installed: &Installed,
    dir: &Path,
    framework: Framework,
    port: &str,
) -> Result<(PathBuf, Vec<String>)> {
    let package: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("package.json"))?)?;
    let script = if package.pointer("/scripts/dev").is_some() { "dev" } else { "start" };
    let tool = |name: &str| {
        let bin = bin_dirs(Kind::Node, installed).remove(0);
        if os() == Os::Windows { bin.join(format!("{name}.cmd")) } else { bin.join(name) }
    };
    let corepack = tool("corepack");
    let manager = if dir.join("pnpm-lock.yaml").is_file() && corepack.is_file() {
        Some("pnpm")
    } else if dir.join("yarn.lock").is_file() && corepack.is_file() {
        Some("yarn")
    } else {
        None
    };
    let flags: Vec<String> = match framework {
        Framework::Vite => {
            vec!["--port".into(), port.into(), "--strictPort".into(), "--host".into(), "127.0.0.1".into()]
        }
        Framework::Next => vec!["-p".into(), port.into(), "-H".into(), "127.0.0.1".into()],
        Framework::Nuxt => vec!["--port".into(), port.into(), "--host".into(), "127.0.0.1".into()],
        // Other Node servers read PORT.
        _ => Vec::new(),
    };
    Ok(match manager {
        Some(manager) => {
            let mut args = vec![manager.to_owned(), "run".into(), script.into()];
            args.extend(flags);
            (corepack, args)
        }
        None => {
            let mut args = vec!["run".into(), script.into()];
            if !flags.is_empty() {
                args.push("--".into());
                args.extend(flags);
            }
            (tool("npm"), args)
        }
    })
}

/// The project's own virtual environment when it has one (where its
/// packages are), else MeowServer's Python.
fn python_command(
    installed: &Installed,
    dir: &Path,
    framework: Framework,
    port: &str,
) -> (PathBuf, Vec<String>) {
    let venv_python = [".venv", "venv"].iter().find_map(|v| {
        let python = if os() == Os::Windows {
            dir.join(v).join("Scripts").join("python.exe")
        } else {
            dir.join(v).join("bin").join("python")
        };
        python.is_file().then_some(python)
    });
    let python = venv_python.unwrap_or_else(|| {
        if os() == Os::Windows {
            installed.dir.join("python.exe")
        } else {
            installed.dir.join("bin").join("python3")
        }
    });
    let host = "127.0.0.1".to_owned();
    let args = match framework {
        Framework::Django => vec!["manage.py".into(), "runserver".into(), format!("{host}:{port}")],
        Framework::FastApi => vec![
            "-m".into(),
            "uvicorn".into(),
            format!("{}:app", fastapi_module(dir)),
            "--host".into(),
            host,
            "--port".into(),
            port.into(),
            "--reload".into(),
        ],
        _ => vec![
            "-m".into(),
            "flask".into(),
            "run".into(),
            "--host".into(),
            host,
            "--port".into(),
            port.into(),
            "--debug".into(),
        ],
    };
    (python, args)
}

/// Where a FastAPI app usually lives: `main.py`, `app/main.py`, `app.py`.
fn fastapi_module(dir: &Path) -> &'static str {
    [("main.py", "main"), ("app/main.py", "app.main"), ("app.py", "app"), ("src/main.py", "src.main")]
        .iter()
        .find(|(file, _)| dir.join(file).is_file())
        .map_or("main", |(_, module)| module)
}
