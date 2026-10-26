// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Adminer and phpMyAdmin (PRD F28), for people who know them from
//! Laragon. They install like any version and are served as sites
//! (`adminer.localhost`, `phpmyadmin.localhost`) by the web server and the
//! PHP pool. On every start MeowServer writes their settings next to them,
//! listing its database servers so they open logged in. They answer only
//! requests from this computer, even with LAN access on.

use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;
use crate::layout::Layout;
use crate::service::{self, Kind, datadir, mysql, postgres};

pub const TOOLS: [Kind; 2] = [Kind::Adminer, Kind::Phpmyadmin];

/// One of MeowServer's database servers, as the tools list it.
pub struct Server {
    /// `mysql`, `mysql-8.0` (another line running too), `mariadb`, `postgres`.
    pub key: String,
    pub name: String,
    pub postgres: bool,
    pub port: u16,
    pub user: &'static str,
}

pub fn servers(layout: &Layout, config: &Config) -> Vec<Server> {
    let mut out = Vec::new();
    for (kind, name) in [(Kind::Mysql, "MySQL"), (Kind::Mariadb, "MariaDB")] {
        let Some(installed) = service::select(layout, kind, config.version_selector(kind.id())) else {
            continue;
        };
        out.push(Server {
            key: kind.id().into(),
            name: format!("{name} {}", datadir::line(kind, &installed.version)),
            postgres: false,
            port: config.port(kind.id(), mysql::DEFAULT_PORT),
            user: "root",
        });
        for (line, port) in config.services.get(kind.id()).map(|s| s.extra.clone()).unwrap_or_default() {
            out.push(Server {
                key: format!("{}-{line}", kind.id()),
                name: format!("{name} {line}"),
                postgres: false,
                port,
                user: "root",
            });
        }
    }
    if let Some(installed) = service::select(layout, Kind::Postgres, config.version_selector("postgres")) {
        out.push(Server {
            key: "postgres".into(),
            name: format!("PostgreSQL {}", datadir::line(Kind::Postgres, &installed.version)),
            postgres: true,
            port: config.port("postgres", postgres::DEFAULT_PORT),
            user: "postgres",
        });
    }
    out
}

/// The installed tool's folder, which is served as its site.
pub fn folder(layout: &Layout, config: &Config, tool: Kind) -> Option<PathBuf> {
    service::select(layout, tool, config.version_selector(tool.id())).map(|i| i.dir)
}

/// phpMyAdmin is for MySQL and MariaDB only.
pub fn supports(tool: Kind, server: &Server) -> bool {
    tool == Kind::Adminer || !server.postgres
}

/// The address that opens `key` (see [`Server::key`]) in `tool`, logged in.
pub fn url(layout: &Layout, config: &Config, tool: Kind, key: &str) -> Option<String> {
    let servers = servers(layout, config);
    let host = format!("{}.{}", tool.id(), config.general.suffix.as_str());
    let port = config.port("web", 80);
    let base = if port == 80 { format!("http://{host}/") } else { format!("http://{host}:{port}/") };
    match tool {
        Kind::Adminer => servers.iter().any(|s| s.key == key).then(|| format!("{base}?meow={key}")),
        _ => {
            // phpMyAdmin numbers its servers from 1, in the order written.
            let index = servers.iter().filter(|s| supports(tool, s)).position(|s| s.key == key)?;
            Some(format!("{base}index.php?server={}", index + 1))
        }
    }
}

/// Writes each installed tool's settings (on start and reload).
pub fn prepare(layout: &Layout, config: &Config) -> Result<()> {
    let servers = servers(layout, config);
    let list = servers_php(&servers);
    if let Some(dir) = folder(layout, config, Kind::Adminer) {
        write_if_changed(&dir.join("meow-servers.php"), &list)?;
        write_if_changed(&dir.join("index.php"), ADMINER)?;
    }
    if let Some(dir) = folder(layout, config, Kind::Phpmyadmin) {
        let temp = layout.tmp().join("phpmyadmin");
        std::fs::create_dir_all(&temp)?;
        let config_php = PHPMYADMIN
            .replace("{secret}", &secret(layout)?)
            .replace("{temp}", &php_string(&temp.to_string_lossy().replace('\\', "/")));
        write_if_changed(&dir.join("meow-servers.php"), &list)?;
        write_if_changed(&dir.join("config.inc.php"), &config_php)?;
    }
    Ok(())
}

fn servers_php(servers: &[Server]) -> String {
    let mut out =
        String::from("<?php\n// Written by MeowServer on every start: its database servers.\nreturn [\n");
    for s in servers {
        out.push_str(&format!(
            "    ['key' => {}, 'name' => {}, 'driver' => '{}', 'port' => {}, 'user' => '{}'],\n",
            php_string(&s.key),
            php_string(&s.name),
            if s.postgres { "pgsql" } else { "server" },
            s.port,
            s.user
        ));
    }
    out.push_str("];\n");
    out
}

/// A single-quoted PHP string literal.
fn php_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// phpMyAdmin's cookie key: 32 random bytes, made once per installation.
fn secret(layout: &Layout) -> Result<String> {
    let path = layout.data().join("phpmyadmin-secret.txt");
    if let Ok(text) = std::fs::read_to_string(&path)
        && text.trim().len() == 64
    {
        return Ok(text.trim().to_owned());
    }
    // RandomState is seeded from the operating system's random source.
    let hex: String = (0..4)
        .map(|_| format!("{:016x}", std::collections::hash_map::RandomState::new().build_hasher().finish()))
        .collect();
    std::fs::create_dir_all(layout.data())?;
    std::fs::write(&path, &hex)?;
    Ok(hex)
}

fn write_if_changed(path: &Path, text: &str) -> Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    std::fs::write(path, text)?;
    Ok(())
}

const ADMINER: &str = r#"<?php
// Written by MeowServer on every start; changes here are lost.
if (!in_array($_SERVER['REMOTE_ADDR'] ?? '', ['127.0.0.1', '::1'], true)) {
    http_response_code(403);
    exit('Adminer is only available on this computer.');
}
// ?meow=<server> logs in to one of MeowServer's servers straight away.
// Adminer then redirects to ?server=…&username=…, which keeps it.
$meow_servers = require __DIR__ . '/meow-servers.php';
$meow_auto = false;
foreach ($meow_servers as $s) {
    if (($_GET['meow'] ?? '') === $s['key'] && !isset($_GET['username']) && empty($_POST)) {
        $_POST['auth'] = [
            'driver' => $s['driver'],
            'server' => '127.0.0.1:' . $s['port'],
            'username' => $s['user'],
            'password' => '',
            'db' => '',
        ];
        $meow_auto = true;
    }
}
// Adminer keeps unknown parameters in its links, and with PostgreSQL that
// sends it round in a loop.
if (isset($_GET['meow'])) {
    unset($_GET['meow']);
    $_SERVER['REQUEST_URI'] = rtrim(preg_replace('~([?&])meow=[^&]*&?~', '$1', $_SERVER['REQUEST_URI']), '?&');
}

function adminer_object() {
    class MeowAdminer extends \Adminer\Adminer {
        // MeowServer's servers have no password, which Adminer refuses
        // by default.
        function login($login, $password) {
            return true;
        }

        function verifyLoginToken() {
            return !$GLOBALS['meow_auto'];
        }
    }
    return new MeowAdminer;
}

require __DIR__ . '/adminer.php';
"#;

const PHPMYADMIN: &str = r#"<?php
// Written by MeowServer on every start; changes here are lost.
if (!in_array($_SERVER['REMOTE_ADDR'] ?? '', ['127.0.0.1', '::1'], true)) {
    http_response_code(403);
    exit('phpMyAdmin is only available on this computer.');
}
$cfg['blowfish_secret'] = hex2bin('{secret}');
// MeowServer's servers, logged in as their user without a password.
$i = 0;
foreach (require __DIR__ . '/meow-servers.php' as $s) {
    if ($s['driver'] !== 'server') {
        continue;
    }
    $i++;
    $cfg['Servers'][$i]['verbose'] = $s['name'];
    $cfg['Servers'][$i]['host'] = '127.0.0.1';
    $cfg['Servers'][$i]['port'] = (string) $s['port'];
    $cfg['Servers'][$i]['auth_type'] = 'config';
    $cfg['Servers'][$i]['user'] = $s['user'];
    $cfg['Servers'][$i]['password'] = '';
    $cfg['Servers'][$i]['AllowNoPassword'] = true;
}
$cfg['TempDir'] = {temp};
// No calls home.
$cfg['VersionCheck'] = false;
$cfg['SendErrorReports'] = 'never';
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_strings_are_escaped() {
        assert_eq!(php_string(r"C:\a'b"), r"'C:\\a\'b'");
    }

    #[test]
    fn servers_file_lists_drivers() {
        let servers = [
            Server {
                key: "mysql".into(),
                name: "MySQL 8.4".into(),
                postgres: false,
                port: 3306,
                user: "root",
            },
            Server {
                key: "postgres".into(),
                name: "PostgreSQL 18".into(),
                postgres: true,
                port: 5432,
                user: "postgres",
            },
        ];
        let text = servers_php(&servers);
        assert!(
            text.contains("'key' => 'mysql', 'name' => 'MySQL 8.4', 'driver' => 'server', 'port' => 3306")
        );
        assert!(text.contains("'driver' => 'pgsql', 'port' => 5432, 'user' => 'postgres'"));
    }
}
