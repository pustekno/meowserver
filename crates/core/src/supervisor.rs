// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Starts, watches, restarts, and stops service processes (PRD §8.2, §9.3).
//!
//! One thread per process waits for it to exit and applies its restart
//! policy. Output goes straight to a log file, so no thread copies pipes.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::logging;
use crate::platform::{self, ProcessTree};

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    /// Unique id, e.g. `caddy` or `php-8.4#2`.
    pub id: String,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: Option<PathBuf>,
    pub log: PathBuf,
    pub restart: Restart,
    /// A command that asks the service to shut down cleanly, run before
    /// the process tree is killed (e.g. `mysqladmin shutdown`).
    pub graceful_stop: Option<Invocation>,
}

impl ProcessSpec {
    /// True if both would start the same program the same way.
    pub fn same_command(&self, other: &ProcessSpec) -> bool {
        self.program == other.program
            && self.args == other.args
            && self.env == other.env
            && self.cwd == other.cwd
    }
}

/// A one-off helper command, such as `mysqladmin shutdown` or
/// `caddy reload`.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restart {
    Never,
    /// Restart after a crash, up to [`CRASH_LIMIT`] times per minute.
    OnFailure,
    /// Also restart after a clean exit. Used for `php-cgi`, which exits on
    /// purpose after `PHP_FCGI_MAX_REQUESTS` requests.
    Always,
}

const CRASH_LIMIT: usize = 3;
const CRASH_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    Starting,
    Running { pid: u32 },
    Stopped,
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub id: String,
    #[serde(flatten)]
    pub state: State,
    pub restarts: u32,
}

#[derive(Debug, Clone)]
pub enum Event {
    StateChanged { id: String, state: State },
}

struct Entry {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

struct Shared {
    spec: ProcessSpec,
    state: Mutex<State>,
    tree: Mutex<Option<ProcessTree>>,
    stopping: AtomicBool,
    restarts: Mutex<u32>,
    events: Option<Sender<Event>>,
}

impl Shared {
    fn set_state(&self, state: State) {
        *self.state.lock().unwrap() = state.clone();
        if let Some(tx) = &self.events {
            let _ = tx.send(Event::StateChanged { id: self.spec.id.clone(), state });
        }
    }
}

#[derive(Clone, Default)]
pub struct Supervisor {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    events: Option<Sender<Event>>,
}

impl Supervisor {
    pub fn new(events: Option<Sender<Event>>) -> Self {
        Self { entries: Arc::default(), events }
    }

    /// Starts a process unless one with the same id is already running.
    pub fn start(&self, spec: ProcessSpec) -> Result<()> {
        let running_differs = {
            let entries = self.entries.lock().unwrap();
            match entries.get(&spec.id) {
                Some(entry)
                    if !matches!(
                        *entry.shared.state.lock().unwrap(),
                        State::Stopped | State::Failed { .. }
                    ) =>
                {
                    if entry.shared.spec.same_command(&spec) {
                        return Ok(());
                    }
                    true
                }
                _ => false,
            }
        };
        // Same id, different command (e.g. a new port): replace it.
        if running_differs {
            self.stop(&spec.id)?;
        }
        let mut entries = self.entries.lock().unwrap();
        let shared = Arc::new(Shared {
            spec,
            state: Mutex::new(State::Starting),
            tree: Mutex::new(None),
            stopping: AtomicBool::new(false),
            restarts: Mutex::new(0),
            events: self.events.clone(),
        });
        // Spawn the first instance here so a missing binary is reported to
        // the caller instead of only showing up as a failed state.
        let first = spawn(&shared.spec).with_context(|| format!("cannot start {}", shared.spec.id))?;
        let id = shared.spec.id.clone();
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name(format!("watch {id}"))
            .spawn(move || watch(thread_shared, first))?;
        entries.insert(id, Entry { shared, thread: Some(thread) });
        Ok(())
    }

    /// Stops a process: graceful command first if it has one, then kills the
    /// whole process tree.
    pub fn stop(&self, id: &str) -> Result<()> {
        let (shared, thread) = {
            let mut entries = self.entries.lock().unwrap();
            let Some(entry) = entries.get_mut(id) else { bail!("{id} is not running") };
            (entry.shared.clone(), entry.thread.take())
        };
        shared.stopping.store(true, Ordering::SeqCst);
        if let Some(stop) = &shared.spec.graceful_stop {
            if run_graceful_stop(stop) {
                wait_until_stopped(&shared, stop.timeout);
            }
        }
        if let Some(tree) = shared.tree.lock().unwrap().as_ref() {
            tree.kill();
        }
        if let Some(thread) = thread {
            let _ = thread.join();
        }
        Ok(())
    }

    /// Stops every process whose id starts with `prefix`.
    pub fn stop_matching(&self, prefix: &str) {
        let ids: Vec<String> =
            self.entries.lock().unwrap().keys().filter(|id| id.starts_with(prefix)).cloned().collect();
        stop_in_parallel(self, ids);
    }

    pub fn stop_all(&self) {
        let ids: Vec<String> = self.entries.lock().unwrap().keys().cloned().collect();
        stop_in_parallel(self, ids);
    }

    pub fn status(&self) -> Vec<Status> {
        let entries = self.entries.lock().unwrap();
        let mut out: Vec<Status> = entries
            .values()
            .map(|e| Status {
                id: e.shared.spec.id.clone(),
                state: e.shared.state.lock().unwrap().clone(),
                restarts: *e.shared.restarts.lock().unwrap(),
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.entries.lock().unwrap().get(id).is_some_and(|e| {
            matches!(*e.shared.state.lock().unwrap(), State::Running { .. } | State::Starting)
        })
    }
}

fn stop_in_parallel(sup: &Supervisor, ids: Vec<String>) {
    std::thread::scope(|s| {
        for id in &ids {
            s.spawn(move || {
                if let Err(e) = sup.stop(id) {
                    log::warn!("stopping {id}: {e:#}");
                }
            });
        }
    });
}

fn spawn(spec: &ProcessSpec) -> Result<(std::process::Child, ProcessTree)> {
    let log = logging::open_rotated(&spec.log)
        .with_context(|| format!("cannot open log {}", spec.log.display()))?;
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    Ok(platform::spawn_service(&mut cmd)?)
}

fn watch(shared: Arc<Shared>, first: (std::process::Child, ProcessTree)) {
    let spec = &shared.spec;
    let mut crashes: Vec<Instant> = Vec::new();
    let mut current = Some(first);

    loop {
        let (mut child, tree) = match current.take() {
            Some(pair) => pair,
            None => match spawn(spec) {
                Ok(pair) => pair,
                Err(e) => {
                    log::error!("{}: {e:#}", spec.id);
                    shared.set_state(State::Failed { reason: format!("{e:#}") });
                    return;
                }
            },
        };
        let pid = child.id();
        *shared.tree.lock().unwrap() = Some(tree);
        shared.set_state(State::Running { pid });
        if shared.stopping.load(Ordering::SeqCst) {
            // stop() raced with a restart and missed this instance.
            if let Some(tree) = shared.tree.lock().unwrap().as_ref() {
                tree.kill();
            }
        }

        let exit = child.wait();
        *shared.tree.lock().unwrap() = None;

        if shared.stopping.load(Ordering::SeqCst) {
            shared.set_state(State::Stopped);
            return;
        }

        let success = exit.as_ref().is_ok_and(|s| s.success());
        let describe = match &exit {
            Ok(s) => format!("exited with {s}"),
            Err(e) => format!("could not be waited on: {e}"),
        };
        let restart = match spec.restart {
            Restart::Never => false,
            Restart::OnFailure => !success,
            Restart::Always => true,
        };
        if !restart {
            if success {
                shared.set_state(State::Stopped);
            } else {
                log::warn!("{} {describe}", spec.id);
                shared.set_state(State::Failed { reason: describe });
            }
            return;
        }

        if !success {
            let now = Instant::now();
            crashes.retain(|t| now.duration_since(*t) < CRASH_WINDOW);
            crashes.push(now);
            log::warn!("{} {describe}", spec.id);
            if crashes.len() > CRASH_LIMIT {
                let reason = format!("{describe}; gave up after {CRASH_LIMIT} restarts in a minute");
                shared.set_state(State::Failed { reason });
                return;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        *shared.restarts.lock().unwrap() += 1;
        shared.set_state(State::Starting);
    }
}

fn run_graceful_stop(stop: &Invocation) -> bool {
    run(stop)
}

/// Runs a helper command to completion; true if it succeeded in time.
pub fn run(stop: &Invocation) -> bool {
    let mut cmd = Command::new(&stop.program);
    cmd.args(&stop.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match platform::spawn_service(&mut cmd) {
        Ok((mut child, _tree)) => {
            let deadline = Instant::now() + stop.timeout;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => return status.success(),
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
                    _ => return false,
                }
            }
        }
        Err(e) => {
            log::warn!("graceful stop {}: {e}", stop.program.display());
            false
        }
    }
}

fn wait_until_stopped(shared: &Shared, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if shared.tree.lock().unwrap().is_none() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
