// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Watches `www/`, parked folders, and `meow.toml` so new projects appear
//! without any action (PRD F6). Uses the OS change notifications, not
//! polling, and only one level deep: edits inside a project are ignored.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher as _};

use crate::config::Config;
use crate::layout::Layout;

/// Quiet time before acting, so a `git clone` or unzip triggers one reload.
const SETTLE: Duration = Duration::from_millis(500);

/// Calls `on_change` after the watched folders change. Runs until the
/// process exits.
pub fn spawn(layout: Layout, on_change: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("watch".into())
        .spawn(move || {
            if let Err(e) = run(&layout, &on_change) {
                log::warn!("folder watching stopped: {e:#}");
            }
        })
        .expect("spawning the watcher thread");
}

fn run(layout: &Layout, on_change: &dyn Fn()) -> anyhow::Result<()> {
    let config_file = layout.config_file();
    loop {
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = event {
                let _ = tx.send(event);
            }
        })?;
        let dirs = watched_dirs(layout);
        for dir in &dirs {
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                log::warn!("cannot watch {}: {e}", dir.display());
            }
        }

        // Block until something relevant happens, then let it settle.
        let mut config_changed = false;
        loop {
            let event = rx.recv()?;
            if is_relevant(&event, layout, &config_file, &mut config_changed) {
                break;
            }
        }
        while let Ok(event) = rx.recv_timeout(SETTLE) {
            is_relevant(&event, layout, &config_file, &mut config_changed);
        }
        drop(watcher);
        on_change();
        // Parked folders may have changed; the loop re-reads them.
    }
}

fn watched_dirs(layout: &Layout) -> Vec<PathBuf> {
    let config = Config::load(layout).unwrap_or_default();
    let mut dirs = vec![layout.root().to_path_buf(), layout.resolve(&config.general.www)];
    dirs.extend(config.general.parked.iter().cloned());
    dirs.retain(|d| d.is_dir());
    dirs.dedup();
    dirs
}

/// Changes in the MeowServer root only matter for `meow.toml`; in project
/// folders, any created, removed, or renamed entry matters.
fn is_relevant(
    event: &notify::Event,
    layout: &Layout,
    config_file: &PathBuf,
    config_changed: &mut bool,
) -> bool {
    use notify::EventKind;
    if event.paths.iter().any(|p| p == config_file) {
        *config_changed = true;
        return true;
    }
    let structural = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
    );
    structural && event.paths.iter().any(|p| p.parent() != Some(layout.root()))
}
