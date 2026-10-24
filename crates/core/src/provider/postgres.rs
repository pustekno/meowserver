// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! PostgreSQL for Windows: supported versions from postgresql.org, binaries
//! from EnterpriseDB's zip builds (no checksum published).

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, sort_newest_first};
use crate::http::Http;

pub struct Edb;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Major {
    major: String,
    latest_minor: String,
    supported: bool,
}

impl Provider for Edb {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let majors: Vec<Major> = http.get_json("https://www.postgresql.org/versions.json")?;
        let mut out: Vec<Release> = majors
            .into_iter()
            .filter(|m| m.supported)
            .filter_map(|m| {
                let v = format!("{}.{}", m.major, m.latest_minor);
                let url = format!(
                    "https://get.enterprisedb.com/postgresql/postgresql-{v}-1-windows-x64-binaries.zip"
                );
                Some(Release { version: v.parse().ok()?, assets: vec![Asset::new(url, Archive::Zip)] })
            })
            .collect();
        sort_newest_first(&mut out);
        Ok(out)
    }
}
