// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Database data folders: one per service and version line, and never
//! handed to a server that cannot use it (PRD F24).

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::Kind;
use crate::config::Config;
use crate::layout::Layout;
use crate::version::Version;

/// Written by MeowServer into every data folder it creates.
pub const MARKER: &str = ".meowserver.json";
/// Left by `meow db upgrade` until the new server has upgraded the data.
pub const PENDING_UPGRADE: &str = ".meowserver-upgrade-pending";

#[derive(Serialize, Deserialize)]
struct Marker {
    service: String,
    version: Version,
}

#[derive(Debug, PartialEq)]
pub enum Found {
    /// Missing or empty: needs initializing.
    Empty,
    Data {
        kind: Kind,
        version: Option<Version>,
    },
    /// Has files, but nothing says what created them.
    Unknown,
}

/// The unit data is kept per: major.minor for MySQL/MariaDB, major for
/// PostgreSQL (whose minor releases share a data format).
pub fn line(kind: Kind, version: &Version) -> String {
    if kind == Kind::Postgres { version.major().to_string() } else { version.line() }
}

pub fn path(layout: &Layout, config: &Config, kind: Kind, version: &Version) -> PathBuf {
    config
        .services
        .get(kind.id())
        .and_then(|s| s.data_dir.as_deref())
        .map(|p| layout.resolve(p))
        .unwrap_or_else(|| layout.data().join(format!("{}-{}", kind.id(), line(kind, version))))
}

pub fn detect(dir: &Path) -> Found {
    let Ok(mut entries) = std::fs::read_dir(dir) else { return Found::Empty };
    if entries.next().is_none() {
        return Found::Empty;
    }
    if let Some(m) =
        std::fs::read(dir.join(MARKER)).ok().and_then(|b| serde_json::from_slice::<Marker>(&b).ok())
    {
        if let Some(kind) = Kind::from_id(&m.service) {
            return Found::Data { kind, version: Some(m.version) };
        }
    }
    // MySQL 8.0.16+ records every version that touched the folder.
    if let Ok(text) = std::fs::read_to_string(dir.join("mysql_upgrade_history")) {
        let version = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["upgrade_history"].as_array()?.last()?["version"].as_str()?.parse().ok());
        return Found::Data { kind: Kind::Mysql, version };
    }
    // Older MySQL and MariaDB (newer MariaDB renames the file): a one-line
    // version string.
    let info = std::fs::read_to_string(dir.join("mariadb_upgrade_info"))
        .or_else(|_| std::fs::read_to_string(dir.join("mysql_upgrade_info")));
    if let Ok(text) = info {
        let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        let kind = if text.contains("MariaDB") { Kind::Mariadb } else { Kind::Mysql };
        let version = text.split('-').next().and_then(|v| v.parse().ok());
        return Found::Data { kind, version };
    }
    if let Ok(text) = std::fs::read_to_string(dir.join("PG_VERSION")) {
        return Found::Data { kind: Kind::Postgres, version: text.trim().parse().ok() };
    }
    Found::Unknown
}

/// Checks that `kind` at `version` may use `dir`. Returns true when the
/// folder is empty and must be initialized first.
pub fn check(dir: &Path, kind: Kind, version: &Version) -> Result<bool> {
    match detect(dir) {
        Found::Empty => Ok(true),
        Found::Unknown => Ok(false),
        Found::Data { kind: found, version: found_version } => {
            if found != kind {
                bail!(
                    "{} holds {} data, which {} cannot read. Export it with the original server \
                     and import it instead, or point {} at another folder.",
                    dir.display(),
                    found.id(),
                    kind.id(),
                    kind.id()
                );
            }
            if let Some(found_version) = found_version {
                let (have, want) = (line(found, &found_version), line(kind, version));
                if have != want {
                    if found_version < *version {
                        bail!(
                            "{} was created by {} {found_version}. Upgrading it to {version} is a \
                             one-way step: run `meow db upgrade {}` to back it up and upgrade it.",
                            dir.display(),
                            kind.id(),
                            kind.id()
                        );
                    }
                    bail!(
                        "{} was created by {} {found_version}, newer than the installed {version}. \
                         Install {} {have} or use another folder.",
                        dir.display(),
                        kind.id(),
                        kind.id()
                    );
                }
            }
            Ok(false)
        }
    }
}

pub fn write_marker(dir: &Path, kind: Kind, version: &Version) -> Result<()> {
    let marker = Marker { service: kind.id().into(), version: version.clone() };
    std::fs::write(dir.join(MARKER), serde_json::to_vec_pretty(&marker)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let d = std::env::temp_dir().join(format!("meow-datadir-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for (f, content) in files {
            std::fs::write(d.join(f), content).unwrap();
        }
        d
    }

    fn v(s: &str) -> Version {
        s.parse().unwrap()
    }

    #[test]
    fn reads_laragon_mysql_history() {
        let d = dir(
            "laragon",
            &[(
                "mysql_upgrade_history",
                r#"{"file_format":"1","upgrade_history":[{"date":"2026-04-13","version":"8.4.3","initialize":true}]}"#,
            )],
        );
        assert_eq!(detect(&d), Found::Data { kind: Kind::Mysql, version: Some(v("8.4.3")) });
        assert!(!check(&d, Kind::Mysql, &v("8.4.13")).unwrap(), "same line is fine");
        assert!(check(&d, Kind::Mariadb, &v("11.4.13")).is_err(), "MariaDB must refuse MySQL data");
        assert!(check(&d, Kind::Mysql, &v("9.7.4")).is_err(), "a new line needs an explicit upgrade");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn reads_mariadb_info() {
        let d = dir("mariadb", &[("mysql_upgrade_info", "11.4.13-MariaDB\0")]);
        assert_eq!(detect(&d), Found::Data { kind: Kind::Mariadb, version: Some(v("11.4.13")) });
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn empty_needs_init() {
        let d = dir("empty", &[]);
        assert!(check(&d, Kind::Postgres, &v("17.11")).unwrap());
        std::fs::remove_dir_all(d).unwrap();
    }
}
