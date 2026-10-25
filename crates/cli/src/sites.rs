// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `sites`, `link`, `unlink`, `park`, `unpark`, `site`.

use std::path::{Path, PathBuf};

use anyhow::Result;
use meow_core::config::Config;
use meow_core::layout::Layout;
use meow_core::site::{self, manage};

use crate::{daemon, print};

pub fn list(layout: &Layout, json: bool) -> Result<()> {
    let config = Config::load(layout)?;
    let discovery = site::discover(layout, &config);
    if json {
        return print::json(&discovery);
    }
    print::sites(&discovery.sites, &discovery.warnings);
    Ok(())
}

#[derive(clap::Subcommand)]
pub enum DevAction {
    /// Start the project's dev server (npm run dev, manage.py runserver, go run .).
    Start { site: String },
    /// Stop it.
    Stop { site: String },
}

pub fn dev(layout: &Layout, action: DevAction) -> Result<()> {
    use meow_core::ipc::Request;
    match action {
        DevAction::Start { site } => {
            daemon::call(layout, Request::StartApp { site: site.clone() })?;
            let config = Config::load(layout)?;
            if let Some(s) = site::discover(layout, &config).sites.into_iter().find(|s| s.name == site) {
                println!("Started. Open http://{}", s.host);
                println!("Its output: {}", meow_core::app::log_file(layout, &site).display());
            }
        }
        DevAction::Stop { site } => {
            daemon::call(layout, Request::StopApp { site })?;
            println!("Stopped.");
        }
    }
    Ok(())
}

/// Loads `meow.toml`, applies `change`, saves, and reloads a running
/// MeowServer. The watcher would notice too; this makes it immediate.
fn edit(layout: &Layout, change: impl FnOnce(&mut Config) -> Result<String>) -> Result<()> {
    let mut config = Config::load(layout)?;
    let message = change(&mut config)?;
    manage::tidy(&mut config);
    config.save(layout)?;
    println!("{message}");
    daemon::reload_if_running(layout)
}

pub fn new_project(layout: &Layout, template: &str, name: &str, package: Option<&str>) -> Result<()> {
    let template = meow_core::create::Template::parse(template, package)?;
    let dir = meow_core::create::create(layout, &template, name, &mut |text| println!("{text}"))?;
    let config = Config::load(layout)?;
    let slug = site::slugify(name).unwrap_or_default();
    println!("Created {} → http://{slug}.{}", dir.display(), config.general.suffix.as_str());
    daemon::reload_if_running(layout)
}

pub fn link(layout: &Layout, path: &Path, name: Option<&str>) -> Result<()> {
    edit(layout, |config| {
        let name = manage::link(layout, config, path, name)?;
        Ok(format!("Serving {} at http://{name}.{}", path.display(), config.general.suffix.as_str()))
    })
}

pub fn unlink(layout: &Layout, name: &str) -> Result<()> {
    edit(layout, |config| {
        manage::unlink(config, name)?;
        Ok(format!("{name} is no longer served. No files were touched."))
    })
}

pub fn park(layout: &Layout, dir: &Path) -> Result<()> {
    edit(layout, |config| {
        let dir = manage::park(config, dir)?;
        Ok(format!("Every folder in {} is now a site.", dir.display()))
    })
}

pub fn unpark(layout: &Layout, dir: &Path) -> Result<()> {
    edit(layout, |config| {
        manage::unpark(config, dir)?;
        Ok(format!("{} is no longer parked.", dir.display()))
    })
}

pub fn configure(
    layout: &Layout,
    name: &str,
    php: Option<String>,
    proxy: Option<String>,
    root: Option<String>,
    domain: Option<String>,
) -> Result<()> {
    edit(layout, |config| {
        let mut changes = Vec::new();
        if let Some(target) = proxy {
            let name = manage::proxy(layout, config, name, &target)?;
            let to = config.sites[&name].proxy.clone().unwrap_or_default();
            changes.push(format!("{name} now forwards to {to}"));
            if target == "auto" {
                let port = to.rsplit(':').next().unwrap_or_default().to_owned();
                changes.push(format!(
                    "start your dev server on port {port} (e.g. `npm run dev -- --port {port}`)"
                ));
            }
        }
        let settings = manage::settings(config, name);
        if let Some(php) = php {
            settings.php = (php != "default").then(|| php.clone());
            changes.push(format!("PHP: {php}"));
        }
        if let Some(root) = root {
            settings.root = (root != "auto").then(|| PathBuf::from(&root));
            changes.push(format!("document root: {root}"));
        }
        if let Some(domain) = domain {
            settings.suffix = match domain.as_str() {
                "default" => None,
                other => Some(
                    meow_core::config::Suffix::parse(other)
                        .ok_or_else(|| anyhow::anyhow!("--domain is localhost, test, or default"))?,
                ),
            };
            changes.push(format!("domain: {domain}"));
        }
        if changes.is_empty() {
            anyhow::bail!("nothing to change; see `meow site --help`");
        }
        Ok(format!("{name}: {}", changes.join(", ")))
    })
}
