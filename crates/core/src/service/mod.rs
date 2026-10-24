// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Managed services and the versions installed for each (PRD §5.1).

pub mod caddy;
pub mod php;

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::layout::Layout;
use crate::version::{self, Version};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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
}

impl Kind {
    pub const ALL: [Kind; 11] = [
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
        }
    }

    pub fn from_id(id: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.id() == id)
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

/// Writes a generated config file, creating its folder.
pub(crate) fn write_generated(path: &Path, text: &str) -> anyhow::Result<()> {
    crate::config::write_atomic(path, text.as_bytes())
}
