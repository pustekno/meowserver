// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! The only place for OS-specific code (PRD §11).

use std::process::{Child, Command};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

/// A spawned service together with every process it starts. Dropping it
/// kills the whole tree, so a crashed or killed `meowd` never leaves orphans
/// (PRD §8.2).
pub struct ProcessTree(imp::ProcessTree);

impl ProcessTree {
    /// Forcefully ends the process and all its descendants.
    pub fn kill(&self) {
        self.0.kill();
    }
}

/// Spawns a service process without a console window, bound to a
/// [`ProcessTree`].
pub fn spawn_service(cmd: &mut Command) -> std::io::Result<(Child, ProcessTree)> {
    let (child, tree) = imp::spawn_service(cmd)?;
    Ok((child, ProcessTree(tree)))
}

/// Spawns a process that must outlive its parent, such as `meowd` started
/// by the CLI, or a terminal opened from the tray.
pub fn spawn_detached(cmd: &mut Command) -> std::io::Result<Child> {
    imp::spawn_detached(cmd)
}

/// Opens a folder or URL with the desktop's default handler.
pub fn open(target: &std::ffi::OsStr) -> std::io::Result<()> {
    let program = match os() {
        Os::Windows => "explorer",
        Os::MacOs => "open",
        Os::Linux => "xdg-open",
    };
    let mut cmd = Command::new(program);
    cmd.arg(target);
    spawn_detached(&mut cmd).map(drop)
}

pub const EXE_SUFFIX: &str = std::env::consts::EXE_SUFFIX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

pub fn os() -> Os {
    if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else {
        Os::Linux
    }
}

pub fn arch() -> Arch {
    if cfg!(target_arch = "aarch64") { Arch::Arm64 } else { Arch::X64 }
}
