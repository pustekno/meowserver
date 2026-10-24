// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Apache for Windows from Apache Lounge, the build Laragon uses. The page
//! offers only the current build; each zip has a `.txt` with its SHA-256.

use anyhow::Result;

use super::{Archive, Asset, Provider, Release};
use crate::http::Http;
use crate::version::Version;

const SITE: &str = "https://www.apachelounge.com";

pub struct ApacheLounge;

impl Provider for ApacheLounge {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let page = http.get_text(&format!("{SITE}/download/"))?;
        Ok(parse(&page).into_iter().collect())
    }
}

/// Finds `/download/VSxx/binaries/httpd-2.4.69-261002-Win64-VSxx.zip`.
fn parse(page: &str) -> Option<Release> {
    let path = page.split("href=\"").skip(1).find_map(|rest| {
        let href = &rest[..rest.find('"')?];
        let name = href.rsplit('/').next()?;
        (href.contains("/binaries/")
            && name.starts_with("httpd-")
            && name.contains("-Win64-")
            && name.ends_with(".zip"))
        .then_some(href)
    })?;
    let name = path.rsplit('/').next()?;
    let version: Version = name.strip_prefix("httpd-")?.split('-').next()?.parse().ok()?;
    let url = format!("{SITE}{path}");
    Some(Release {
        version,
        assets: vec![Asset {
            sha256_url: Some(format!("{url}.txt")),
            root: Some("Apache24".into()),
            ..Asset::new(url, Archive::Zip)
        }],
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_download_link() {
        let page = r#"<a href="/download/VS18/modules/x.zip">m</a>
            <a href="/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip">Apache</a>
            <a href="/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip.txt">sum</a>"#;
        let rel = super::parse(page).unwrap();
        assert_eq!(rel.version.to_string(), "2.4.69");
        assert!(rel.assets[0].url.ends_with("httpd-2.4.69-261002-Win64-VS18.zip"));
        assert!(rel.assets[0].sha256_url.as_deref().unwrap().ends_with(".zip.txt"));
    }
}
