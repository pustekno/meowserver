// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Windows: each service runs in its own Job Object with
//! `KILL_ON_JOB_CLOSE`. The process is created suspended and only resumed
//! once it is inside the job, so children it starts early (Apache, Nginx
//! workers) cannot escape.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
    TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED,
    DETACHED_PROCESS, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};

pub struct ProcessTree {
    job: Handle,
}

impl ProcessTree {
    pub fn kill(&self) {
        unsafe { TerminateJobObject(self.job.0, 1) };
    }
}

struct Handle(HANDLE);

// A job handle may be used and closed from any thread.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn spawn_service(cmd: &mut Command) -> io::Result<(Child, ProcessTree)> {
    let job = create_kill_on_close_job()?;
    cmd.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    let mut child = cmd.spawn()?;
    let attached = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) } != 0;
    if !attached {
        let err = io::Error::last_os_error();
        let _ = child.kill();
        return Err(err);
    }
    if let Err(err) = resume_threads(child.id()) {
        let _ = child.kill();
        return Err(err);
    }
    Ok((child, ProcessTree { job }))
}

pub fn spawn_detached(cmd: &mut Command) -> io::Result<Child> {
    keep_std_handles_private();
    // Leave any job we were started in (e.g. a terminal's), if it allows it.
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
    match cmd.spawn() {
        Ok(child) => Ok(child),
        Err(_) => {
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            cmd.spawn()
        }
    }
}

/// Windows hands every inheritable handle to child processes. Our own
/// stdout may be a pipe (`meow up | more`); a long-lived child holding it
/// would keep the reader waiting forever.
fn keep_std_handles_private() {
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        let handle = unsafe { GetStdHandle(which) };
        if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
        }
    }
}

pub fn spawn_console(cmd: &mut Command) -> io::Result<Child> {
    keep_std_handles_private();
    use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
    cmd.creation_flags(CREATE_NEW_CONSOLE | CREATE_BREAKAWAY_FROM_JOB);
    match cmd.spawn() {
        Ok(child) => Ok(child),
        Err(_) => {
            cmd.creation_flags(CREATE_NEW_CONSOLE);
            cmd.spawn()
        }
    }
}

fn create_kill_on_close_job() -> io::Result<Handle> {
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = Handle(job);
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&raw const info).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(job)
}

/// Resumes the threads of a process created with `CREATE_SUSPENDED`.
fn resume_threads(pid: u32) -> io::Result<()> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = Handle(snapshot);
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<THREADENTRY32>() as u32;
    let mut resumed = 0;
    let mut more = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid {
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if !thread.is_null() {
                let thread = Handle(thread);
                if unsafe { ResumeThread(thread.0) } != u32::MAX {
                    resumed += 1;
                }
            }
        }
        more = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
    }
    if resumed == 0 {
        return Err(io::Error::other(format!("could not resume process {pid}")));
    }
    Ok(())
}

/// The user's locale name, e.g. `id-ID`.
pub fn locale() -> String {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 1 { "en".into() } else { String::from_utf16_lossy(&buf[..len as usize - 1]) }
}

/// Every process listening on a TCP port, on any IPv4 or IPv6 address.
pub fn listener_pids(port: u16) -> Vec<u32> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

    fn table(family: u32) -> Vec<u8> {
        let mut size = 0u32;
        unsafe {
            GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, family, TCP_TABLE_OWNER_PID_LISTENER, 0)
        };
        let mut buf = vec![0u8; size as usize + 1024];
        let mut size = buf.len() as u32;
        let rc = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr().cast(),
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if rc != 0 { Vec::new() } else { buf }
    }
    // Both tables are a u32 row count followed by the rows.
    fn rows<T: Copy>(buf: &[u8]) -> Vec<T> {
        if buf.len() < 4 {
            return Vec::new();
        }
        let count = u32::from_ne_bytes(buf[..4].try_into().unwrap()) as usize;
        let offset = align_of::<T>().max(4);
        (0..count)
            .filter_map(|i| {
                let start = offset + i * size_of::<T>();
                (start + size_of::<T>() <= buf.len())
                    .then(|| unsafe { std::ptr::read_unaligned(buf[start..].as_ptr().cast::<T>()) })
            })
            .collect()
    }
    let port_of = |raw: u32| u16::from_be(raw as u16);
    let v4 = table(AF_INET as u32);
    let v6 = table(AF_INET6 as u32);
    let mut pids: Vec<u32> = rows::<MIB_TCPROW_OWNER_PID>(&v4)
        .into_iter()
        .filter(|r| port_of(r.dwLocalPort) == port)
        .map(|r| r.dwOwningPid)
        .chain(
            rows::<MIB_TCP6ROW_OWNER_PID>(&v6)
                .into_iter()
                .filter(|r| port_of(r.dwLocalPort) == port)
                .map(|r| r.dwOwningPid),
        )
        .collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

/// Full path of a process's executable, when we are allowed to see it.
pub fn process_path(pid: u32) -> Option<String> {
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let handle = Handle(handle);
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// Whether Windows' RedirectionGuard is enforced on `pid`. Such a process
/// refuses to follow junctions and links made without administrator rights,
/// which is how Laragon users put projects into www. Setup programs (Inno
/// Setup 6.7 and later) enforce it on themselves, and what they start
/// inherits it.
pub fn redirection_guarded(pid: u32) -> bool {
    use windows_sys::Win32::System::Threading::{
        GetProcessMitigationPolicy, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    const PROCESS_REDIRECTION_TRUST_POLICY: i32 = 16;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let handle = Handle(handle);
    let mut flags: u32 = 0;
    let ok = unsafe {
        GetProcessMitigationPolicy(
            handle.0,
            PROCESS_REDIRECTION_TRUST_POLICY,
            (&mut flags as *mut u32).cast(),
            size_of::<u32>(),
        )
    };
    // Bit 0: EnforceRedirectionTrust (bit 1 only audits).
    ok != 0 && flags & 1 != 0
}

/// Ends a process at once. For programs that keep no unsaved state.
pub fn terminate(pid: u32) {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if !handle.is_null() {
        let handle = Handle(handle);
        unsafe { TerminateProcess(handle.0, 1) };
    }
}

/// (pid, executable file name) of every running process.
pub fn process_names() -> Vec<(u32, String)> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let snapshot = Handle(snapshot);
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut out = Vec::new();
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        out.push((entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..len])));
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    out
}

/// Opens a URL or folder with its default program (the browser, Explorer).
pub fn shell_open(target: &std::ffi::OsStr) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain([0]).collect::<Vec<u16>>();
    let verb = wide("open".as_ref());
    let file = wide(target);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    // Called from short-lived worker threads: finish before returning.
    info.fMask = SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.nShow = 1; // SW_SHOWNORMAL
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Runs `program args` with administrator rights after a UAC prompt and
/// waits for it. Returns its exit code; an error if the user declined.
pub fn run_elevated(program: &std::path::Path, args: &[String]) -> io::Result<i32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain([0]).collect::<Vec<u16>>();
    let params: Vec<String> = args.iter().map(|a| quote_arg(a)).collect();
    let verb = wide("runas".as_ref());
    let file = wide(program.as_os_str());
    let params = wide(params.join(" ").as_ref());
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = 0; // SW_HIDE
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.hProcess.is_null() {
        return Err(io::Error::other("the elevated process did not start"));
    }
    let process = Handle(info.hProcess);
    unsafe { WaitForSingleObject(process.0, INFINITE) };
    let mut code = 0u32;
    unsafe { GetExitCodeProcess(process.0, &mut code) };
    Ok(code as i32)
}

/// Quotes one argument the way the C runtime splits command lines.
fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn quotes_like_the_c_runtime() {
        assert_eq!(super::quote_arg("plain"), "plain");
        assert_eq!(super::quote_arg(r"C:\My Folder\"), r#""C:\My Folder\\""#);
        assert_eq!(super::quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
    }
}
