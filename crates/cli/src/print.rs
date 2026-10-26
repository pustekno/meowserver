// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Human-readable output.

use anyhow::Result;
use meow_core::engine::Status;
use meow_core::site::Site;
use meow_core::supervisor::State;

pub fn json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// `web_port`: read after the status call, which may have moved it.
pub fn status(value: &serde_json::Value, as_json: bool, web_port: u16) -> Result<()> {
    if as_json {
        return json(value);
    }
    let status: Status = serde_json::from_value(value.clone())?;
    if status.processes.is_empty() {
        println!("No services running.");
    }
    for p in &status.processes {
        let state = match &p.state {
            State::Starting => "starting".to_owned(),
            State::Running { pid } => format!("running (pid {pid})"),
            State::Stopped => "stopped".to_owned(),
            State::Failed { reason } => format!("FAILED: {reason}"),
        };
        let restarts = if p.restarts > 0 { format!(", {} restarts", p.restarts) } else { String::new() };
        println!("{:<16} {state}{restarts}", p.id);
    }
    println!();
    sites(&status.sites, &status.warnings, web_port);
    Ok(())
}

pub fn sites(sites: &[Site], warnings: &[String], web_port: u16) {
    if sites.is_empty() {
        println!("No sites yet. Add a folder to www/ or run `meow link <folder>`.");
    }
    let width = sites.iter().map(|s| site_url(&s.host, web_port).len()).max().unwrap_or(0);
    for s in sites {
        let url = site_url(&s.host, web_port);
        let target = match &s.proxy {
            Some(p) => format!("→ {p}"),
            None => s.docroot.display().to_string(),
        };
        let php = s.php.as_deref().map(|v| format!("  [php {v}]")).unwrap_or_default();
        // Projects with their own server: runtime and how to start it.
        let app = match (s.framework.runtime(), &s.runtime) {
            (Some(kind), Some(line)) => format!("  [{} {line}; meow dev start {}]", kind.id(), s.name),
            (Some(kind), None) => format!("  [{}: not installed]", kind.id()),
            _ => String::new(),
        };
        println!("{url:<width$} {target}{php}{app}");
    }
    for w in warnings {
        println!("warning: {w}");
    }
}

/// The address to open: the port shows unless it is 80, e.g. after
/// another program held port 80 and MeowServer moved to 8080.
pub fn site_url(host: &str, web_port: u16) -> String {
    if web_port == 80 { format!("http://{host}") } else { format!("http://{host}:{web_port}") }
}
