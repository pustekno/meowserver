// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Updates (PRD F37). The daily check is off by default and only asks
//! GitHub Releases whether a newer version exists. Downloading and
//! installing happen only when the user presses Update: the installer is
//! fetched from the release, its SHA-256 checked against GitHub's, and run
//! on this very folder, which keeps every project, database, and setting.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::http::Http;
use crate::layout::Layout;
use crate::platform::{Os, os};
use crate::version::Version;

/// Pre-releases included: every 0.x version is published as one.
const API: &str = "https://api.github.com/repos/pustekno/meowserver/releases?per_page=10";
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone)]
pub struct Available {
    pub version: Version,
    /// The release page.
    pub url: String,
    /// The Windows installer and its SHA-256, when the release has one.
    pub setup: Option<(String, String)>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

/// The newest release if it is newer than this build. No release yet (404)
/// counts as "up to date".
pub fn check(http: &Http) -> Result<Option<Available>> {
    let releases: Vec<ApiRelease> = match http.get_json(API) {
        Ok(r) => r,
        Err(e) if format!("{e:#}").contains("404") => return Ok(None),
        Err(e) => return Err(e),
    };
    let current: Version = crate::VERSION.parse()?;
    let newest = releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| Some((r.tag_name.parse::<Version>().ok()?, r)))
        .max_by(|a, b| a.0.cmp(&b.0));
    let Some((version, release)) = newest.filter(|(v, _)| *v > current) else { return Ok(None) };
    let setup = release
        .assets
        .into_iter()
        .find(|a| a.name.ends_with("-windows-x86_64-setup.exe"))
        .and_then(|a| Some((a.browser_download_url, a.digest?.strip_prefix("sha256:")?.to_owned())));
    Ok(Some(Available { version, url: release.html_url, setup }))
}

/// Downloads the new installer, checks it, and starts it on this folder.
/// It quits MeowServer, replaces only the program files, and opens
/// MeowServer again; the caller should exit right after.
pub fn install(
    layout: &Layout,
    http: &Http,
    available: &Available,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<()> {
    if os() != Os::Windows {
        bail!("updating from the window is for Windows; download the new version from {}", available.url);
    }
    let Some((url, sha256)) = &available.setup else {
        bail!("this release has no Windows installer; see {}", available.url);
    };
    let dir = layout.tmp().join("update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let setup = dir.join(format!("MeowServer-{}-setup.exe", available.version));
    let got = http.download(url, &setup, progress)?;
    if !got.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(&setup);
        bail!("the downloaded installer does not match the release's SHA-256; nothing was installed");
    }
    run_setup(&setup, layout.root())
}

/// `/DIR` names this folder, so a portable copy is updated in place too.
fn run_setup(setup: &Path, root: &Path) -> Result<()> {
    let mut cmd = Command::new(setup);
    cmd.args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]).arg(format!("/DIR={}", root.display()));
    crate::platform::spawn_detached(&mut cmd).map(drop).context("could not start the installer")
}

/// True when the last check was more than a day ago; records this one.
pub fn due(layout: &Layout) -> bool {
    let stamp = layout.tmp().join("update-check");
    let fresh = std::fs::metadata(&stamp)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).is_ok_and(|age| age < EVERY));
    if fresh {
        return false;
    }
    let _ = std::fs::create_dir_all(layout.tmp());
    let _ = std::fs::write(&stamp, b"");
    true
}
