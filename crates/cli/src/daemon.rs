// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Talking to `meowd`, and starting it when needed.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use meow_core::ipc::{Client, Request};
use meow_core::layout::Layout;
use meow_core::platform;

use crate::print;

pub fn call(layout: &Layout, request: Request) -> Result<serde_json::Value> {
    Client::connect(layout)?.call(request)
}

/// Tells a running `meowd` to re-read its configuration; does nothing if
/// MeowServer is not running.
pub fn reload_if_running(layout: &Layout) -> Result<()> {
    match Client::connect(layout) {
        Ok(mut client) => client.call(Request::Reload).map(drop),
        Err(_) => Ok(()),
    }
}

/// Closes this folder's window, asks `meowd` to stop everything and exit,
/// and waits until it has. The installer relies on this before replacing
/// or removing the program files.
pub fn quit(layout: &Layout) -> Result<()> {
    // The window first, so it cannot start `meowd` again.
    platform::end_program(&layout.root().join(format!("meowserver{}", platform::EXE_SUFFIX)));
    let Ok(mut client) = Client::connect(layout) else { return Ok(()) };
    // meowd may close the pipe as it exits; that is the expected outcome.
    let _ = client.call(Request::Shutdown);
    drop(client);
    let deadline = Instant::now() + Duration::from_secs(60);
    while Client::connect(layout).is_ok() {
        if Instant::now() > deadline {
            bail!(
                "MeowServer is still running after a minute; see {}",
                layout.logs().join("meowd.log").display()
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

pub fn status(layout: &Layout, json: bool) -> Result<()> {
    match Client::connect(layout) {
        Ok(mut client) => print::status(&client.call(Request::Status)?, json),
        Err(_) => {
            println!("MeowServer is not running. Start it with `meow up`.");
            Ok(())
        }
    }
}

/// Connects to `meowd`, starting it first if needed.
pub fn ensure(layout: &Layout, headless: bool) -> Result<Client> {
    meow_core::ipc::ensure_running(layout, headless)
}
