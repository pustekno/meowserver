// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! PHP: official Windows builds from php.net, static builds from
//! static-php-cli on macOS and Linux.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, sort_newest_first};
use crate::http::Http;
use crate::platform::{Arch, Os, arch, os};
use crate::version::Version;

const WINDOWS_BASE: &str = "https://downloads.php.net/~windows/releases/";
const STATIC_BASE: &str = "https://dl.static-php.dev/static-php-cli/common/";

/// `releases.json` lists the latest patch of each supported minor version,
/// with SHA-256 for every zip. Non-thread-safe (NTS) builds are the right
/// ones for FastCGI.
pub struct Windows;

impl Provider for Windows {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let index: BTreeMap<String, serde_json::Value> =
            http.get_json(&format!("{WINDOWS_BASE}releases.json"))?;
        let mut out: Vec<Release> = index.values().filter_map(windows_release).collect();
        sort_newest_first(&mut out);
        Ok(out)
    }
}

fn windows_release(entry: &serde_json::Value) -> Option<Release> {
    let version: Version = entry.get("version")?.as_str()?.parse().ok()?;
    let arch = match arch() {
        Arch::X64 => "-x64",
        Arch::Arm64 => "-arm64",
    };
    let build = entry
        .as_object()?
        .iter()
        .find(|(key, _)| key.starts_with("nts-") && key.ends_with(arch))
        .map(|(_, build)| build)?;
    let zip = build.get("zip")?;
    Some(Release {
        version,
        assets: vec![Asset {
            url: format!("{WINDOWS_BASE}{}", zip.get("path")?.as_str()?),
            sha256: zip.get("sha256").and_then(|s| s.as_str()).map(str::to_owned),
            archive: Archive::Zip,
        }],
    })
}

/// static-php-cli publishes a JSON directory listing. Each version needs
/// two archives: `php-fpm` for sites and `php` for the terminal. No
/// checksums are published.
pub struct StaticPhp;

#[derive(Deserialize)]
struct ListingEntry {
    name: String,
    is_dir: bool,
}

impl Provider for StaticPhp {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let listing: Vec<ListingEntry> = http.get_json(&format!("{STATIC_BASE}?format=json"))?;
        let names: Vec<&str> = listing.iter().filter(|e| !e.is_dir).map(|e| e.name.as_str()).collect();
        let platform = static_platform();
        let mut out = Vec::new();
        for name in &names {
            let Some(version) = name
                .strip_prefix("php-")
                .and_then(|rest| rest.strip_suffix(&format!("-fpm-{platform}.tar.gz")))
            else {
                continue;
            };
            let cli = format!("php-{version}-cli-{platform}.tar.gz");
            if !names.contains(&cli.as_str()) {
                continue;
            }
            let Ok(parsed) = version.parse::<Version>() else { continue };
            let asset = |file: &str| Asset {
                url: format!("{STATIC_BASE}{file}"),
                sha256: None,
                archive: Archive::TarGz,
            };
            out.push(Release { version: parsed, assets: vec![asset(name), asset(&cli)] });
        }
        sort_newest_first(&mut out);
        // Keep the latest patch of each minor version, like php.net does.
        out.dedup_by(|b, a| a.version.line() == b.version.line());
        Ok(out)
    }
}

fn static_platform() -> String {
    let os = if os() == Os::MacOs { "macos" } else { "linux" };
    let arch = match arch() {
        Arch::X64 => "x86_64",
        Arch::Arm64 => "aarch64",
    };
    format!("{os}-{arch}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_nts_build_from_releases_json() {
        let entry = serde_json::json!({
            "version": "8.4.27",
            "ts-vs17-x64": { "zip": { "path": "php-8.4.27-Win32-vs17-x64.zip", "sha256": "aa" } },
            "nts-vs17-x64": { "zip": { "path": "php-8.4.27-nts-Win32-vs17-x64.zip", "sha256": "bb" } },
            "nts-vs17-x86": { "zip": { "path": "php-8.4.27-nts-Win32-vs17-x86.zip", "sha256": "cc" } }
        });
        let rel = windows_release(&entry).unwrap();
        assert_eq!(rel.version.to_string(), "8.4.27");
        if arch() == Arch::X64 {
            assert!(rel.assets[0].url.ends_with("php-8.4.27-nts-Win32-vs17-x64.zip"));
            assert_eq!(rel.assets[0].sha256.as_deref(), Some("bb"));
        }
    }
}
