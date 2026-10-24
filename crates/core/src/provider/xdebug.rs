// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Xdebug DLLs for Windows PHP builds, from xdebug.org (ADR 0006). The
//! download page lists every build with its SHA-256:
//! `<a title="SHA256:&nbsp;<hash>" href='/files/php_xdebug-…dll'>`.

use anyhow::{Context, Result};

use super::{Archive, Asset};
use crate::http::Http;
use crate::version::Version;

const PAGE: &str = "https://xdebug.org/download/historical";
const SITE: &str = "https://xdebug.org";

/// The download page, fetched once for all PHP versions.
pub fn index(http: &Http) -> Result<String> {
    http.get_text(PAGE)
}

/// The newest stable Xdebug build for a PHP line (e.g. `8.4`):
/// non-thread-safe, 64-bit. Returns its Xdebug version and the asset.
pub fn for_php(index: &str, php_line: &str) -> Result<(Version, Asset)> {
    best(index, php_line).with_context(|| format!("xdebug.org has no Windows build for PHP {php_line}"))
}

fn best(page: &str, php_line: &str) -> Option<(Version, Asset)> {
    let mut found: Vec<(Version, Asset)> = Vec::new();
    for (i, _) in page.match_indices("href='/files/php_xdebug-") {
        let rest = &page[i + "href='".len()..];
        let Some(path) = rest.split('\'').next() else { continue };
        let name = path.trim_start_matches("/files/");
        // php_xdebug-<xdebug>-<php line>-<parts>-x86_64.dll, where <parts>
        // is "nts-vs17" for PHP 8 and "vc15-nts" for PHP 7.4; thread-safe
        // builds lack "nts".
        let parts = name
            .strip_prefix("php_xdebug-")
            .and_then(|r| r.split_once('-'))
            .and_then(|(_, r)| r.strip_prefix(&format!("{php_line}-")))
            .and_then(|r| r.strip_suffix("-x86_64.dll"));
        if !parts.is_some_and(|p| p.split('-').any(|x| x == "nts")) {
            continue;
        }
        // Pre-releases ("3.6.0alpha1") do not parse as versions; skip them.
        let Some(version) = name.strip_prefix("php_xdebug-").and_then(|r| r.split('-').next()) else {
            continue;
        };
        let Ok(version) = version.parse::<Version>() else { continue };
        // The hash sits in the title attribute just before href.
        let before = &page[i.saturating_sub(120)..i];
        let sha256 = before
            .rsplit("SHA256:&nbsp;")
            .next()
            .map(|s| s.chars().take_while(|c| c.is_ascii_hexdigit()).collect::<String>())
            .filter(|h| h.len() == 64);
        let asset = Asset {
            sha256,
            ..Asset::new(format!("{SITE}{path}"), Archive::File { name: "php_xdebug.dll".into() })
        };
        found.push((version, asset));
    }
    found.into_iter().max_by(|a, b| a.0.cmp(&b.0))
}

#[cfg(test)]
mod tests {
    #[test]
    fn picks_newest_stable_build_with_checksum() {
        let h = "a".repeat(64);
        let page = format!(
            r#"<a title="SHA256:&nbsp;{h}" href='/files/php_xdebug-3.5.1-8.4-nts-vs17-x86_64.dll'>x</a>
            <a title="SHA256:&nbsp;{h}" href='/files/php_xdebug-3.6.0alpha1-8.4-nts-vs17-x86_64.dll'>x</a>
            <a title="SHA256:&nbsp;{h}" href='/files/php_xdebug-3.5.3-8.4-nts-vs17-x86_64.dll'>x</a>
            <a title="SHA256:&nbsp;{h}" href='/files/php_xdebug-3.5.3-8.4-ts-vs17-x86_64.dll'>x</a>
            <a title="SHA256:&nbsp;{h}" href='/files/php_xdebug-3.5.3-8.3-nts-vs16-x86_64.dll'>x</a>
            <a title="SHA256:&nbsp;{h}" href='/files/php_xdebug-3.1.6-7.4-vc15-nts-x86_64.dll'>x</a>
            <a title="SHA256:&nbsp;{h}" href='/files/php_xdebug-3.1.6-7.4-vc15-x86_64.dll'>x</a>"#
        );
        let (v, asset) = super::best(&page, "8.4").unwrap();
        assert_eq!(v.to_string(), "3.5.3");
        assert!(asset.url.ends_with("/files/php_xdebug-3.5.3-8.4-nts-vs17-x86_64.dll"));
        assert_eq!(asset.sha256.as_deref(), Some(h.as_str()));
        let (v74, a74) = super::best(&page, "7.4").unwrap();
        assert_eq!(v74.to_string(), "3.1.6");
        assert!(a74.url.ends_with("php_xdebug-3.1.6-7.4-vc15-nts-x86_64.dll"));
        assert!(super::best(&page, "7.3").is_none());
    }
}
