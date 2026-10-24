// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! PHP over FastCGI (PRD §9.4): one pool per PHP version in use. On
//! Windows a pool is several `php-cgi` processes, one per port, because
//! there is no FPM; elsewhere it is one `php-fpm` master.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::Serialize;

use super::{Installed, Kind, write_generated};
use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{Os, os};
use crate::site::{Framework, Site};
use crate::supervisor::{ProcessSpec, Restart};
use crate::template;
use crate::version::Version;

/// FastCGI ports: 19000–19899 by default (`[services.php] port`), away from
/// Xdebug's 9003 (PRD §9.10). Each PHP line has a fixed block of ten, so
/// adding a version never moves the ports of one already running:
/// 7.4 → 19240, 8.4 → 19340. A second MeowServer folder running at the same
/// time needs another base.
pub const DEFAULT_PORT_BASE: u16 = 19000;
const PORTS_PER_VERSION: u16 = 10;

fn base_port(port_base: u16, version: &Version) -> Result<u16> {
    let line = version.line();
    let (major, minor) = line.split_once('.').unwrap_or((&line, "0"));
    let index = major.parse::<u16>()? * 10 + minor.parse::<u16>()?.min(9);
    match index.checked_sub(50) {
        Some(i) if i < 90 => Ok(port_base + i * PORTS_PER_VERSION),
        _ => bail!("PHP {version} is outside the supported range (5.0–13.9)"),
    }
}
/// Each `php-cgi` exits after this many requests and is replaced, which
/// keeps leaks in long-running workers in check.
const MAX_REQUESTS: &str = "1000";

pub struct Pool {
    pub installed: Installed,
    /// `127.0.0.1:<port>` addresses the web server balances across.
    pub upstreams: Vec<String>,
    pub specs: Vec<ProcessSpec>,
}

#[derive(Default)]
pub struct Plan {
    pub pools: Vec<Pool>,
    default_pool: Option<usize>,
    site_pool: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
}

impl Pool {
    /// The TCP ports this pool listens on.
    pub fn ports(&self) -> Vec<u16> {
        self.upstreams.iter().filter_map(|u| u.rsplit(':').next()?.parse().ok()).collect()
    }

    /// A name web server configs can use for this pool, e.g. `php-8-4-27`.
    /// Hyphens, not underscores: Apache rejects `_` in balancer names.
    pub fn name(&self) -> String {
        format!("php-{}", self.installed.version.to_string().replace('.', "-"))
    }
}

impl Plan {
    /// The pool serving a site, or `None` if it does not run PHP.
    pub fn pool_for(&self, site: &Site) -> Option<&Pool> {
        if matches!(site.framework, Framework::Proxy | Framework::Static) {
            return None;
        }
        let index = self.site_pool.get(&site.name).copied().or(self.default_pool)?;
        Some(&self.pools[index])
    }

    /// The pool used by sites that do not pin a version.
    pub fn default_pool(&self) -> Option<&Pool> {
        self.default_pool.map(|i| &self.pools[i])
    }
}

pub fn plan(layout: &Layout, config: &Config, sites: &[Site]) -> Result<Plan> {
    let mut plan = Plan::default();
    let Some(default) = super::select(layout, Kind::Php, config.version_selector("php")) else {
        match config.version_selector("php") {
            Some(sel) => bail!("PHP {sel} is not installed. Run `meow install php {sel}`."),
            None => bail!("PHP is not installed. Run `meow install php`."),
        }
    };

    let mut needed: Vec<Installed> = vec![default.clone()];
    let mut pins: BTreeMap<String, Version> = BTreeMap::new();
    for site in sites {
        let Some(selector) = &site.php else { continue };
        let line = selector.split('.').take(2).collect::<Vec<_>>().join(".");
        match super::select(layout, Kind::Php, Some(&line)) {
            // One pool per line: a site pinned to 8.4 shares the default
            // pool when that is also 8.4.
            Some(found) => match needed.iter().find(|i| i.version.line() == found.version.line()) {
                Some(existing) => {
                    pins.insert(site.name.clone(), existing.version.clone());
                }
                None => {
                    pins.insert(site.name.clone(), found.version.clone());
                    needed.push(found);
                }
            },
            None => plan.warnings.push(format!(
                "{} asks for PHP {selector}, which is not installed; using PHP {} instead",
                site.name, default.version
            )),
        }
    }
    needed.sort_by(|a, b| a.version.cmp(&b.version));

    for installed in needed {
        let base = base_port(config.port("php", DEFAULT_PORT_BASE), &installed.version)?;
        let pool = build_pool(layout, config, installed, base)?;
        plan.pools.push(pool);
    }
    plan.default_pool = plan.pools.iter().position(|p| p.installed.version == default.version);
    for (site, version) in pins {
        if let Some(index) = plan.pools.iter().position(|p| p.installed.version == version) {
            plan.site_pool.insert(site, index);
        }
    }
    Ok(plan)
}

/// Writes the generated `php.ini` for `installed` and returns its folder,
/// for `PHPRC` in terminals (PRD F29).
pub fn write_ini(layout: &Layout, config: &Config, installed: &Installed) -> Result<PathBuf> {
    let dir = layout.generated().join("php").join(installed.version.to_string());
    write_generated(&dir.join("php.ini"), &render_ini(layout, config, installed)?)?;
    Ok(dir)
}

/// `PHP_INI_SCAN_DIR`: the user's extra `.ini` folders.
pub fn scan_dirs(layout: &Layout, installed: &Installed) -> Result<OsString> {
    let custom = layout.custom().join("php");
    let custom_line = custom.join(installed.version.line());
    std::fs::create_dir_all(&custom_line)?;
    Ok(std::env::join_paths([&custom, &custom_line])?)
}

fn build_pool(layout: &Layout, config: &Config, installed: Installed, base: u16) -> Result<Pool> {
    let version = installed.version.to_string();
    let dir = layout.generated().join("php").join(&version);
    let ini = dir.join("php.ini");
    write_generated(&ini, &render_ini(layout, config, &installed)?)?;

    let env = vec![
        (OsString::from("PHP_INI_SCAN_DIR"), scan_dirs(layout, &installed)?),
        (OsString::from("PHP_FCGI_MAX_REQUESTS"), OsString::from(MAX_REQUESTS)),
    ];
    let log = layout.logs().join(format!("php-{version}.log"));
    let mut specs = Vec::new();
    let mut upstreams = Vec::new();

    if os() == Os::Windows {
        for n in 0..config.php.pool_size.clamp(1, PORTS_PER_VERSION as u8) {
            let addr = format!("127.0.0.1:{}", base + u16::from(n));
            specs.push(ProcessSpec {
                id: format!("php-{version}#{n}"),
                program: installed.exe("php-cgi"),
                args: vec!["-b".into(), addr.clone().into(), "-c".into(), ini.clone().into()],
                env: env.clone(),
                cwd: Some(installed.dir.clone()),
                log: log.clone(),
                restart: Restart::Always,
                graceful_stop: None,
            });
            upstreams.push(addr);
        }
    } else {
        let addr = format!("127.0.0.1:{base}");
        let fpm_conf = dir.join("php-fpm.conf");
        let ctx = FpmContext { version: &version, listen: &addr, error_log: &log };
        write_generated(&fpm_conf, &template::render("php/php-fpm.conf", ctx)?)?;
        specs.push(ProcessSpec {
            id: format!("php-{version}"),
            program: installed.dir.join("php-fpm"),
            args: vec![
                "--nodaemonize".into(),
                "--fpm-config".into(),
                fpm_conf.into(),
                "-c".into(),
                ini.into(),
            ],
            env,
            cwd: Some(installed.dir.clone()),
            log: log.clone(),
            restart: Restart::OnFailure,
            graceful_stop: None,
        });
        upstreams.push(addr);
    }
    Ok(Pool { installed, upstreams, specs })
}

#[derive(Serialize)]
struct FpmContext<'a> {
    version: &'a str,
    listen: &'a str,
    error_log: &'a Path,
}

#[derive(Serialize)]
struct IniContext {
    version: String,
    custom_dir: PathBuf,
    extension_dir: Option<PathBuf>,
    extensions: Vec<String>,
    opcache: bool,
    xdebug: bool,
    error_log: PathBuf,
    session_dir: PathBuf,
    tmp_dir: PathBuf,
    settings: Vec<(String, String)>,
}

/// Development-friendly defaults; anything in `[php.ini]` wins.
///
/// The OPcache and realpath settings come from measuring slow Laragon
/// setups (PRD §8.7): without OPcache every request recompiles every file,
/// and Windows file lookups are slow, so both caches are sized for large
/// Laravel/WordPress projects. `opcache.file_cache` (set in `render_ini`)
/// keeps compiled scripts on disk so the first request after a reboot does
/// not start cold.
const DEFAULT_SETTINGS: &[(&str, &str)] = &[
    ("display_errors", "On"),
    ("display_startup_errors", "On"),
    ("error_reporting", "E_ALL"),
    ("log_errors", "On"),
    ("memory_limit", "512M"),
    ("upload_max_filesize", "128M"),
    ("post_max_size", "128M"),
    ("max_execution_time", "120"),
    ("max_input_vars", "5000"),
    ("date.timezone", "UTC"),
    ("realpath_cache_size", "4096K"),
    ("realpath_cache_ttl", "600"),
    ("opcache.enable", "1"),
    ("opcache.enable_cli", "0"),
    ("opcache.memory_consumption", "256"),
    ("opcache.interned_strings_buffer", "32"),
    ("opcache.max_accelerated_files", "50000"),
    // Edits show up on the next request: timestamps are checked every time.
    ("opcache.validate_timestamps", "1"),
    ("opcache.revalidate_freq", "0"),
];

/// Downloads Xdebug into a Windows PHP build's `ext/` (PRD F20, ADR 0006),
/// checking xdebug.org's SHA-256. `index` is the download page, fetched
/// once with [`crate::provider::xdebug::index`].
pub fn install_xdebug(http: &crate::http::Http, index: &str, installed: &Installed) -> Result<String> {
    let ext = installed.dir.join("ext");
    if !ext.is_dir() {
        bail!("PHP {} has no ext/ folder; install Xdebug with your package manager", installed.version);
    }
    let (version, asset) = crate::provider::xdebug::for_php(index, &installed.version.line())?;
    let tmp = ext.join("php_xdebug.dll.download");
    let actual = http.download(&asset.url, &tmp, &mut |_, _| {})?;
    if let Some(expected) = &asset.sha256 {
        if !expected.eq_ignore_ascii_case(&actual) {
            let _ = std::fs::remove_file(&tmp);
            bail!("checksum mismatch for {}; nothing was installed", asset.url);
        }
    }
    std::fs::rename(&tmp, ext.join("php_xdebug.dll"))?;
    Ok(version.to_string())
}

pub fn has_xdebug(installed: &Installed) -> bool {
    installed.dir.join("ext").join("php_xdebug.dll").exists()
}

/// MeowServer's default for a php.ini setting, if it sets one.
pub fn default_setting(key: &str) -> Option<&'static str> {
    DEFAULT_SETTINGS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// Extensions a Windows PHP build ships as DLLs in `ext/`, by name.
pub fn available_extensions(installed: &Installed) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(installed.dir.join("ext")) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let ext = name.strip_prefix("php_")?.strip_suffix(".dll")?.to_owned();
            // OPcache is always on and managed separately.
            (ext != "opcache" && ext != "xdebug").then_some(ext)
        })
        .collect();
    names.sort();
    names
}

fn render_ini(layout: &Layout, config: &Config, installed: &Installed) -> Result<String> {
    // Windows builds load extensions from ext/; static builds have them
    // compiled in, so there is nothing to list.
    let ext_dir = installed.dir.join("ext");
    let (extension_dir, extensions, opcache, xdebug) = if ext_dir.is_dir() {
        let has = |name: &str| ext_dir.join(format!("php_{name}.dll")).exists();
        let extensions = config.php.extensions.iter().filter(|e| has(e)).cloned().collect();
        (Some(ext_dir.clone()), extensions, has("opcache"), config.php.xdebug && has("xdebug"))
    } else {
        (None, Vec::new(), false, false)
    };

    let mut settings: BTreeMap<String, String> =
        DEFAULT_SETTINGS.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    // Send mail() to Mailpit when it is installed (PRD F22).
    if let Some(mailpit) = super::select(layout, Kind::Mailpit, None) {
        let smtp = config.port("mailpit", super::mailpit::SMTP_PORT);
        if os() == Os::Windows {
            settings.insert("SMTP".into(), "127.0.0.1".into());
            settings.insert("smtp_port".into(), smtp.to_string());
            // Windows mail() refuses to send without a sender address.
            settings.insert("sendmail_from".into(), "meowserver@localhost".into());
        } else {
            let exe = mailpit.exe("mailpit").display().to_string();
            settings.insert("sendmail_path".into(), format!("\"{exe} sendmail -S 127.0.0.1:{smtp}\""));
        }
    }
    let opcache_dir = layout.data().join("php").join("opcache").join(installed.version.line());
    std::fs::create_dir_all(&opcache_dir)?;
    let opcache_path = opcache_dir.display().to_string().replace('\\', "/");
    settings.insert("opcache.file_cache".into(), format!("\"{opcache_path}\""));
    settings.extend(config.php.ini.clone());

    let session_dir = layout.data().join("php").join("sessions");
    let tmp_dir = layout.tmp().join("php");
    std::fs::create_dir_all(&session_dir)?;
    std::fs::create_dir_all(&tmp_dir)?;

    let ctx = IniContext {
        version: installed.version.to_string(),
        custom_dir: layout.custom().join("php"),
        extension_dir,
        extensions,
        opcache,
        xdebug,
        error_log: layout.logs().join(format!("php-{}-errors.log", installed.version)),
        session_dir,
        tmp_dir,
        settings: settings.into_iter().collect(),
    };
    template::render("php/php.ini", ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_are_fixed_per_line() {
        let port = |v: &str| base_port(DEFAULT_PORT_BASE, &v.parse().unwrap()).unwrap();
        assert_eq!(port("7.4.33"), 19240);
        assert_eq!(port("8.4.27"), 19340);
        assert_eq!(port("8.4.1"), 19340);
        assert_eq!(port("8.5.0"), 19350);
        assert!(port("13.9.0") < 19900);
    }
}
