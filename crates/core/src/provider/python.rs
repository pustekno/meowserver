// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Python from python-build-standalone: CPython built to run from any
//! folder on Windows, macOS, and Linux (the builds `uv` installs). Each
//! release carries every supported line; GitHub lists a SHA-256 per file.

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, latest_per_line};
use crate::http::Http;
use crate::platform::{Arch, Os, arch, os};
use crate::version::Version;

const RELEASES: &str = "https://api.github.com/repos/astral-sh/python-build-standalone/releases?per_page=1";

pub struct Standalone;

#[derive(Deserialize)]
struct ApiRelease {
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

impl Provider for Standalone {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let latest: Vec<ApiRelease> = http.get_json(RELEASES)?;
        let triple = triple();
        let suffix = format!("-{triple}-install_only_stripped.tar.gz");
        let mut out = Vec::new();
        for asset in latest.into_iter().flat_map(|r| r.assets) {
            // cpython-3.13.17+20261021-x86_64-pc-windows-msvc-install_only_stripped.tar.gz
            let Some(rest) = asset.name.strip_prefix("cpython-") else { continue };
            if !rest.ends_with(&suffix) {
                continue;
            }
            let Some(version) = rest.split('+').next().and_then(|v| v.parse::<Version>().ok()) else {
                continue;
            };
            let mut out_asset = Asset::new(asset.browser_download_url, Archive::TarGz);
            out_asset.sha256 = asset.digest.and_then(|d| d.strip_prefix("sha256:").map(str::to_owned));
            out_asset.root = Some("python".into());
            out.push(Release { version, assets: vec![out_asset] });
        }
        latest_per_line(&mut out);
        Ok(out)
    }
}

fn triple() -> &'static str {
    match (os(), arch()) {
        (Os::Windows, Arch::X64) => "x86_64-pc-windows-msvc",
        (Os::Windows, Arch::Arm64) => "aarch64-pc-windows-msvc",
        (Os::MacOs, Arch::X64) => "x86_64-apple-darwin",
        (Os::MacOs, Arch::Arm64) => "aarch64-apple-darwin",
        (Os::Linux, Arch::X64) => "x86_64-unknown-linux-gnu",
        (Os::Linux, Arch::Arm64) => "aarch64-unknown-linux-gnu",
    }
}
