// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! GitHub Releases. GitHub publishes a SHA-256 digest for every asset.

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, sort_newest_first};
use crate::http::Http;
use crate::platform::{Arch, Os, arch, os};
use crate::version::Version;

pub struct GitHub {
    repo: &'static str,
    /// The asset file name for a version on this platform, if one exists.
    asset_name: fn(&Version) -> Option<(String, Archive)>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

impl Provider for GitHub {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let url = format!("https://api.github.com/repos/{}/releases?per_page=30", self.repo);
        let api: Vec<ApiRelease> = http.get_json(&url)?;
        let mut out = Vec::new();
        for rel in api.into_iter().filter(|r| !r.draft && !r.prerelease) {
            let Ok(version) = rel.tag_name.parse::<Version>() else { continue };
            let Some((name, archive)) = (self.asset_name)(&version) else { continue };
            let Some(asset) = rel.assets.into_iter().find(|a| a.name == name) else { continue };
            let sha256 = asset.digest.and_then(|d| d.strip_prefix("sha256:").map(str::to_owned));
            let mut asset_out = Asset::new(asset.browser_download_url, archive);
            asset_out.sha256 = sha256;
            out.push(Release { version, assets: vec![asset_out] });
        }
        sort_newest_first(&mut out);
        Ok(out)
    }
}

pub fn caddy() -> GitHub {
    GitHub {
        repo: "caddyserver/caddy",
        asset_name: |v| {
            let (os, archive, ext) = match os() {
                Os::Windows => ("windows", Archive::Zip, "zip"),
                Os::MacOs => ("mac", Archive::TarGz, "tar.gz"),
                Os::Linux => ("linux", Archive::TarGz, "tar.gz"),
            };
            let arch = match arch() {
                Arch::X64 => "amd64",
                Arch::Arm64 => "arm64",
            };
            Some((format!("caddy_{v}_{os}_{arch}.{ext}"), archive))
        },
    }
}

pub fn mailpit() -> GitHub {
    GitHub {
        repo: "axllent/mailpit",
        asset_name: |_| {
            let arch = match arch() {
                Arch::X64 => "amd64",
                Arch::Arm64 => "arm64",
            };
            Some(match os() {
                Os::Windows => (format!("mailpit-windows-{arch}.zip"), Archive::Zip),
                Os::MacOs => (format!("mailpit-darwin-{arch}.tar.gz"), Archive::TarGz),
                Os::Linux => (format!("mailpit-linux-{arch}.tar.gz"), Archive::TarGz),
            })
        },
    }
}

/// Redis has no official Windows build; this community port is the one
/// Laragon users know (PRD §9.5).
pub fn redis_windows() -> GitHub {
    GitHub {
        repo: "redis-windows/redis-windows",
        asset_name: |v| {
            (arch() == Arch::X64).then(|| (format!("Redis-{v}-Windows-x64-msys2.zip"), Archive::Zip))
        },
    }
}
