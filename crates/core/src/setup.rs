// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! First setup from the installer (PRD F48), so MeowServer is ready the
//! first time it opens. The installer asks which web server and database,
//! then: `meow setup plan` resolves the newest official releases and their
//! SHA-256 into a folder; the installer downloads the files listed in
//! `downloads.txt` with its own progress bar, checking each SHA-256; and
//! `meow setup apply` installs them from that folder and records the
//! choices. A first import from another tool is offered the same way.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{Config, WebServer};
use crate::download;
use crate::http::Http;
use crate::layout::Layout;
use crate::provider::{self, Release};
use crate::service::{self, Kind};

pub struct Choices {
    pub web: WebServer,
    /// MySQL (8.4, what Laragon users have), MariaDB, PostgreSQL, or none.
    pub database: Option<Kind>,
}

#[derive(Serialize, Deserialize)]
pub struct Step {
    pub kind: Kind,
    pub release: Release,
}

fn web_kind(web: WebServer) -> Kind {
    match web {
        WebServer::Caddy => Kind::Caddy,
        WebServer::Nginx => Kind::Nginx,
        WebServer::Apache => Kind::Apache,
    }
}

/// The releases to install, newest first in each, with every checksum
/// resolved so the installer can check the files itself.
pub fn plan(http: &Http, choices: &Choices) -> Result<Vec<Step>> {
    let mut wanted: Vec<(Kind, Option<&str>)> = vec![(web_kind(choices.web), None), (Kind::Php, None)];
    if let Some(db) = choices.database {
        wanted.push((db, (db == Kind::Mysql).then_some("8.4")));
    }
    wanted.push((Kind::Composer, None));

    let mut steps = Vec::new();
    for (kind, selector) in wanted {
        let provider = provider::for_service(kind)
            .with_context(|| format!("{} cannot be downloaded here", kind.id()))?;
        let mut release = provider
            .releases(http)?
            .into_iter()
            .find(|r| selector.is_none_or(|s| r.version.matches(s)))
            .with_context(|| format!("no release of {} found", kind.id()))?;
        for asset in &mut release.assets {
            if asset.sha256.is_none()
                && let Some(url) = &asset.sha256_url
            {
                asset.sha256 = Some(provider::fetch_checksum(http, url, asset.file_name())?);
            }
        }
        steps.push(Step { kind, release });
    }
    Ok(steps)
}

/// Writes `plan.json` (for `apply`) and `downloads.txt`, one file per
/// line as `url<TAB>file name<TAB>sha256` (empty when upstream has none).
pub fn write_plan(steps: &[Step], dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("plan.json"), serde_json::to_vec_pretty(steps)?)?;
    let lines: Vec<String> = steps
        .iter()
        .flat_map(|s| &s.release.assets)
        .map(|a| {
            format!("{}\t{}\t{}", a.url, download::file_name(&a.url), a.sha256.clone().unwrap_or_default())
        })
        .collect();
    std::fs::write(dir.join("downloads.txt"), lines.join("\n") + "\n")?;
    Ok(())
}

/// Installs what `plan.json` in `dir` lists from the files downloaded next
/// to it, then makes them what MeowServer runs. Versions already installed
/// (an update, a second run) are kept.
pub fn apply(layout: &Layout, dir: &Path) -> Result<Vec<String>> {
    layout.ensure()?;
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(dir.join("plan.json"))?)?;
    let mut notes = Vec::new();
    let mut config = Config::load(layout)?;
    for step in &steps {
        let id = step.kind.id();
        let version = step.release.version.to_string();
        if service::select(layout, step.kind, Some(&version)).is_none() {
            download::install_from_folder(layout, id, &step.release, dir)?;
            notes.push(format!("installed {id} {version}"));
        }
        match step.kind {
            Kind::Caddy => config.general.web_server = WebServer::Caddy,
            Kind::Nginx => config.general.web_server = WebServer::Nginx,
            Kind::Apache => config.general.web_server = WebServer::Apache,
            Kind::Mysql => {
                config.versions.insert(id.into(), step.release.version.line());
            }
            _ => {}
        }
        let unit = match step.kind {
            Kind::Caddy | Kind::Nginx | Kind::Apache => Some("web"),
            Kind::Php => Some("php"),
            Kind::Mysql | Kind::Mariadb | Kind::Postgres => Some(id),
            _ => None,
        };
        if let Some(unit) = unit
            && !config.general.autostart.iter().any(|a| a == unit)
        {
            config.general.autostart.push(unit.into());
        }
    }
    config.save(layout)?;
    Ok(notes)
}

/// Asks the window to open the import plan for `tool` the first time it
/// starts (the installer's "Bring my projects from Laragon").
pub fn offer_import(layout: &Layout, tool: &str) -> Result<()> {
    std::fs::create_dir_all(layout.data())?;
    std::fs::write(layout.data().join("first-run-import.txt"), tool)?;
    Ok(())
}

/// The tool to offer importing from, once: reading it removes the request.
pub fn take_import_offer(layout: &Layout) -> Option<String> {
    let path = layout.data().join("first-run-import.txt");
    let tool = std::fs::read_to_string(&path).ok()?.trim().to_owned();
    let _ = std::fs::remove_file(path);
    (!tool.is_empty()).then_some(tool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Archive, Asset};

    fn step(kind: Kind, version: &str, folder: &Path, name: &str, body: &[u8]) -> Step {
        use sha2::{Digest, Sha256};
        let url = format!("https://example.invalid/{version}/{name}");
        std::fs::write(folder.join(download::file_name(&url)), body).unwrap();
        let mut asset = Asset::new(url, Archive::File { name: name.into() });
        asset.sha256 = Some(Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect());
        Step { kind, release: Release { version: version.parse().unwrap(), assets: vec![asset] } }
    }

    #[test]
    fn apply_installs_the_downloaded_files_and_records_the_choices() {
        let root = std::env::temp_dir().join(format!("meow-setup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let folder = root.join("tmp");
        std::fs::create_dir_all(&folder).unwrap();
        let layout = Layout::at(root.join("meow"));
        let steps = [
            step(Kind::Nginx, "1.31.8", &folder, "nginx.exe", b"nginx"),
            step(Kind::Composer, "2.10.3", &folder, "composer.phar", b"composer"),
        ];
        write_plan(&steps, &folder).unwrap();
        let listed = std::fs::read_to_string(folder.join("downloads.txt")).unwrap();
        assert_eq!(listed.lines().count(), 2);
        assert!(listed.lines().all(|l| l.split('\t').count() == 3));

        assert_eq!(apply(&layout, &folder).unwrap().len(), 2);
        let config = Config::load(&layout).unwrap();
        assert_eq!(config.general.web_server, WebServer::Nginx);
        assert!(config.general.autostart.iter().any(|a| a == "web"));
        assert!(service::select(&layout, Kind::Composer, None).is_some());
        // A second run (an update) keeps what is there.
        assert!(apply(&layout, &folder).unwrap().is_empty());

        // A tampered download is refused.
        std::fs::write(folder.join("composer.phar"), b"evil").unwrap();
        let bad = Layout::at(root.join("other"));
        assert!(apply(&bad, &folder).is_err());

        offer_import(&layout, "laragon").unwrap();
        assert_eq!(take_import_offer(&layout).as_deref(), Some("laragon"));
        assert_eq!(take_import_offer(&layout), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
