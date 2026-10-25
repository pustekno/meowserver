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

pub fn status(value: &serde_json::Value, as_json: bool) -> Result<()> {
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
    sites(&status.sites, &status.warnings);
    Ok(())
}

pub fn sites(sites: &[Site], warnings: &[String]) {
    if sites.is_empty() {
        println!("No sites yet. Add a folder to www/ or run `meow link <folder>`.");
    }
    let width = sites.iter().map(|s| s.host.len()).max().unwrap_or(0) + 8;
    for s in sites {
        let url = format!("http://{}", s.host);
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
