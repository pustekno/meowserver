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
    /// Command that opens a database GUI, with `{host}`, `{port}`, `{user}`
    /// (PRD F28). Unset: HeidiSQL when installed.
    pub db_tool: Option<String>,
    pub lan_access: bool,
    /// Write hosts entries for every site (PRD F16).
    pub manage_hosts: bool,
    /// Serve every site over HTTPS too, with the local CA (PRD F18).
    pub https: bool,
    /// Look for a newer MeowServer once a day (PRD F37). Off by default.
    pub check_updates: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Suffix {
    Localhost,
    Test,
}

impl Suffix {
    pub fn parse(text: &str) -> Option<Suffix> {
        match text.trim_start_matches('.') {
            "localhost" => Some(Suffix::Localhost),
            "test" => Some(Suffix::Test),
            _ => None,
        }
    }

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
    /// A second port some services have, e.g. Mailpit's web inbox.
    pub web_port: Option<u16>,
    /// Database data folder, when not the default `data/<service>-<version>`
    /// (e.g. Laragon's, used in place; PRD F27).
    pub data_dir: Option<PathBuf>,
    /// More versions of this database running next to the default one, by
    /// line, each on its own port and with its own data (PRD F24): projects
    /// pick one through the port in their settings.
    /// `[services.mysql.extra]` with `"5.7" = 3307`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, u16>,
    /// The usual port of a field (`port`, `tls_port`, `web_port`) that
    /// MeowServer moved because another program held it. The service goes
    /// back to it once it is free. Delete the entry to keep the new port.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub moved_from: BTreeMap<String, u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PhpSettings {
    pub extensions: Vec<String>,
    pub ini: BTreeMap<String, String>,
    /// Number of `php-cgi` processes per PHP version on Windows (PRD §9.4).
    pub pool_size: u8,
    /// Load Xdebug where it is installed (PRD F20). Off by default: it
    /// slows every request.
    pub xdebug: bool,
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
    /// Domain ending for this site when not `general.suffix`: `.test` for
    /// projects brought from Laragon, whose settings and data name it.
    pub suffix: Option<Suffix>,
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
        // First run follows the OS language when a translation exists
        // (PRD §19); otherwise English.
        let os_language = crate::platform::language();
        Self {
            language: if os_language == "id" { "id".into() } else { "en".into() },
            suffix: Suffix::Localhost,
            web_server: WebServer::Caddy,
            www: PathBuf::from("www"),
            parked: Vec::new(),
            start_on_login: false,
            autostart: vec!["web".into(), "php".into(), "mysql".into()],
            editor: None,
            db_tool: None,
            lan_access: false,
            manage_hosts: true,
            https: true,
            check_updates: false,
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
            xdebug: false,
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
        // Notepad and PowerShell 5 save UTF-8 with a byte-order mark, which
        // TOML does not allow; accept it rather than fail.
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let config: Config =
            toml::from_str(text).with_context(|| format!("{} is not valid", path.display()))?;
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

    pub fn web_port(&self, service: &str, default: u16) -> u16 {
        self.services.get(service).and_then(|s| s.web_port).unwrap_or(default)
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
        assert!(c.general.manage_hosts, "missing keys fall back to defaults");
    }

    #[test]
    fn accepts_byte_order_mark() {
        let dir = std::env::temp_dir().join(format!("meow-bom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("meow.toml"), "\u{feff}[services.web]\nport = 8080\n").unwrap();
        let config = Config::load(&Layout::at(dir.clone())).unwrap();
        assert_eq!(config.port("web", 80), 8080);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn round_trips() {
        let c = Config::default();
        let back: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(back.php.extensions, c.php.extensions);
    }
}
