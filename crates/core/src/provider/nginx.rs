// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Nginx for Windows from nginx.org's download listing. nginx.org signs
//! releases with PGP but publishes no SHA-256.

use anyhow::Result;

use super::{Archive, Asset, Provider, Release, latest_per_line};
use crate::http::Http;
use crate::version::Version;

const BASE: &str = "https://nginx.org/download/";

pub struct Windows;

impl Provider for Windows {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let listing = http.get_text(BASE)?;
        let mut out: Vec<Release> = zip_names(&listing)
            .filter_map(|name| {
                let version: Version = name.strip_prefix("nginx-")?.strip_suffix(".zip")?.parse().ok()?;
                Some(Release { version, assets: vec![Asset::new(format!("{BASE}{name}"), Archive::Zip)] })
            })
            .collect();
        latest_per_line(&mut out);
        Ok(out)
    }
}

/// `nginx-1.31.8.zip` names from `href="…"` attributes.
fn zip_names(html: &str) -> impl Iterator<Item = &str> {
    html.split("href=\"").skip(1).filter_map(|rest| {
        let name = &rest[..rest.find('"')?];
        (name.starts_with("nginx-") && name.ends_with(".zip")).then_some(name)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_zip_links() {
        let html = r#"<a href="nginx-1.30.0.zip">x</a> <a href="nginx-1.30.0.zip.asc">y</a>
                      <a href="nginx-1.31.8.tar.gz">z</a> <a href="nginx-1.31.8.zip">w</a>"#;
        let names: Vec<&str> = super::zip_names(html).collect();
        assert_eq!(names, ["nginx-1.30.0.zip", "nginx-1.31.8.zip"]);
    }
}
