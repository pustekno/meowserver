// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! macOS and Linux: each service leads its own process group. On Linux the
//! kernel also signals the service if `meowd` dies (`PR_SET_PDEATHSIG`).
//! macOS has no equivalent; stale processes are cleaned up on the next start.

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

pub struct ProcessTree {
    pgid: libc::pid_t,
}

impl ProcessTree {
    pub fn kill(&self) {
        unsafe { libc::killpg(self.pgid, libc::SIGKILL) };
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.kill();
    }
}

pub fn spawn_service(cmd: &mut Command) -> io::Result<(Child, ProcessTree)> {
    cmd.process_group(0);
    #[cfg(target_os = "linux")]
    unsafe {
        cmd.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    let pgid = child.id() as libc::pid_t;
    Ok((child, ProcessTree { pgid }))
}

pub fn spawn_detached(cmd: &mut Command) -> io::Result<Child> {
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()
}

/// The user's locale from the environment, e.g. `id_ID.UTF-8`.
pub fn locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty() && s != "C" && s != "POSIX"))
        .unwrap_or_else(|| "en".into())
}

pub fn spawn_console(cmd: &mut Command) -> io::Result<Child> {
    spawn_detached(cmd)
}

/// Every process listening on a TCP port (needs `lsof`).
pub fn listener_pids(port: u16) -> Vec<u32> {
    let Ok(out) = Command::new("lsof").args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"]).output()
    else {
        return Vec::new();
    };
    let mut pids: Vec<u32> =
        String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().parse().ok()).collect();
    pids.dedup();
    pids
}

pub fn process_path(pid: u32) -> Option<String> {
    let out = Command::new("ps").args(["-p", &pid.to_string(), "-o", "comm="]).output().ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!name.is_empty()).then_some(name)
}

/// Windows only.
pub fn redirection_guarded(_pid: u32) -> bool {
    false
}

/// Asks a process to end (`SIGTERM`).
pub fn terminate(pid: u32) {
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
}

/// (pid, executable file name) of every running process.
pub fn process_names() -> Vec<(u32, String)> {
    let Ok(out) = Command::new("ps").args(["-A", "-o", "pid=,comm="]).output() else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let (pid, name) = l.trim().split_once(char::is_whitespace)?;
            let name = name.trim().rsplit('/').next()?.to_owned();
            Some((pid.parse().ok()?, name))
        })
        .collect()
}

/// Runs `program args` as root after the desktop's password prompt.
pub fn run_elevated(program: &std::path::Path, args: &[String]) -> io::Result<i32> {
    let status = if cfg!(target_os = "macos") {
        let quote = |s: &str| format!("'{}'", s.replace('\'', r#"'\''"#));
        let mut line = quote(&program.to_string_lossy());
        for a in args {
            line.push(' ');
            line.push_str(&quote(a));
        }
        let script =
            format!("do shell script \"{}\" with administrator privileges", line.replace('"', "\\\""));
        Command::new("osascript").args(["-e", &script]).status()?
    } else {
        Command::new("pkexec").arg(program).args(args).status()?
    };
    Ok(status.code().unwrap_or(-1))
}
