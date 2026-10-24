// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Node.js from nodejs.org/dist, newest release of each major version.
//! Checksums come from each release's SHASUMS256.txt.

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, sort_newest_first};
use crate::http::Http;
use crate::platform::{Arch, Os, arch, os};
use crate::version::Version;

const DIST: &str = "https://nodejs.org/dist/";

pub struct Dist;

#[derive(Deserialize)]
struct Entry {
    version: String,
    files: Vec<String>,
}

impl Provider for Dist {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let index: Vec<Entry> = http.get_json(&format!("{DIST}index.json"))?;
        let (key, platform, ext, archive) = target();
        let mut out = Vec::new();
        for entry in index {
            if !entry.files.iter().any(|f| f == key) {
                continue;
            }
            let Ok(version) = entry.version.parse::<Version>() else { continue };
            if out.iter().any(|r: &Release| r.version.major() == version.major()) {
                continue;
            }
            let v = &entry.version;
            let url = format!("{DIST}{v}/node-{v}-{platform}.{ext}");
            let sums = format!("{DIST}{v}/SHASUMS256.txt");
            out.push(Release {
                version,
                assets: vec![Asset { sha256_url: Some(sums), ..Asset::new(url, archive.clone()) }],
            });
        }
        sort_newest_first(&mut out);
        Ok(out)
    }
}

/// (key in index.json, platform in the file name, extension, archive kind)
fn target() -> (&'static str, &'static str, &'static str, Archive) {
    match (os(), arch()) {
        (Os::Windows, Arch::X64) => ("win-x64-zip", "win-x64", "zip", Archive::Zip),
        (Os::Windows, Arch::Arm64) => ("win-arm64-zip", "win-arm64", "zip", Archive::Zip),
        (Os::MacOs, Arch::X64) => ("osx-x64-tar", "darwin-x64", "tar.gz", Archive::TarGz),
        (Os::MacOs, Arch::Arm64) => ("osx-arm64-tar", "darwin-arm64", "tar.gz", Archive::TarGz),
        (Os::Linux, Arch::X64) => ("linux-x64", "linux-x64", "tar.gz", Archive::TarGz),
        (Os::Linux, Arch::Arm64) => ("linux-arm64", "linux-arm64", "tar.gz", Archive::TarGz),
    }
}
