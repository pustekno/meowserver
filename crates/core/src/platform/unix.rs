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
