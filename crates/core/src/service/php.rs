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

/// FastCGI ports: 19000–19899, away from Xdebug's 9003 (PRD §9.10).
const PORT_BASE: u16 = 19000;
const PORTS_PER_VERSION: u16 = 20;
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

impl Plan {
    /// FastCGI upstreams for a site, or `None` if it does not run PHP.
    pub fn upstreams(&self, site: &Site) -> Option<&[String]> {
        if matches!(site.framework, Framework::Proxy | Framework::Static) {
            return None;
        }
        let index = self.site_pool.get(&site.name).copied().or(self.default_pool)?;
        Some(&self.pools[index].upstreams)
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
        match super::select(layout, Kind::Php, Some(selector)) {
            Some(found) => {
                pins.insert(site.name.clone(), found.version.clone());
                if !needed.iter().any(|i| i.version == found.version) {
                    needed.push(found);
                }
            }
            None => plan.warnings.push(format!(
                "{} asks for PHP {selector}, which is not installed; using PHP {} instead",
                site.name, default.version
            )),
        }
    }
    needed.sort_by(|a, b| a.version.cmp(&b.version));

    for (i, installed) in needed.into_iter().enumerate() {
        let base = PORT_BASE + i as u16 * PORTS_PER_VERSION;
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

fn build_pool(layout: &Layout, config: &Config, installed: Installed, base: u16) -> Result<Pool> {
    let version = installed.version.to_string();
    let dir = layout.generated().join("php").join(&version);
    let ini = dir.join("php.ini");
    write_generated(&ini, &render_ini(layout, config, &installed)?)?;

    let custom = layout.custom().join("php");
    let custom_line = custom.join(installed.version.line());
    std::fs::create_dir_all(&custom_line)?;
    let scan_dirs = std::env::join_paths([&custom, &custom_line])?;
    let env = vec![
        (OsString::from("PHP_INI_SCAN_DIR"), scan_dirs),
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
    error_log: PathBuf,
    session_dir: PathBuf,
    tmp_dir: PathBuf,
    settings: Vec<(String, String)>,
}

/// Development-friendly defaults; anything in `[php.ini]` wins.
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
    ("opcache.enable", "1"),
    ("opcache.enable_cli", "0"),
    ("opcache.validate_timestamps", "1"),
    ("opcache.revalidate_freq", "0"),
];

fn render_ini(layout: &Layout, config: &Config, installed: &Installed) -> Result<String> {
    // Windows builds load extensions from ext/; static builds have them
    // compiled in, so there is nothing to list.
    let ext_dir = installed.dir.join("ext");
    let (extension_dir, extensions, opcache) = if ext_dir.is_dir() {
        let has = |name: &str| ext_dir.join(format!("php_{name}.dll")).exists();
        let extensions = config.php.extensions.iter().filter(|e| has(e)).cloned().collect();
        (Some(ext_dir.clone()), extensions, has("opcache"))
    } else {
        (None, Vec::new(), false)
    };

    let mut settings: BTreeMap<String, String> =
        DEFAULT_SETTINGS.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
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
        error_log: layout.logs().join(format!("php-{}-errors.log", installed.version)),
        session_dir,
        tmp_dir,
        settings: settings.into_iter().collect(),
    };
    template::render("php/php.ini", ctx)
}
