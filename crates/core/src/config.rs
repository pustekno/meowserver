// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `meow.toml`: app settings and per-site settings (PRD §9.9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::layout::Layout;

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema: u32,
    pub general: General,
    /// Active version per service, by major.minor (e.g. `php = "8.4"`).
    pub versions: BTreeMap<String, String>,
    pub services: BTreeMap<String, ServiceSettings>,
    pub php: PhpSettings,
    pub sites: BTreeMap<String, SiteSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub language: String,
    pub suffix: Suffix,
    pub web_server: WebServer,
    pub www: PathBuf,
    pub parked: Vec<PathBuf>,
    pub start_on_login: bool,
    pub autostart: Vec<String>,
    pub editor: Option<String>,
    pub lan_access: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Suffix {
    Localhost,
    Test,
}

impl Suffix {
    pub fn as_str(self) -> &'static str {
        match self {
            Suffix::Localhost => "localhost",
            Suffix::Test => "test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebServer {
    Caddy,
    Nginx,
    Apache,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServiceSettings {
    pub port: Option<u16>,
    pub tls_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PhpSettings {
    pub extensions: Vec<String>,
    pub ini: BTreeMap<String, String>,
    /// Number of `php-cgi` processes per PHP version on Windows (PRD §9.4).
    pub pool_size: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteSettings {
    /// Set for projects added from anywhere (PRD F8).
    pub path: Option<PathBuf>,
    pub php: Option<String>,
    /// Document root override, relative to the project folder (PRD F10).
    pub root: Option<PathBuf>,
    /// Proxy target such as `http://127.0.0.1:3000` (PRD F12).
    pub proxy: Option<String>,
    pub force_https: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            general: General::default(),
            versions: BTreeMap::new(),
            services: BTreeMap::new(),
            php: PhpSettings::default(),
            sites: BTreeMap::new(),
        }
    }
}

impl Default for General {
    fn default() -> Self {
        Self {
            language: "en".into(),
            suffix: Suffix::Localhost,
            web_server: WebServer::Caddy,
            www: PathBuf::from("www"),
            parked: Vec::new(),
            start_on_login: false,
            autostart: vec!["web".into(), "php".into(), "mysql".into()],
            editor: None,
            lan_access: false,
        }
    }
}

impl Default for PhpSettings {
    fn default() -> Self {
        let extensions = [
            "curl",
            "fileinfo",
            "gd",
            "intl",
            "mbstring",
            "mysqli",
            "openssl",
            "pdo_mysql",
            "pdo_pgsql",
            "pdo_sqlite",
            "pgsql",
            "sodium",
            "sqlite3",
            "zip",
        ];
        Self {
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
            ini: BTreeMap::new(),
            pool_size: 4,
        }
    }
}

impl Config {
    /// Reads `meow.toml`; a missing file yields the defaults.
    pub fn load(layout: &Layout) -> Result<Self> {
        let path = layout.config_file();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        let config: Config =
            toml::from_str(&text).with_context(|| format!("{} is not valid", path.display()))?;
        if config.schema > SCHEMA {
            bail!(
                "{} was written by a newer MeowServer (schema {}); this version understands schema {SCHEMA}",
                path.display(),
                config.schema
            );
        }
        Ok(config)
    }

    /// Writes `meow.toml` atomically: a crash never leaves a half-written file.
    pub fn save(&self, layout: &Layout) -> Result<()> {
        let path = layout.config_file();
        let text = toml::to_string_pretty(self)?;
        write_atomic(&path, text.as_bytes())
    }

    pub fn port(&self, service: &str, default: u16) -> u16 {
        self.services.get(service).and_then(|s| s.port).unwrap_or(default)
    }

    pub fn tls_port(&self, service: &str, default: u16) -> u16 {
        self.services.get(service).and_then(|s| s.tls_port).unwrap_or(default)
    }

    pub fn version_selector(&self, service: &str) -> Option<&str> {
        self.versions.get(service).map(String::as_str)
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prd_example() {
        let text = r#"
            schema = 1
            [general]
            suffix = "test"
            web_server = "nginx"
            parked = ["D:/projects"]
            [versions]
            php = "8.4"
            [services.mysql]
            port = 3307
            [sites.asis]
            path = "D:/projects/asis"
            php = "8.3"
            force_https = true
            [sites.frontend]
            proxy = "http://127.0.0.1:3000"
        "#;
        let c: Config = toml::from_str(text).unwrap();
        assert_eq!(c.general.suffix, Suffix::Test);
        assert_eq!(c.general.web_server, WebServer::Nginx);
        assert_eq!(c.port("mysql", 3306), 3307);
        assert_eq!(c.port("web", 80), 80);
        assert_eq!(c.sites["asis"].php.as_deref(), Some("8.3"));
        assert!(c.sites["asis"].force_https);
        assert_eq!(c.general.language, "en", "missing keys fall back to defaults");
    }

    #[test]
    fn round_trips() {
        let c = Config::default();
        let back: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(back.php.extensions, c.php.extensions);
    }
}
