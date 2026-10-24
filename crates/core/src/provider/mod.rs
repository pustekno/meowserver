// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Version lists read straight from official sources (PRD §9.5). A change
//! in an upstream format must only affect that source's file.

mod github;
mod php;

use anyhow::Result;
use serde::Serialize;

use crate::http::Http;
use crate::service::Kind;
use crate::version::Version;

#[derive(Debug, Clone, Serialize)]
pub struct Release {
    pub version: Version,
    /// Usually one archive; PHP on macOS/Linux needs two (FPM and CLI).
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Asset {
    pub url: String,
    /// Official SHA-256, when upstream publishes one.
    pub sha256: Option<String>,
    pub archive: Archive,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Archive {
    Zip,
    TarGz,
    /// A single file saved under `name`, e.g. `composer.phar`.
    File {
        name: String,
    },
}

pub trait Provider {
    /// Available releases for this OS and CPU, newest first.
    fn releases(&self, http: &Http) -> Result<Vec<Release>>;
}

/// The provider for a service on this platform, if it can be downloaded.
pub fn for_service(kind: Kind) -> Option<Box<dyn Provider>> {
    use crate::platform::{Os, os};
    match (kind, os()) {
        (Kind::Caddy, _) => Some(Box::new(github::caddy())),
        (Kind::Php, Os::Windows) => Some(Box::new(php::Windows)),
        (Kind::Php, _) => Some(Box::new(php::StaticPhp)),
        _ => None,
    }
}

fn sort_newest_first(releases: &mut [Release]) {
    releases.sort_by(|a, b| b.version.cmp(&a.version));
}
