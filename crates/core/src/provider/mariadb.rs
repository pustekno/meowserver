// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! MariaDB from the downloads.mariadb.org REST API, which lists stable
//! release series and SHA-256 for every file. One request per series.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, sort_newest_first};
use crate::http::Http;
use crate::platform::{Arch, Os, arch, os};

const API: &str = "https://downloads.mariadb.org/rest-api/mariadb/";

pub struct Rest;

#[derive(Deserialize)]
struct Majors {
    major_releases: Vec<Major>,
}

#[derive(Deserialize)]
struct Major {
    release_id: String,
    release_status: String,
}

#[derive(Deserialize)]
struct Latest {
    releases: BTreeMap<String, ReleaseInfo>,
}

#[derive(Deserialize)]
struct ReleaseInfo {
    files: Vec<File>,
}

#[derive(Deserialize)]
struct File {
    file_name: String,
    file_download_url: String,
    checksum: Checksums,
}

#[derive(Deserialize)]
struct Checksums {
    sha256sum: Option<String>,
}

impl Provider for Rest {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let majors: Majors = http.get_json(API)?;
        let mut out = Vec::new();
        for major in majors.major_releases.iter().filter(|m| m.release_status == "Stable") {
            let latest: Latest = http.get_json(&format!("{API}{}/latest/", major.release_id))?;
            for (version, info) in latest.releases {
                let Ok(version) = version.parse() else { continue };
                let Some(file) = info.files.into_iter().find(|f| wanted(&f.file_name)) else { continue };
                // The API hands out http:// links; the same paths work over HTTPS.
                let url = file.file_download_url.replacen("http://", "https://", 1);
                let archive = if url.ends_with(".zip") { Archive::Zip } else { Archive::TarGz };
                out.push(Release {
                    version,
                    assets: vec![Asset { sha256: file.checksum.sha256sum, ..Asset::new(url, archive) }],
                });
            }
        }
        sort_newest_first(&mut out);
        Ok(out)
    }
}

fn wanted(name: &str) -> bool {
    match (os(), arch()) {
        (Os::Windows, Arch::X64) => name.ends_with("-winx64.zip"),
        (Os::Linux, Arch::X64) => name.ends_with("-linux-systemd-x86_64.tar.gz"),
        (Os::Linux, Arch::Arm64) => name.ends_with("-linux-systemd-aarch64.tar.gz"),
        _ => false,
    }
}
