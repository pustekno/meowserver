// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! phpMyAdmin from phpmyadmin.net: the versions its own update check
//! lists, each with a `.sha256` file next to the archive.

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, sort_newest_first};
use crate::http::Http;

pub struct Official;

#[derive(Deserialize)]
struct Versions {
    releases: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    version: String,
}

impl Provider for Official {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let versions: Versions = http.get_json("https://www.phpmyadmin.net/home_page/version.json")?;
        let mut out: Vec<Release> = versions
            .releases
            .into_iter()
            .filter_map(|e| {
                let name = format!("phpMyAdmin-{}-all-languages", e.version);
                let url = format!("https://files.phpmyadmin.net/phpMyAdmin/{}/{name}.zip", e.version);
                let asset = Asset {
                    sha256_url: Some(format!("{url}.sha256")),
                    root: Some(name),
                    ..Asset::new(url, Archive::Zip)
                };
                Some(Release { version: e.version.parse().ok()?, assets: vec![asset] })
            })
            .collect();
        sort_newest_first(&mut out);
        Ok(out)
    }
}
