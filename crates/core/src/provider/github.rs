// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! GitHub Releases. GitHub publishes a SHA-256 digest for every asset.
//!
//! The API answers 60 requests an hour per address without a token, which
//! a class or an office behind one address uses up. Then the releases come
//! from the repository's Atom feed instead, which has no such limit but no
//! digests either.

use anyhow::{Context, Result};
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
        match self.api_releases(http) {
            Ok(releases) => Ok(releases),
            Err(api) => self.feed_releases(http).map_err(|feed| anyhow::anyhow!("{api:#}; then {feed:#}")),
        }
    }
}

impl GitHub {
    fn api_releases(&self, http: &Http) -> Result<Vec<Release>> {
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

    /// The feed lists tags only: a pre-release tag (`v2.11.0-beta.1`) is not
    /// a version and is skipped, and the asset is assumed to be there.
    fn feed_releases(&self, http: &Http) -> Result<Vec<Release>> {
        let url = format!("https://github.com/{}/releases.atom", self.repo);
        let feed = http.get_text(&url)?;
        let mut out = Vec::new();
        for tag in feed_tags(&feed) {
            let Ok(version) = tag.parse::<Version>() else { continue };
            let Some((name, archive)) = (self.asset_name)(&version) else { continue };
            let asset = Asset::new(
                format!("https://github.com/{}/releases/download/{tag}/{name}", self.repo),
                archive,
            );
            out.push(Release { version, assets: vec![asset] });
        }
        sort_newest_first(&mut out);
        out.dedup_by(|a, b| a.version == b.version);
        (!out.is_empty()).then_some(out).with_context(|| format!("no releases in {url}"))
    }
}

/// The release tags in a GitHub releases feed, from each entry's
/// `<link … href="https://github.com/<repo>/releases/tag/<tag>"/>`.
fn feed_tags(feed: &str) -> Vec<String> {
    feed.split("/releases/tag/")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|tag| !tag.is_empty() && !tag.contains(['<', '>', '/', ' ']))
        .map(str::to_owned)
        .collect()
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

/// Adminer with every driver (MySQL/MariaDB and PostgreSQL), as one file.
pub fn adminer() -> GitHub {
    GitHub {
        repo: "vrana/adminer",
        asset_name: |v| Some((format!("adminer-{v}.php"), Archive::File { name: "adminer.php".into() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_come_from_the_feed_links() {
        let feed = r#"<feed><entry><id>tag:github.com,2008:Repository/1/v6.2.0</id>
<link rel="alternate" type="text/html" href="https://github.com/vrana/adminer/releases/tag/v6.2.0"/>
</entry><entry>
<link rel="alternate" type="text/html" href="https://github.com/vrana/adminer/releases/tag/v6.2.0-beta.1"/>
</entry><entry>
<link rel="alternate" type="text/html" href="https://github.com/r/redis/releases/tag/8.10.3"/>
</entry></feed>"#;
        assert_eq!(feed_tags(feed), ["v6.2.0", "v6.2.0-beta.1", "8.10.3"]);
        assert!("v6.2.0-beta.1".parse::<Version>().is_err());
    }
}
