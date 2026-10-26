// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Client ↔ `meowd` protocol (PRD §9.3): newline-delimited JSON over a
//! named pipe (Windows) or Unix socket. Each request gets one response.

use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use interprocess::local_socket::{GenericNamespaced, Listener, ListenerOptions, Name, Stream, prelude::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::http::to_hex;
use crate::layout::Layout;

pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    StartAll,
    StopAll,
    Start {
        unit: String,
    },
    Stop {
        unit: String,
    },
    Restart {
        unit: String,
    },
    /// Re-read `meow.toml` and the project folders, then apply changes.
    Reload,
    /// Start or stop a project's own development server (PRD F43).
    StartApp {
        site: String,
    },
    StopApp {
        site: String,
    },
    /// Stop all services and exit `meowd`.
    Shutdown,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    v: u32,
    #[serde(flatten)]
    request: Request,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(data: impl Serialize) -> Self {
        Self { ok: true, data: serde_json::to_value(data).unwrap_or(Value::Null), error: None }
    }

    pub fn err(error: impl std::fmt::Display) -> Self {
        Self { ok: false, data: Value::Null, error: Some(error.to_string()) }
    }
}

/// One endpoint per MeowServer folder, so a portable copy and an installed
/// copy never talk to each other's `meowd`.
fn endpoint(layout: &Layout) -> Result<Name<'static>> {
    let root = layout.root().to_string_lossy();
    let key = if cfg!(windows) { root.to_lowercase() } else { root.into_owned() };
    let hash = to_hex(&Sha256::digest(key.as_bytes()));
    let name = format!("meowd-{}", &hash[..12]);
    Ok(name.to_ns_name::<GenericNamespaced>()?.into_owned())
}

pub struct Client {
    conn: BufReader<Stream>,
}

impl Client {
    /// Connects to the running `meowd`, failing if there is none.
    pub fn connect(layout: &Layout) -> Result<Self> {
        let stream = Stream::connect(endpoint(layout)?).context("MeowServer is not running")?;
        Ok(Self { conn: BufReader::new(stream) })
    }

    pub fn call(&mut self, request: Request) -> Result<Value> {
        let mut line = serde_json::to_vec(&Envelope { v: PROTOCOL, request })?;
        line.push(b'\n');
        self.conn.get_mut().write_all(&line)?;
        let mut reply = String::new();
        if self.conn.read_line(&mut reply)? == 0 {
            bail!("MeowServer closed the connection");
        }
        let response: Response = serde_json::from_str(&reply).context("unreadable reply from MeowServer")?;
        if response.ok {
            Ok(response.data)
        } else {
            Err(anyhow!(response.error.unwrap_or_else(|| "unknown error".into())))
        }
    }
}

/// Connects to `meowd`, starting it first if needed. `meowd` is looked for
/// next to the running executable (all three ship together).
pub fn ensure_running(layout: &Layout, headless: bool) -> Result<Client> {
    if let Ok(client) = Client::connect(layout) {
        return Ok(client);
    }
    let meowd = std::env::current_exe()?.with_file_name(format!("meowd{}", std::env::consts::EXE_SUFFIX));
    let mut cmd = std::process::Command::new(&meowd);
    if headless {
        cmd.arg("--headless");
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::platform::spawn_detached(&mut cmd).with_context(|| format!("cannot start {}", meowd.display()))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Ok(client) = Client::connect(layout) {
            return Ok(client);
        }
        if std::time::Instant::now() > deadline {
            bail!("MeowServer did not start; see {}", layout.logs().join("meowd.log").display());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub type Handler = Arc<dyn Fn(Request) -> Response + Send + Sync>;

/// This folder's endpoint, owned but not answering yet: clients that
/// connect wait until [`Endpoint::serve`].
pub struct Endpoint(Listener);

/// Takes this folder's endpoint. Fails if another `meowd` owns it.
pub fn bind(layout: &Layout) -> Result<Endpoint> {
    let listener = match ListenerOptions::new().name(endpoint(layout)?).create_sync() {
        Ok(listener) => listener,
        // On macOS the endpoint is a socket file in /tmp, left behind when
        // `meowd` is killed. Nobody answering on it means it is stale.
        Err(e)
            if e.kind() == std::io::ErrorKind::AddrInUse && Stream::connect(endpoint(layout)?).is_err() =>
        {
            ListenerOptions::new().name(endpoint(layout)?).try_overwrite(true).create_sync()?
        }
        Err(e) => return Err(e).context("another MeowServer is already running for this folder"),
    };
    Ok(Endpoint(listener))
}

impl Endpoint {
    /// Answers requests on a background thread.
    pub fn serve(self, handler: Handler) -> Result<()> {
        let listener = self.0;
        std::thread::Builder::new().name("ipc".into()).spawn(move || {
            for conn in listener.incoming() {
                match conn {
                    Ok(conn) => {
                        let handler = handler.clone();
                        let _ = std::thread::Builder::new()
                            .name("ipc client".into())
                            .spawn(move || serve_client(conn, handler));
                    }
                    Err(e) => log::warn!("ipc accept failed: {e}"),
                }
            }
        })?;
        Ok(())
    }
}

fn serve_client(conn: Stream, handler: Handler) {
    let mut conn = BufReader::new(conn);
    let mut line = String::new();
    loop {
        line.clear();
        match conn.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let response = match serde_json::from_str::<Envelope>(&line) {
            Ok(env) if env.v != PROTOCOL => Response::err(format!(
                "this client speaks protocol {}, MeowServer speaks {PROTOCOL}; please restart MeowServer",
                env.v
            )),
            Ok(env) => handler(env.request),
            Err(e) => Response::err(format!("bad request: {e}")),
        };
        let mut out = serde_json::to_vec(&response).unwrap_or_default();
        out.push(b'\n');
        if conn.get_mut().write_all(&out).is_err() {
            return;
        }
    }
}
