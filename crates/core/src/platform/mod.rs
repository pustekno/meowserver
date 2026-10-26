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

/// Spawns a console program in a new console window (Windows), or
/// detached elsewhere.
pub fn spawn_console(cmd: &mut Command) -> std::io::Result<Child> {
    imp::spawn_console(cmd)
}

/// Every process listening on a TCP port, as (pid, executable). All
/// addresses count: on Windows a program bound to 127.0.0.1 silently wins
/// over one bound to 0.0.0.0 on the same port, so one owner is not enough.
pub fn port_owners(port: u16) -> Vec<(u32, String)> {
    imp::listener_pids(port)
        .into_iter()
        .map(|pid| (pid, imp::process_path(pid).unwrap_or_else(|| format!("pid {pid}"))))
        .collect()
}

/// True if a process with this executable file name is running
/// (case-insensitive, e.g. `laragon.exe`).
pub fn is_running(exe_name: &str) -> bool {
    imp::process_names().iter().any(|(_, n)| n.eq_ignore_ascii_case(exe_name))
}

/// True when `path` is `root` or inside it, ignoring case. Compares whole
/// folder names: `C:\MeowServer-0.1.0\caddy.exe` is not inside
/// `C:\MeowServer`, although the text starts the same.
pub fn is_inside(path: &str, root: &std::path::Path) -> bool {
    let path = path.to_lowercase().replace('/', "\\");
    let root = root.to_string_lossy().to_lowercase().replace('/', "\\");
    let root = root.trim_end_matches('\\');
    path == root || path.starts_with(&format!("{root}\\"))
}

/// True if a process named one of `stems` (without `.exe`) runs from inside
/// `root`. Where the full path is unknown, any such process counts, so
/// callers err on the side of "in use".
pub fn processes_under(root: &std::path::Path, stems: &[&str]) -> bool {
    imp::process_names().into_iter().any(|(pid, name)| {
        let lower = name.to_ascii_lowercase();
        let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
        if !stems.contains(&stem) {
            return false;
        }
        match imp::process_path(pid) {
            Some(path) if std::path::Path::new(&path).is_absolute() => is_inside(&path, root),
            _ => true,
        }
    })
}

/// Processes named one of `stems` (without `.exe`) whose executable is
/// known to be inside `root`, as (pid, path). Safe to act on, unlike
/// [`processes_under`], which also counts processes it cannot place.
pub fn processes_in(root: &std::path::Path, stems: &[&str]) -> Vec<(u32, String)> {
    imp::process_names()
        .into_iter()
        .filter(|(_, name)| {
            let lower = name.to_ascii_lowercase();
            stems.contains(&lower.strip_suffix(".exe").unwrap_or(&lower))
        })
        .filter_map(|(pid, _)| Some((pid, imp::process_path(pid)?)))
        .filter(|(_, path)| std::path::Path::new(path).is_absolute() && is_inside(path, root))
        .collect()
}

/// Processes named one of `stems` (without `.exe`), as (pid, executable
/// path, or the name when the path is unknown).
pub fn processes_named(stems: &[&str]) -> Vec<(u32, String)> {
    imp::process_names()
        .into_iter()
        .filter(|(_, name)| {
            let lower = name.to_ascii_lowercase();
            stems.contains(&lower.strip_suffix(".exe").unwrap_or(&lower))
        })
        .map(|(pid, name)| (pid, imp::process_path(pid).unwrap_or(name)))
        .collect()
}

/// How a process was started, where the OS tells (macOS and Linux).
pub fn command_line(pid: u32) -> Option<String> {
    imp::command_line(pid)
}

/// Ends a process at once (Windows) or asks it to end (`SIGTERM`).
pub fn terminate(pid: u32) {
    imp::terminate(pid);
}

/// Whether `pid` refuses to follow links made without administrator rights
/// (Windows' RedirectionGuard, inherited from setup programs).
pub fn redirection_guarded(pid: u32) -> bool {
    imp::redirection_guarded(pid)
}

pub fn is_alive(pid: u32) -> bool {
    imp::process_names().iter().any(|(p, _)| *p == pid)
}

/// Ends every process started from exactly `exe`, such as the window of one
/// MeowServer folder; copies in other folders are left alone. Returns how
/// many were ended.
pub fn end_program(exe: &std::path::Path) -> usize {
    let Some(name) = exe.file_name().map(|n| n.to_string_lossy().to_lowercase()) else { return 0 };
    let wanted = exe.to_string_lossy().to_lowercase();
    let mut ended = 0;
    for (pid, process) in imp::process_names() {
        if process.to_lowercase() == name
            && imp::process_path(pid).is_some_and(|p| p.to_lowercase() == wanted)
            && pid != std::process::id()
        {
            imp::terminate(pid);
            ended += 1;
        }
    }
    ended
}

/// Runs a command with administrator rights after the OS prompt (UAC or a
/// password dialog) and returns its exit code. Fails if the user declines.
pub fn run_elevated(program: &std::path::Path, args: &[String]) -> std::io::Result<i32> {
    imp::run_elevated(program, args)
}

/// Opens a folder or URL with the desktop's default handler. On Windows
/// through the shell: `explorer <url>` opens a folder window instead when
/// the URL has a query (`?server=1`).
pub fn open(target: &std::ffi::OsStr) -> std::io::Result<()> {
    #[cfg(windows)]
    return imp::shell_open(target);
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new(if os() == Os::MacOs { "open" } else { "xdg-open" });
        cmd.arg(target);
        spawn_detached(&mut cmd).map(drop)
    }
}

/// A command for a helper program that shows no console window. The
/// MeowServer window and `meowd` have no console, so on Windows every
/// console program they start (mysqladmin, PowerShell, php) would
/// otherwise flash one.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    cmd
}

/// The user's language as a two-letter code (`en`, `id`, …).
pub fn language() -> String {
    imp::locale().split(['-', '_', '.']).next().unwrap_or("en").to_ascii_lowercase()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_compares_whole_folder_names() {
        let root = std::path::Path::new(r"C:\MeowServer");
        assert!(is_inside(r"C:\MeowServer\bin\caddy\caddy.exe", root));
        assert!(is_inside(r"c:\meowserver\meowd.exe", root), "case does not matter");
        assert!(is_inside(r"C:\MeowServer", root));
        assert!(!is_inside(r"C:\MeowServer-0.1.0\bin\caddy\caddy.exe", root), "a sibling folder");
        assert!(!is_inside(r"C:\MeowServer2\meowd.exe", root));
        assert!(is_inside(r"C:\MeowServer\x", std::path::Path::new(r"C:\MeowServer\")));
    }
}
