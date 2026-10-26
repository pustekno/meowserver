// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Version lists read straight from official sources (PRD §9.5). A change
//! in an upstream format must only affect that source's file.

mod apache;
mod composer;
mod github;
mod go;
mod mariadb;
mod mysql;
mod nginx;
mod node;
mod php;
mod postgres;
mod python;
pub mod xdebug;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::http::Http;
use crate::platform::{Os, os};
use crate::service::Kind;
use crate::version::Version;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub version: Version,
    /// Usually one archive; PHP on macOS/Linux needs two (FPM and CLI).
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub url: String,
    /// Official SHA-256, when upstream lists it with the version.
    pub sha256: Option<String>,
    /// A file holding the official SHA-256, fetched at install time.
    pub sha256_url: Option<String>,
    pub archive: Archive,
    /// The folder inside the archive that holds the program, when the
    /// archive puts other files next to it (e.g. Apache Lounge's `Apache24`).
    pub root: Option<String>,
}

impl Asset {
    pub fn new(url: impl Into<String>, archive: Archive) -> Self {
        Self { url: url.into(), sha256: None, sha256_url: None, archive, root: None }
    }

    pub fn has_checksum(&self) -> bool {
        self.sha256.is_some() || self.sha256_url.is_some()
    }

    pub fn file_name(&self) -> &str {
        self.url.rsplit('/').next().unwrap_or(&self.url)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Archive {
    Zip,
    TarGz,
    /// A single file saved under `name`, e.g. `composer.phar`.
    File {
        name: String,
    },
}

pub trait Provider {
    /// Available releases for this OS and CPU, newest first.
    fn releases(&self, http: &Http) -> Result<Vec<Release>>;
}

/// The provider for a service on this platform, if it can be downloaded.
/// Services without one on macOS/Linux use the system's packages (PRD F4).
pub fn for_service(kind: Kind) -> Option<Box<dyn Provider>> {
    let windows = os() == Os::Windows;
    let linux = os() == Os::Linux;
    Some(match kind {
        Kind::Caddy => Box::new(github::caddy()),
        Kind::Php if windows => Box::new(php::Windows),
        Kind::Php => Box::new(php::StaticPhp),
        Kind::Nginx if windows => Box::new(nginx::Windows),
        Kind::Apache if windows => Box::new(apache::ApacheLounge),
        Kind::Mysql if windows => Box::new(mysql::Curated),
        Kind::Mariadb if windows || linux => Box::new(mariadb::Rest),
        Kind::Postgres if windows => Box::new(postgres::Edb),
        Kind::Redis if windows => Box::new(github::redis_windows()),
        Kind::Mailpit => Box::new(github::mailpit()),
        Kind::Node => Box::new(node::Dist),
        Kind::Composer => Box::new(composer::Phar),
        Kind::Python => Box::new(python::Standalone),
        Kind::Go => Box::new(go::Dl),
        _ => return None,
    })
}

fn sort_newest_first(releases: &mut [Release]) {
    releases.sort_by(|a, b| b.version.cmp(&a.version));
}

/// Keeps the newest patch of each major.minor line.
fn latest_per_line(releases: &mut Vec<Release>) {
    sort_newest_first(releases);
    releases.dedup_by(|later, kept| later.version.line() == kept.version.line());
}

/// Fetches a checksum file and finds the SHA-256 for `file_name`.
pub fn fetch_checksum(http: &Http, url: &str, file_name: &str) -> Result<String> {
    let text = http.get_text(url)?;
    parse_checksum(&text, file_name).with_context(|| format!("no SHA-256 for {file_name} in {url}"))
}

/// Understands the formats upstreams use:
/// `<hash>  <file>` lines (sha256sum, Node, Composer), and Apache Lounge's
/// `SHA256-Checksum for: <file>:` followed by the hash on the next line.
fn parse_checksum(text: &str, file_name: &str) -> Option<String> {
    let is_hash = |t: &str| t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit());
    let hash_in = |line: &str| line.split_whitespace().find(|t| is_hash(t)).map(str::to_ascii_lowercase);
    let lines: Vec<&str> = text.lines().map(str::trim).collect();

    for line in &lines {
        if line.contains(file_name) {
            if let Some(h) = hash_in(line) {
                return Some(h);
            }
        }
    }
    for (i, line) in lines.iter().enumerate() {
        if line.to_ascii_uppercase().contains("SHA256") && line.contains(file_name) {
            if let Some(h) = lines[i + 1..].iter().find(|l| !l.is_empty()).and_then(|l| hash_in(l)) {
                return Some(h);
            }
        }
    }
    let all: Vec<String> = lines.iter().filter_map(|l| hash_in(l)).collect();
    if all.len() == 1 { all.into_iter().next() } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "77d715d0d9cee731e2251e7ddc2090a7a8e6d41fedef2e9827cd918593c5a652";

    #[test]
    fn parses_sha256sum_lines() {
        let text = format!("{}  other.zip\n{H}  node-v24.0.0-win-x64.zip\n", "a".repeat(64));
        assert_eq!(parse_checksum(&text, "node-v24.0.0-win-x64.zip").as_deref(), Some(H));
    }

    #[test]
    fn parses_apache_lounge_format() {
        let text = format!(
            "SHA1-Checksum for: httpd.zip:\nB7372A7DE3E05FD0D11C4E72FF80A11AF0EF4FA5\n\n\
             SHA256-Checksum for: httpd.zip:\n{}\n\nSHA512-Checksum for: httpd.zip:\n{}\n",
            H.to_uppercase(),
            "F".repeat(128)
        );
        assert_eq!(parse_checksum(&text, "httpd.zip").as_deref(), Some(H));
    }

    #[test]
    fn single_hash_file() {
        assert_eq!(parse_checksum(&format!("{H}  composer.phar\n"), "x").as_deref(), Some(H));
    }
}
