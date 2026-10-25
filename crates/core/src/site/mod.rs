// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Sites from `www/`, parked folders, and projects added from anywhere
//! (PRD F6–F12). Project folders are only read, never modified.

mod detect;
pub mod manage;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use detect::Framework;

use crate::config::Config;
use crate::layout::Layout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub name: String,
    pub host: String,
    pub source: Source,
    /// The project folder.
    pub path: PathBuf,
    /// Where the web server serves files from.
    pub docroot: PathBuf,
    pub framework: Framework,
    /// PHP version selector pinned for this site, if any (PRD F11).
    pub php: Option<String>,
    pub proxy: Option<String>,
    pub force_https: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "dir", rename_all = "snake_case")]
pub enum Source {
    Www,
    Parked(PathBuf),
    Added,
}

#[derive(Debug, Default, Serialize)]
pub struct Discovery {
    pub sites: Vec<Site>,
    /// Problems the user should see, e.g. two folders mapping to one name.
    pub warnings: Vec<String>,
}

/// All sites, sorted by name. Precedence on name clashes: added projects,
/// then `www/`, then parked folders in the order configured.
pub fn discover(layout: &Layout, config: &Config) -> Discovery {
    let mut out = Discovery::default();
    let suffix = config.general.suffix.as_str();

    for (name, settings) in &config.sites {
        let Some(slug) = slugify(name) else {
            out.warnings.push(format!("site name {name:?} is not usable as a host name"));
            continue;
        };
        if let Some(proxy) = &settings.proxy {
            out.sites.push(Site {
                host: format!("{slug}.{suffix}"),
                name: slug,
                source: Source::Added,
                path: settings.path.clone().unwrap_or_default(),
                docroot: PathBuf::new(),
                framework: Framework::Proxy,
                php: None,
                proxy: Some(proxy.clone()),
                force_https: settings.force_https,
            });
        } else if let Some(path) = &settings.path {
            if !path.is_dir() {
                out.warnings.push(format!("{name}: folder {} does not exist", path.display()));
                continue;
            }
            push_folder(&mut out, config, &slug, path, Source::Added);
        }
    }

    let www = layout.resolve(&config.general.www);
    scan(&mut out, config, &www, Source::Www);
    for parked in &config.general.parked {
        scan(&mut out, config, parked, Source::Parked(parked.clone()));
    }

    out.sites.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn scan(out: &mut Discovery, config: &Config, dir: &Path, source: Source) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        if source != Source::Www {
            out.warnings.push(format!("cannot read folder {}", dir.display()));
        }
        return;
    };
    // `path.is_dir()` follows links: Laragon users keep projects on another
    // drive and link them into www (junctions and symbolic links alike).
    let mut folders = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
            continue;
        }
        if path.is_dir() {
            folders.push(path);
        } else if entry.file_type().is_ok_and(|t| t.is_symlink()) {
            let target = std::fs::read_link(&path).map(|t| t.display().to_string()).unwrap_or_default();
            out.warnings.push(format!("{} links to {target}, which does not exist", path.display()));
        }
    }
    folders.sort();
    for folder in folders {
        let raw = folder.file_name().unwrap().to_string_lossy().into_owned();
        let Some(slug) = slugify(&raw) else { continue };
        push_folder(out, config, &slug, &folder, source.clone());
    }
}

fn push_folder(out: &mut Discovery, config: &Config, slug: &str, path: &Path, source: Source) {
    if let Some(existing) = out.sites.iter().find(|s| s.name == slug) {
        out.warnings.push(format!(
            "{} and {} both map to {slug}; using the first",
            existing.path.display(),
            path.display()
        ));
        return;
    }
    let settings = config.sites.get(slug).cloned().unwrap_or_default();
    let (framework, detected_root) = detect::detect(path);
    let docroot = match &settings.root {
        Some(root) => path.join(root),
        None => detected_root,
    };
    out.sites.push(Site {
        name: slug.to_owned(),
        host: format!("{slug}.{}", config.general.suffix.as_str()),
        source,
        path: path.to_path_buf(),
        docroot,
        framework,
        php: settings.php,
        proxy: None,
        force_https: settings.force_https,
    });
}

/// Turns a folder name into a host label: lowercase ASCII letters, digits,
/// and single hyphens (`My Project_v2` → `my-project-v2`).
pub fn slugify(name: &str) -> Option<String> {
    let mut slug = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() || slug.len() > 63 { None } else { Some(slug.to_owned()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_makes_host_labels() {
        assert_eq!(slugify("My Project_v2").as_deref(), Some("my-project-v2"));
        assert_eq!(slugify("asis").as_deref(), Some("asis"));
        assert_eq!(slugify("--a..b--").as_deref(), Some("a-b"));
        assert_eq!(slugify("___"), None);
    }

    /// A junction on Windows (what Laragon users make; no admin needed),
    /// a symbolic link elsewhere.
    fn link(target: &Path, link: &Path) {
        #[cfg(windows)]
        {
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    #[test]
    fn linked_folders_in_www_are_sites() {
        let root = std::env::temp_dir().join(format!("meow-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(root.join("www").join("plain")).unwrap();
        std::fs::create_dir_all(elsewhere.join("shop").join("public")).unwrap();
        std::fs::create_dir_all(elsewhere.join("gone")).unwrap();
        std::fs::write(elsewhere.join("shop").join("artisan"), "").unwrap();
        std::fs::write(elsewhere.join("shop").join("public").join("index.php"), "").unwrap();
        link(&elsewhere.join("shop"), &root.join("www").join("shop"));
        link(&elsewhere.join("gone"), &root.join("www").join("gone"));
        std::fs::remove_dir(elsewhere.join("gone")).unwrap();

        let layout = Layout::at(root.clone());
        let found = discover(&layout, &Config::default());
        let names: Vec<&str> = found.sites.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["plain", "shop"]);
        assert!(found.sites[1].docroot.ends_with("public"), "detection looks through the link");
        assert!(found.warnings.iter().any(|w| w.contains("does not exist")), "{:?}", found.warnings);
        std::fs::remove_dir_all(root).unwrap();
    }
}
