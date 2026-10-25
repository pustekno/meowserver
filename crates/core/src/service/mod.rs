// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Managed services and the versions installed for each (PRD §5.1).

pub mod apache;
pub mod caddy;
pub mod datadir;
pub mod mailpit;
pub mod mysql;
pub mod nginx;
pub mod php;
pub mod postgres;
pub mod redis;
pub mod web;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::Layout;
use crate::version::{self, Version};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Caddy,
    Nginx,
    Apache,
    Php,
    Mysql,
    Mariadb,
    Postgres,
    Redis,
    Mailpit,
    Node,
    Composer,
    Python,
    Go,
}

impl Kind {
    pub const ALL: [Kind; 13] = [
        Kind::Caddy,
        Kind::Nginx,
        Kind::Apache,
        Kind::Php,
        Kind::Mysql,
        Kind::Mariadb,
        Kind::Postgres,
        Kind::Redis,
        Kind::Mailpit,
        Kind::Node,
        Kind::Composer,
        Kind::Python,
        Kind::Go,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Kind::Caddy => "caddy",
            Kind::Nginx => "nginx",
            Kind::Apache => "apache",
            Kind::Php => "php",
            Kind::Mysql => "mysql",
            Kind::Mariadb => "mariadb",
            Kind::Postgres => "postgres",
            Kind::Redis => "redis",
            Kind::Mailpit => "mailpit",
            Kind::Node => "node",
            Kind::Composer => "composer",
            Kind::Python => "python",
            Kind::Go => "go",
        }
    }

    pub fn from_id(id: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.id() == id)
    }
}

/// What users start and stop: `general.autostart`, `meow start <unit>`.
/// "web" is whichever web server is configured; "php" is every PHP pool in
/// use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Mysql,
    Mariadb,
    Postgres,
    Redis,
    Mailpit,
    Php,
    Web,
}

impl Unit {
    /// In start order: databases first, the web server last.
    pub const ALL: [Unit; 7] =
        [Unit::Mysql, Unit::Mariadb, Unit::Postgres, Unit::Redis, Unit::Mailpit, Unit::Php, Unit::Web];

    pub fn id(self) -> &'static str {
        match self {
            Unit::Mysql => "mysql",
            Unit::Mariadb => "mariadb",
            Unit::Postgres => "postgres",
            Unit::Redis => "redis",
            Unit::Mailpit => "mailpit",
            Unit::Php => "php",
            Unit::Web => "web",
        }
    }

    /// Also accepts web server names and `postgresql`.
    pub fn parse(name: &str) -> Option<Unit> {
        match name {
            "caddy" | "nginx" | "apache" | "httpd" => Some(Unit::Web),
            "postgresql" | "pgsql" => Some(Unit::Postgres),
            _ => Unit::ALL.into_iter().find(|u| u.id() == name),
        }
    }

    /// The process ids that belong to this unit.
    pub fn owns(self, process_id: &str) -> bool {
        match self {
            Unit::Web => matches!(process_id, "caddy" | "nginx" | "apache"),
            Unit::Php => process_id.starts_with("php-"),
            // `mysql`, and `mysql-5.7` for a version running next to it.
            Unit::Mysql | Unit::Mariadb => process_id
                .strip_prefix(self.id())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('-')),
            other => process_id == other.id(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub kind: Kind,
    pub version: Version,
    pub dir: PathBuf,
}

impl Installed {
    /// Path to an executable inside this version's folder (`bin` adds the
    /// platform suffix, e.g. `.exe`).
    pub fn exe(&self, rel: &str) -> PathBuf {
        self.dir.join(format!("{rel}{}", crate::platform::EXE_SUFFIX))
    }
}

/// Installed versions of `kind`, newest first.
pub fn installed(layout: &Layout, kind: Kind) -> Vec<Installed> {
    let dir = layout.service_bin(kind.id());
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out: Vec<Installed> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let version = e.file_name().to_str()?.parse::<Version>().ok()?;
            Some(Installed { kind, version, dir: e.path() })
        })
        .collect();
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

/// The installed version matching `selector`, or the newest one.
pub fn select(layout: &Layout, kind: Kind, selector: Option<&str>) -> Option<Installed> {
    let all = installed(layout, kind);
    let best = version::pick(all.iter().map(|i| &i.version), selector)?.clone();
    all.into_iter().find(|i| i.version == best)
}

/// Before a new version of a database is installed, pin the one in use, so
/// the newer one does not silently become active with an empty data folder.
/// Moving data to it is an explicit step (`meow db upgrade`, PRD F25).
pub fn pin_before_install(layout: &Layout, kind: Kind) -> anyhow::Result<()> {
    if !matches!(kind, Kind::Mysql | Kind::Mariadb | Kind::Postgres) {
        return Ok(());
    }
    let mut config = crate::config::Config::load(layout)?;
    if config.version_selector(kind.id()).is_some() {
        return Ok(());
    }
    if let Some(current) = select(layout, kind, None) {
        config.versions.insert(kind.id().into(), datadir::line(kind, &current.version));
        config.save(layout)?;
    }
    Ok(())
}

/// Writes a generated config file, creating its folder.
pub(crate) fn write_generated(path: &Path, text: &str) -> anyhow::Result<()> {
    crate::config::write_atomic(path, text.as_bytes())
}
