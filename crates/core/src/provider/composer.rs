// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Composer from getcomposer.org: stable versions with a `.sha256sum` each.

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, sort_newest_first};
use crate::http::Http;

const SITE: &str = "https://getcomposer.org";

pub struct Phar;

#[derive(Deserialize)]
struct Versions {
    stable: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
    version: String,
}

impl Provider for Phar {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let versions: Versions = http.get_json(&format!("{SITE}/versions"))?;
        let mut out: Vec<Release> = versions
            .stable
            .into_iter()
            .filter_map(|e| {
                let url = format!("{SITE}{}", e.path);
                let asset = Asset {
                    sha256_url: Some(format!("{url}.sha256sum")),
                    ..Asset::new(url, Archive::File { name: "composer.phar".into() })
                };
                Some(Release { version: e.version.parse().ok()?, assets: vec![asset] })
            })
            .collect();
        sort_newest_first(&mut out);
        Ok(out)
    }
}
