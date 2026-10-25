// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! PHP extensions that are not part of PHP's own build (imagick, redis,
//! apcu, mongodb, …), from the PECL builds php.net publishes for Windows.
//! php.net lists no checksums for these; they come only from
//! downloads.php.net over HTTPS. macOS and Linux PHP builds have their
//! extensions compiled in, so nothing can be added to them this way.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};

use crate::download::extract;
use crate::http::Http;
use crate::layout::Layout;
use crate::platform::{Os, os};
use crate::provider::Archive;
use crate::service::Installed;
use crate::version::Version;

const BASE: &str = "https://downloads.php.net/~windows/pecl/releases/";

/// Offered before anyone searches: what projects ask for most.
pub const POPULAR: [&str; 14] = [
    "apcu",
    "imagick",
    "redis",
    "igbinary",
    "mongodb",
    "memcache",
    "sqlsrv",
    "pdo_sqlsrv",
    "yaml",
    "pcov",
    "ssh2",
    "mailparse",
    "msgpack",
    "xlswriter",
];

/// The names php.net builds for Windows, cached for a day.
pub fn index(layout: &Layout, http: &Http) -> Result<Vec<String>> {
    let cache = layout.tmp().join("pecl-index.txt");
    let fresh = std::fs::metadata(&cache).and_then(|m| m.modified()).is_ok_and(|t| {
        SystemTime::now().duration_since(t).is_ok_and(|age| age < Duration::from_secs(86_400))
    });
    if fresh && let Ok(text) = std::fs::read_to_string(&cache) {
        return Ok(text.lines().map(str::to_owned).collect());
    }
    let listing = http.get_text(BASE)?;
    let mut names: Vec<String> = folders(&listing)
        .into_iter()
        .filter(|n| n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .collect();
    names.sort();
    names.dedup();
    let _ = std::fs::create_dir_all(layout.tmp());
    let _ = std::fs::write(&cache, names.join("\n"));
    Ok(names)
}

/// Sub-folder names in an Apache directory listing (`href="3.8.1/"`).
fn folders(listing: &str) -> Vec<String> {
    listing
        .split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|href| href.ends_with('/') && !href.starts_with('/') && !href.starts_with('?'))
        .map(|href| href.trim_end_matches('/').to_owned())
        .collect()
}

/// Downloads `name` for this PHP version and puts it in place: the
/// extension DLLs in `ext\`, the libraries they load next to `php.exe`
/// (never replacing PHP's own). Returns the extensions it added, e.g.
/// `["imagick"]`, and the PECL version.
pub fn install(
    layout: &Layout,
    http: &Http,
    php: &Installed,
    name: &str,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(Vec<String>, String)> {
    if os() != Os::Windows {
        bail!(
            "PECL extensions can be added to Windows PHP builds only; macOS and Linux builds have theirs built in"
        );
    }
    let line = php.version.line();
    let (version, file) = find_build(http, name, &line)?
        .with_context(|| format!("php.net has no Windows build of {name} for PHP {line}"))?;

    let work = layout.tmp().join(format!("pecl-{name}"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let zip = work.join(&file);
    http.download(&format!("{BASE}{name}/{version}/{file}"), &zip, progress)?;
    let unpacked = work.join("files");
    extract(&zip, &Archive::Zip, &unpacked)?;

    let ext_dir = php.dir.join("ext");
    let mut added = Vec::new();
    for dll in dlls(&unpacked) {
        let file_name = dll.file_name().unwrap().to_string_lossy().into_owned();
        if let Some(ext) = file_name.strip_prefix("php_").and_then(|f| f.strip_suffix(".dll")) {
            std::fs::copy(&dll, ext_dir.join(&file_name))?;
            added.push(ext.to_owned());
        } else {
            // A library the extension loads (ImageMagick's CORE_RL_*.dll,
            // libssh2.dll): next to php-cgi.exe, where Windows finds it.
            let dest = php.dir.join(&file_name);
            if !dest.exists() {
                std::fs::copy(&dll, dest)?;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    if added.is_empty() {
        bail!("the {name} download held no php_*.dll");
    }
    Ok((added, version))
}

fn dlls(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(dlls(&path));
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dll")) {
            out.push(path);
        }
    }
    out
}

/// The newest stable release of `name` that has a 64-bit, non-thread-safe
/// build for PHP `line` (what MeowServer runs), as (version, file name).
fn find_build(http: &Http, name: &str, line: &str) -> Result<Option<(String, String)>> {
    let listing = http.get_text(&format!("{BASE}{name}/"))?;
    let mut versions: Vec<(Version, String)> = folders(&listing)
        .into_iter()
        // Stable releases only: `3.8.0RC2` and `2.0.0beta1` do not parse.
        .filter_map(|v| Some((v.parse::<Version>().ok()?, v)))
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    // Newer releases may have dropped this PHP line; older may not have it yet.
    for (_, version) in versions.into_iter().take(10) {
        let files = http.get_text(&format!("{BASE}{name}/{version}/"))?;
        let prefix = format!("php_{name}-{version}-{line}-nts-");
        let build = files
            .split("href=\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .find(|f| f.starts_with(&prefix) && f.ends_with("-x64.zip"));
        if let Some(file) = build {
            return Ok(Some((version, file.to_owned())));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_directory_listings() {
        let listing = r#"<a href="?C=N;O=D">Name</a><a href="/~windows/pecl/">Parent</a>
            <a href="3.8.0RC2/">3.8.0RC2/</a><a href="3.8.1/">3.8.1/</a><a href="logs/">logs/</a>
            <a href="php_imagick-3.8.1-8.4-nts-vs17-x64.zip">zip</a>"#;
        assert_eq!(folders(listing), ["3.8.0RC2", "3.8.1", "logs"]);
    }
}
