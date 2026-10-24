// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Update notification (PRD F37): off by default; when on, at most one
//! request a day to GitHub Releases. Nothing is downloaded or installed.

use std::time::{Duration, SystemTime};

use anyhow::Result;
use serde::Deserialize;

use crate::http::Http;
use crate::layout::Layout;
use crate::version::Version;

const API: &str = "https://api.github.com/repos/pustekno/meowserver/releases/latest";
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone)]
pub struct Available {
    pub version: Version,
    pub url: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
}

/// The newest release if it is newer than this build. No release yet (404)
/// counts as "up to date".
pub fn check(http: &Http) -> Result<Option<Available>> {
    let release: ApiRelease = match http.get_json(API) {
        Ok(r) => r,
        Err(e) if format!("{e:#}").contains("404") => return Ok(None),
        Err(e) => return Err(e),
    };
    let Ok(latest) = release.tag_name.parse::<Version>() else { return Ok(None) };
    let current: Version = crate::VERSION.parse()?;
    Ok((latest > current).then_some(Available { version: latest, url: release.html_url }))
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
