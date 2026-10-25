// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Sites from `www/`, parked folders, and projects added from anywhere
//! (PRD F6–F12). Project folders are only read, never modified.

mod detect;
pub mod manage;
pub mod needs;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use detect::Framework;

use crate::config::Config;
use crate::layout::Layout;
use crate::service::Kind;

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
    /// What the project's own files say it needs (PRD F42).
    #[serde(default)]
    pub needs: needs::Needs,
    /// `php` was chosen from `composer.json`, not set by the user.
    #[serde(default)]
    pub php_auto: bool,
    /// A project that runs its own server (PRD F43): the runtime line it
    /// uses (`None` when no installed version fits) and the port MeowServer
    /// gives it; `proxy` then points at that port.
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub app_port: Option<u16>,
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
    let php = PhpChoice::new(layout, config);

    for (name, settings) in &config.sites {
        let Some(slug) = slugify(name) else {
            out.warnings.push(format!("site name {name:?} is not usable as a host name"));
            continue;
        };
        if let Some(proxy) = &settings.proxy {
            out.sites.push(Site {
                host: format!("{slug}.{}", settings.suffix.unwrap_or(config.general.suffix).as_str()),
                name: slug,
                source: Source::Added,
                path: settings.path.clone().unwrap_or_default(),
                docroot: PathBuf::new(),
                framework: Framework::Proxy,
                php: None,
                proxy: Some(proxy.clone()),
                force_https: settings.force_https,
                needs: needs::Needs::default(),
                php_auto: false,
                runtime: None,
                app_port: None,
            });
        } else if let Some(path) = &settings.path {
            if !path.is_dir() {
                out.warnings.push(format!("{name}: folder {} does not exist", path.display()));
                continue;
            }
            push_folder(&mut out, config, &php, &slug, path, Source::Added);
        }
    }

    let www = layout.resolve(&config.general.www);
    scan(&mut out, config, &php, &www, Source::Www);
    for parked in &config.general.parked {
        scan(&mut out, config, &php, parked, Source::Parked(parked.clone()));
    }

    out.sites.sort_by(|a, b| a.name.cmp(&b.name));
    give_app_ports(config, &mut out.sites);
    out
}

/// Every project that runs its own server gets a port for it, and its
/// name forwards there (PRD F43). A port saved in `sites.<name>.port`
/// (written the first time the server starts) stays; the others are
/// handed out from 3000 in name order, skipping ports in use.
fn give_app_ports(config: &Config, sites: &mut [Site]) {
    let mut taken: Vec<u16> = config.sites.values().filter_map(|s| s.port).collect();
    taken.extend(
        config.sites.values().filter_map(|s| s.proxy.as_deref()?.rsplit(':').next()?.parse::<u16>().ok()),
    );
    let mut next = 3000u16;
    for site in sites.iter_mut().filter(|s| s.framework.runtime().is_some()) {
        let port = match site.app_port {
            Some(p) => p,
            None => {
                while taken.contains(&next) {
                    next += 1;
                }
                taken.push(next);
                next
            }
        };
        site.app_port = Some(port);
        site.proxy = Some(format!("http://127.0.0.1:{port}"));
    }
}

/// The PHP lines a site can be given automatically.
struct PhpChoice {
    layout: Layout,
    default: Option<String>,
    /// Installed lines, newest first.
    installed: Vec<String>,
}

impl PhpChoice {
    fn new(layout: &Layout, config: &Config) -> Self {
        let mut installed: Vec<String> =
            crate::service::installed(layout, Kind::Php).iter().map(|i| i.version.line()).collect();
        installed.sort_by_key(|l| std::cmp::Reverse(l.parse::<crate::version::Version>().ok()));
        installed.dedup();
        let default = crate::service::select(layout, Kind::Php, config.version_selector("php"))
            .map(|i| i.version.line());
        Self { layout: layout.clone(), default, installed }
    }

    /// For a site without its own setting: the newest installed line the
    /// project allows when the default does not fit it.
    fn pick(&self, needs: &needs::Needs) -> Option<String> {
        if needs.php.is_empty() || self.default.as_ref().is_some_and(|d| needs.php.contains(d)) {
            return None;
        }
        self.installed.iter().find(|l| needs.php.contains(l)).cloned()
    }
}

fn scan(out: &mut Discovery, config: &Config, php: &PhpChoice, dir: &Path, source: Source) {
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
        push_folder(out, config, php, &slug, &folder, source.clone());
    }
}

fn push_folder(
    out: &mut Discovery,
    config: &Config,
    php: &PhpChoice,
    slug: &str,
    path: &Path,
    source: Source,
) {
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
    let needs = needs::read(path, framework);
    let auto = if settings.php.is_none() && framework.runtime().is_none() { php.pick(&needs) } else { None };
    let runtime = needs.runtime.as_ref().and_then(|r| needs::pick_runtime(&php.layout, r));
    out.sites.push(Site {
        name: slug.to_owned(),
        host: format!("{slug}.{}", settings.suffix.unwrap_or(config.general.suffix).as_str()),
        source,
        path: path.to_path_buf(),
        docroot,
        framework,
        php_auto: auto.is_some(),
        php: settings.php.or(auto),
        proxy: None,
        force_https: settings.force_https,
        needs,
        runtime,
        app_port: settings.port,
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
