// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Adding and removing projects (PRD F8, F9, F11, F12). These only change
//! `meow.toml`; project folders are never touched.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::{discover, slugify};
use crate::config::{Config, SiteSettings};
use crate::layout::Layout;

/// Registers a folder from anywhere as a site and returns its name.
pub fn link(layout: &Layout, config: &mut Config, path: &Path, name: Option<&str>) -> Result<String> {
    let path = std::fs::canonicalize(path).with_context(|| format!("{} does not exist", path.display()))?;
    if !path.is_dir() {
        bail!("{} is not a folder", path.display());
    }
    let raw = match name {
        Some(n) => n.to_owned(),
        None => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    let slug = slugify(&raw).with_context(|| format!("{raw:?} cannot be used as a site name"))?;
    if let Some(existing) = discover(layout, config).sites.into_iter().find(|s| s.name == slug) {
        if existing.path == path {
            bail!("{} is already served as {}", path.display(), existing.host);
        }
        bail!("the name {slug} is taken by {}; choose another with --name", existing.path.display());
    }
    let settings = config.sites.entry(slug.clone()).or_default();
    settings.path = Some(clean(path));
    Ok(slug)
}

/// Removes a site added with [`link`] or a proxy site. Folders in `www/`
/// and parked folders cannot be removed this way: the folder is the site.
pub fn unlink(config: &mut Config, name: &str) -> Result<()> {
    match config.sites.get_mut(name) {
        Some(s) if s.path.is_some() || s.proxy.is_some() => {
            s.path = None;
            s.proxy = None;
            if is_default(s) {
                config.sites.remove(name);
            }
            Ok(())
        }
        _ => bail!("{name} was not added with `meow link`; remove or rename its folder instead"),
    }
}

/// Adds a folder whose subfolders all become sites.
pub fn park(config: &mut Config, dir: &Path) -> Result<PathBuf> {
    let dir = clean(std::fs::canonicalize(dir).with_context(|| format!("{} does not exist", dir.display()))?);
    if config.general.parked.contains(&dir) {
        bail!("{} is already parked", dir.display());
    }
    config.general.parked.push(dir.clone());
    Ok(dir)
}

pub fn unpark(config: &mut Config, dir: &Path) -> Result<()> {
    let target = std::fs::canonicalize(dir).map(clean).unwrap_or_else(|_| dir.to_path_buf());
    let before = config.general.parked.len();
    config.general.parked.retain(|p| p != &target && p != dir);
    if config.general.parked.len() == before {
        bail!("{} is not parked", dir.display());
    }
    Ok(())
}

/// Adds a site that forwards to a local port or URL (PRD F12).
pub fn proxy(layout: &Layout, config: &mut Config, name: &str, target: &str) -> Result<String> {
    let slug = slugify(name).with_context(|| format!("{name:?} cannot be used as a site name"))?;
    let target = if target == "auto" {
        format!("http://127.0.0.1:{}", free_dev_port(config)?)
    } else if target.chars().all(|c| c.is_ascii_digit()) {
        format!("http://127.0.0.1:{target}")
    } else if target.starts_with("http://") || target.starts_with("https://") {
        target.trim_end_matches('/').to_owned()
    } else {
        bail!("proxy target must be a port (3000) or a URL (http://127.0.0.1:3000)");
    };
    let taken = discover(layout, config).sites.into_iter().find(|s| s.name == slug && s.proxy.is_none());
    if let Some(site) = taken {
        bail!("{slug} is already a folder site ({})", site.path.display());
    }
    config.sites.entry(slug.clone()).or_default().proxy = Some(target);
    Ok(slug)
}

/// A port for a project's own dev server (Vite, Next, `artisan serve`)
/// that no other proxy site claims and nothing listens on (PRD F40).
fn free_dev_port(config: &Config) -> Result<u16> {
    let claimed: Vec<u16> = config
        .sites
        .values()
        .filter_map(|s| s.proxy.as_deref()?.rsplit(':').next()?.trim_end_matches('/').parse().ok())
        .collect();
    (3000..4000)
        .find(|p| !claimed.contains(p) && crate::platform::port_owners(*p).is_empty())
        .context("no free port between 3000 and 3999")
}

/// Settings that apply to any site, whatever its source.
pub fn settings<'a>(config: &'a mut Config, name: &str) -> &'a mut SiteSettings {
    config.sites.entry(name.to_owned()).or_default()
}

/// Drops entries that only hold defaults, keeping `meow.toml` tidy.
pub fn tidy(config: &mut Config) {
    config.sites.retain(|_, s| !is_default(s));
}

fn is_default(s: &SiteSettings) -> bool {
    s.path.is_none() && s.php.is_none() && s.root.is_none() && s.proxy.is_none() && !s.force_https
}

/// Windows `canonicalize` adds a `\\?\` prefix that other programs choke on.
fn clean(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_and_unlink_round_trip() {
        let root = std::env::temp_dir().join(format!("meow-link-{}", std::process::id()));
        let project = root.join("projects").join("My App");
        std::fs::create_dir_all(&project).unwrap();
        let layout = Layout::at(root.join("home"));
        let mut config = Config::default();

        let name = link(&layout, &mut config, &project, None).unwrap();
        assert_eq!(name, "my-app");
        assert!(link(&layout, &mut config, &project, None).is_err(), "linking twice fails");
        let sites = discover(&layout, &config).sites;
        assert_eq!(sites.len(), 1);
        assert!(!sites[0].path.to_string_lossy().starts_with(r"\\?\"));

        unlink(&mut config, "my-app").unwrap();
        assert!(config.sites.is_empty());
        assert!(project.exists(), "the folder is never touched");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn proxy_accepts_port_or_url() {
        let layout = Layout::at(std::env::temp_dir().join("meow-proxy-none"));
        let mut config = Config::default();
        proxy(&layout, &mut config, "front", "3000").unwrap();
        assert_eq!(config.sites["front"].proxy.as_deref(), Some("http://127.0.0.1:3000"));
        assert!(proxy(&layout, &mut config, "x", "localhost:3000").is_err());
    }
}
