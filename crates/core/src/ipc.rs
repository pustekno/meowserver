// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Client ↔ `meowd` protocol (PRD §9.3): newline-delimited JSON over a
//! named pipe (Windows) or Unix socket. Each request gets one response.

use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, Stream, prelude::*};
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

pub type Handler = Arc<dyn Fn(Request) -> Response + Send + Sync>;

/// Starts serving requests on a background thread. Fails if another
/// `meowd` already owns this folder's endpoint.
pub fn serve(layout: &Layout, handler: Handler) -> Result<()> {
    let listener = ListenerOptions::new()
        .name(endpoint(layout)?)
        .create_sync()
        .context("another MeowServer is already running for this folder")?;
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
