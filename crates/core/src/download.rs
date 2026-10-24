// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Download, verify, and extract a release into `bin/<service>/<version>/`
//! (PRD F3). Nothing is left behind when a step fails.

use std::fs::File;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::http::Http;
use crate::layout::Layout;
use crate::provider::{Archive, Asset, Release};

/// Written next to an installed version; records where it came from.
pub const RECEIPT: &str = ".meow-install.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub version: String,
    pub sources: Vec<ReceiptSource>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReceiptSource {
    pub url: String,
    pub sha256: String,
    /// False when upstream publishes no checksum (PRD §9.5).
    pub verified: bool,
}

pub struct Progress<'a> {
    pub on_asset: &'a mut dyn FnMut(&Asset),
    pub on_bytes: &'a mut dyn FnMut(u64, Option<u64>),
}

/// Installs `release` for `service` and returns the installed folder.
pub fn install(
    layout: &Layout,
    http: &Http,
    service: &str,
    release: &Release,
    progress: Progress,
) -> Result<PathBuf> {
    let dest = layout.service_bin(service).join(release.version.to_string());
    if dest.exists() {
        bail!("{service} {} is already installed", release.version);
    }
    let staging = layout.tmp().join(format!("install-{service}-{}", release.version));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    let result = install_into(layout, http, release, &staging, progress);
    let sources = match result {
        Ok(sources) => sources,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };

    let content = staging.join("content");
    let receipt = Receipt { version: release.version.to_string(), sources };
    std::fs::write(content.join(RECEIPT), serde_json::to_vec_pretty(&receipt)?)?;
    std::fs::create_dir_all(dest.parent().unwrap())?;
    std::fs::rename(&content, &dest).with_context(|| format!("cannot move into {}", dest.display()))?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(dest)
}

fn install_into(
    layout: &Layout,
    http: &Http,
    release: &Release,
    staging: &Path,
    progress: Progress,
) -> Result<Vec<ReceiptSource>> {
    let content = staging.join("content");
    std::fs::create_dir_all(&content)?;
    let mut sources = Vec::new();
    for asset in &release.assets {
        (progress.on_asset)(asset);
        let expected = match (&asset.sha256, &asset.sha256_url) {
            (Some(h), _) => Some(h.clone()),
            (None, Some(url)) => Some(crate::provider::fetch_checksum(http, url, asset.file_name())?),
            (None, None) => None,
        };
        let file = layout.tmp().join(file_name(&asset.url));
        let sha256 = http.download(&asset.url, &file, progress.on_bytes);
        let sha256 = match sha256 {
            Ok(h) => h,
            Err(e) => {
                let _ = std::fs::remove_file(&file);
                return Err(e);
            }
        };
        let verified = match &expected {
            Some(expected) if !expected.eq_ignore_ascii_case(&sha256) => {
                let _ = std::fs::remove_file(&file);
                bail!(
                    "checksum mismatch for {}: expected {expected}, got {sha256}. Nothing was installed.",
                    asset.url
                );
            }
            Some(_) => true,
            None => false,
        };
        let extracted = extract(&file, &asset.archive, &staging.join("extract"));
        let _ = std::fs::remove_file(&file);
        extracted?;
        merge_stripped(&staging.join("extract"), &content, asset.root.as_deref())?;
        sources.push(ReceiptSource { url: asset.url.clone(), sha256, verified });
    }
    Ok(sources)
}

fn file_name(url: &str) -> String {
    let name = url.rsplit('/').next().unwrap_or("download");
    let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || "._-".contains(*c)).collect();
    if name.is_empty() { "download".into() } else { name }
}

/// Extracts `archive` into a fresh `dest`, refusing entries that would land
/// outside it.
pub fn extract(archive: &Path, kind: &Archive, dest: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest)?;
    match kind {
        Archive::Zip => extract_zip(archive, dest),
        Archive::TarGz => extract_tar_gz(archive, dest),
        Archive::File { name } => {
            std::fs::copy(archive, dest.join(name))?;
            Ok(())
        }
    }
    .with_context(|| format!("cannot extract {}", archive.display()))
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            bail!("archive entry {:?} points outside the target folder", entry.name());
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&out)?;
        std::io::copy(&mut entry, &mut file)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let gz = flate2::read::GzDecoder::new(File::open(archive)?);
    let mut tar = tar::Archive::new(gz);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
            bail!("archive entry {} points outside the target folder", path.display());
        }
        entry.unpack_in(dest)?;
    }
    Ok(())
}

/// Moves the extracted files into `content`. If the archive wrapped
/// everything in one top-level folder (e.g. `nginx-1.31.8/`), that folder
/// is stripped.
fn merge_stripped(extracted: &Path, content: &Path, root: Option<&str>) -> Result<()> {
    let entries: Vec<_> = std::fs::read_dir(extracted)?.collect::<Result<_, _>>()?;
    let root = match (root, entries.as_slice()) {
        (Some(root), _) => {
            let dir = extracted.join(root);
            if !dir.is_dir() {
                bail!("the archive has no {root} folder");
            }
            dir
        }
        (None, [only]) if only.file_type()?.is_dir() => only.path(),
        _ => extracted.to_path_buf(),
    };
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let target = content.join(entry.file_name());
        if target.exists() {
            bail!("{} is provided by two downloads", target.display());
        }
        std::fs::rename(entry.path(), target)?;
    }
    std::fs::remove_dir_all(extracted)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_is_sanitized() {
        assert_eq!(file_name("https://x/y/php-8.4.1-Win32-vs17-x64.zip"), "php-8.4.1-Win32-vs17-x64.zip");
        assert_eq!(file_name("https://x/y/a b?c"), "abc");
    }

    #[test]
    fn strips_single_top_level_folder() {
        let tmp = std::env::temp_dir().join(format!("meow-test-strip-{}", std::process::id()));
        let extracted = tmp.join("extract");
        let content = tmp.join("content");
        std::fs::create_dir_all(extracted.join("nginx-1.0").join("conf")).unwrap();
        std::fs::write(extracted.join("nginx-1.0").join("nginx.exe"), b"").unwrap();
        std::fs::create_dir_all(&content).unwrap();
        merge_stripped(&extracted, &content, None).unwrap();
        assert!(content.join("nginx.exe").exists());
        assert!(content.join("conf").is_dir());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
