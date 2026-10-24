// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Quick create (PRD F13): a new project in `www/` from a template.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::http::{Http, to_hex};
use crate::layout::Layout;
use crate::platform::EXE_SUFFIX;
use crate::provider::Archive;
use crate::service::{self, Kind, mysql};
use crate::{download, site, terminal};

#[derive(Debug, Clone)]
pub enum Template {
    Blank,
    Laravel,
    WordPress,
    /// Any `composer create-project` package, e.g. `symfony/skeleton`.
    Composer(String),
}

impl Template {
    pub fn parse(name: &str, package: Option<&str>) -> Result<Template> {
        Ok(match name {
            "blank" | "empty" | "php" => Template::Blank,
            "laravel" => Template::Laravel,
            "wordpress" | "wp" => Template::WordPress,
            "composer" => {
                Template::Composer(package.context("give the package, e.g. symfony/skeleton")?.to_owned())
            }
            other => bail!("unknown template {other:?}; use blank, laravel, wordpress, or composer"),
        })
    }
}

/// Creates the project and returns its folder. `say` receives progress.
pub fn create(
    layout: &Layout,
    template: &Template,
    name: &str,
    say: &mut dyn FnMut(&str),
) -> Result<PathBuf> {
    let config = Config::load(layout)?;
    let slug = site::slugify(name).with_context(|| format!("{name:?} cannot be used as a site name"))?;
    let www = layout.resolve(&config.general.www);
    std::fs::create_dir_all(&www)?;
    let dir = www.join(&slug);
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }
    match template {
        Template::Blank => {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                dir.join("index.php"),
                format!(
                    "<?php\n// {slug}: created by MeowServer.\necho 'Hello from {slug} on PHP ' . PHP_VERSION;\n"
                ),
            )?;
        }
        Template::Laravel => composer_create(layout, &config, "laravel/laravel", &www, &slug, say)?,
        Template::Composer(package) => composer_create(layout, &config, package, &www, &slug, say)?,
        Template::WordPress => wordpress(layout, &config, &dir, &slug, say)?,
    }
    Ok(dir)
}

fn composer_create(
    layout: &Layout,
    config: &Config,
    package: &str,
    www: &Path,
    slug: &str,
    say: &mut dyn FnMut(&str),
) -> Result<()> {
    if service::select(layout, Kind::Composer, None).is_none() {
        say("Installing Composer…");
        let provider = crate::provider::for_service(Kind::Composer).context("Composer is not available")?;
        let http = Http::new();
        let release = provider.releases(&http)?.into_iter().next().context("no Composer release found")?;
        let mut on_asset = |_: &crate::provider::Asset| {};
        let mut on_bytes = |_: u64, _: Option<u64>| {};
        download::install(
            layout,
            &http,
            "composer",
            &release,
            download::Progress { on_asset: &mut on_asset, on_bytes: &mut on_bytes },
        )?;
    }
    let php =
        service::select(layout, Kind::Php, config.version_selector("php")).context("PHP is not installed")?;
    let composer = service::select(layout, Kind::Composer, None).context("Composer is not installed")?;
    say(&format!(
        "composer create-project {package} {slug} (this downloads dependencies and can take minutes)…"
    ));
    let log_path = layout.logs().join("create-project.log");
    let log = crate::logging::open_rotated(&log_path)?;
    let status = Command::new(php.exe("php"))
        .arg(composer.dir.join("composer.phar"))
        .args(["create-project", package, slug, "--prefer-dist", "--no-interaction", "--no-progress"])
        .current_dir(www)
        .envs(terminal::env(layout, config)?)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()
        .context("cannot run Composer")?;
    if !status.success() {
        bail!("composer create-project failed; see {}", log_path.display());
    }
    Ok(())
}

/// WordPress: the latest release, a database named after the site, and a
/// `wp-config.php` that points at it (what Laragon users expect).
fn wordpress(
    layout: &Layout,
    config: &Config,
    dir: &Path,
    slug: &str,
    say: &mut dyn FnMut(&str),
) -> Result<()> {
    say("Downloading WordPress…");
    let http = Http::new();
    let zip = layout.tmp().join("wordpress-latest.zip");
    std::fs::create_dir_all(layout.tmp())?;
    http.download("https://wordpress.org/latest.zip", &zip, &mut |_, _| {})?;
    let staging = layout.tmp().join(format!("wordpress-{slug}"));
    download::extract(&zip, &Archive::Zip, &staging)?;
    let _ = std::fs::remove_file(&zip);
    std::fs::rename(staging.join("wordpress"), dir).context("unexpected WordPress archive layout")?;
    let _ = std::fs::remove_dir_all(&staging);

    let db = slug.replace('-', "_");
    let (host, created) = match create_database(layout, config, &db) {
        Ok(host) => (host, true),
        Err(e) => {
            say(&format!("Database not created ({e:#}); create `{db}` yourself."));
            (format!("127.0.0.1:{}", config.port("mysql", mysql::DEFAULT_PORT)), false)
        }
    };
    let sample = std::fs::read_to_string(dir.join("wp-config-sample.php"))?;
    let mut wp_config = sample
        .replace("database_name_here", &db)
        .replace("username_here", "root")
        .replace("password_here", "")
        .replace("'localhost'", &format!("'{host}'"));
    for _ in 0..8 {
        wp_config = wp_config.replacen("put your unique phrase here", &salt(), 1);
    }
    std::fs::write(dir.join("wp-config.php"), wp_config)?;
    if created {
        say(&format!("Database `{db}` created."));
    }
    Ok(())
}

/// `CREATE DATABASE` on whichever of MySQL/MariaDB is active and reachable.
/// Returns `host:port` for the app's config.
fn create_database(layout: &Layout, config: &Config, name: &str) -> Result<String> {
    for kind in [Kind::Mysql, Kind::Mariadb] {
        let Some(installed) = service::select(layout, kind, config.version_selector(kind.id())) else {
            continue;
        };
        let port = config.port(kind.id(), mysql::DEFAULT_PORT);
        let client = installed.dir.join("bin").join(format!("mysql{EXE_SUFFIX}"));
        let sql = format!("CREATE DATABASE IF NOT EXISTS `{name}` CHARACTER SET utf8mb4");
        let ok = Command::new(&client)
            .args(["-u", "root", "-h", "127.0.0.1", "-P", &port.to_string(), "-e", &sql])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return Ok(format!("127.0.0.1:{port}"));
        }
    }
    bail!("no running MySQL or MariaDB accepted the connection")
}

/// A 64-character secret for WordPress keys and salts.
fn salt() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut h = Sha256::new();
    h.update(nanos.to_le_bytes());
    h.update(n.to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    to_hex(&h.finalize())
}
