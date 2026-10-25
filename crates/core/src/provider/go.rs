// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Go from go.dev/dl, newest release of each line. The JSON list carries
//! the SHA-256 of every file.

use anyhow::Result;
use serde::Deserialize;

use super::{Archive, Asset, Provider, Release, latest_per_line};
use crate::http::Http;
use crate::platform::{Arch, Os, arch, os};
use crate::version::Version;

const LIST: &str = "https://go.dev/dl/?mode=json&include=all";

/// Older lines are long out of support.
const OLDEST_MINOR: u32 = 20;

pub struct Dl;

#[derive(Deserialize)]
struct Entry {
    version: String,
    stable: bool,
    files: Vec<File>,
}

#[derive(Deserialize)]
struct File {
    filename: String,
    os: String,
    arch: String,
    sha256: String,
    kind: String,
}

impl Provider for Dl {
    fn releases(&self, http: &Http) -> Result<Vec<Release>> {
        let list: Vec<Entry> = http.get_json(LIST)?;
        let (goos, goarch) = target();
        let mut out = Vec::new();
        for entry in list.into_iter().filter(|e| e.stable) {
            let Ok(version) = entry.version.trim_start_matches("go").parse::<Version>() else { continue };
            if version.major() != 1 || version.minor() < OLDEST_MINOR {
                continue;
            }
            let Some(file) =
                entry.files.into_iter().find(|f| f.kind == "archive" && f.os == goos && f.arch == goarch)
            else {
                continue;
            };
            let archive = if file.filename.ends_with(".zip") { Archive::Zip } else { Archive::TarGz };
            let mut asset = Asset::new(format!("https://go.dev/dl/{}", file.filename), archive);
            asset.sha256 = Some(file.sha256);
            asset.root = Some("go".into());
            out.push(Release { version, assets: vec![asset] });
        }
        latest_per_line(&mut out);
        Ok(out)
    }
}

fn target() -> (&'static str, &'static str) {
    let goos = match os() {
        Os::Windows => "windows",
        Os::MacOs => "darwin",
        Os::Linux => "linux",
    };
    let goarch = match arch() {
        Arch::X64 => "amd64",
        Arch::Arm64 => "arm64",
    };
    (goos, goarch)
}
