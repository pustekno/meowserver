// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! MySQL: Oracle offers no download API, so versions come from the
//! hand-curated `data/mysql-versions.toml` (PRD §9.5). No SHA-256 is
//! published (only MD5 on the web page).

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, sort_newest_first};
use crate::http::Http;
use crate::version::Version;

const LIST: &str = include_str!("../../../../data/mysql-versions.toml");

pub struct Curated;

#[derive(Deserialize)]
struct List {
    release: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    version: Version,
}

impl Provider for Curated {
    fn releases(&self, _http: &Http) -> Result<Vec<Release>> {
        let list: List = toml::from_str(LIST)?;
        let mut out: Vec<Release> = list
            .release
            .into_iter()
            .map(|e| {
                let (v, line) = (&e.version, e.version.line());
                let url = format!("https://cdn.mysql.com/Downloads/MySQL-{line}/mysql-{v}-winx64.zip");
                Release { version: e.version.clone(), assets: vec![Asset::new(url, Archive::Zip)] }
            })
            .collect();
        sort_newest_first(&mut out);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_list_parses() {
        let releases = Curated.releases(&Http::new()).unwrap();
        assert!(releases.iter().any(|r| r.version.line() == "8.4"));
    }
}
