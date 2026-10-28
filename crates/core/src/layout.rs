// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! The folders MeowServer uses, all under one root (PRD §12).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
    packaged: bool,
}

impl Layout {
    /// `MEOW_HOME` if set; otherwise the folder holding the running
    /// executable, unless that is a package's read-only program folder (PRD
    /// §12): a macOS app bundle, whose signature covers it, uses
    /// `~/Library/Application Support/MeowServer`; an AppImage or a `.deb`
    /// (`/usr/lib/meowserver`) uses `$XDG_DATA_HOME/meowserver`.
    pub fn discover() -> Result<Self> {
        if let Some(home) = std::env::var_os("MEOW_HOME") {
            return Ok(Self::at(PathBuf::from(home)));
        }
        let exe = std::env::current_exe().context("cannot locate the running executable")?;
        let dir = exe.parent().context("the executable has no parent folder")?;
        if in_app_bundle(dir) {
            let home = std::env::var_os("HOME").context("HOME is not set")?;
            let root = Path::new(&home).join("Library").join("Application Support").join("MeowServer");
            return Ok(Self { root, packaged: true });
        }
        if linux_package(dir, std::env::var_os("APPIMAGE").is_some()) {
            let data = match std::env::var_os("XDG_DATA_HOME").filter(|d| Path::new(d).is_absolute()) {
                Some(data) => PathBuf::from(data),
                None => Path::new(&std::env::var_os("HOME").context("HOME is not set")?).join(".local/share"),
            };
            return Ok(Self { root: data.join("meowserver"), packaged: true });
        }
        Ok(Self::at(dir.to_path_buf()))
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root, packaged: false }
    }

    /// Projects folder for a new `meow.toml`: `www/` in a portable folder;
    /// for a package, whose root is hidden, `~/Sites` on macOS and `~/www`
    /// on Linux.
    pub fn default_www(&self) -> PathBuf {
        let folder = if cfg!(target_os = "macos") { "Sites" } else { "www" };
        match std::env::var_os("HOME") {
            Some(home) if self.packaged => Path::new(&home).join(folder),
            _ => PathBuf::from("www"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("meow.toml")
    }

    /// `bin/<service>/`, holding one folder per installed version.
    pub fn service_bin(&self, service: &str) -> PathBuf {
        self.root.join("bin").join(service)
    }

    /// Rewritten on every start; users must not edit it.
    pub fn generated(&self) -> PathBuf {
        self.root.join("etc").join("generated")
    }

    /// User overrides, never touched by MeowServer.
    pub fn custom(&self) -> PathBuf {
        self.root.join("etc").join("custom")
    }

    pub fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn ssl(&self) -> PathBuf {
        self.root.join("ssl")
    }

    /// Scratch space for downloads and staging; safe to delete when idle.
    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// Resolves a path from `meow.toml`: relative paths are relative to the root.
    pub fn resolve(&self, path: &Path) -> PathBuf {
        if path.is_absolute() { path.to_path_buf() } else { self.root.join(path) }
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in
            [self.root.join("bin"), self.generated(), self.custom(), self.data(), self.logs(), self.tmp()]
        {
            std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        Ok(())
    }
}

/// `…/Name.app/Contents/MacOS`.
fn in_app_bundle(dir: &Path) -> bool {
    cfg!(target_os = "macos")
        && dir.file_name().is_some_and(|n| n == "MacOS")
        && dir.parent().is_some_and(|contents| {
            contents.file_name().is_some_and(|n| n == "Contents")
                && contents.parent().and_then(Path::extension).is_some_and(|e| e == "app")
        })
}

/// Run from an AppImage (`APPIMAGE` is set by its runtime) or installed
/// by the `.deb` under `/usr`.
fn linux_package(dir: &Path, appimage: bool) -> bool {
    cfg!(target_os = "linux") && (appimage || dir.starts_with("/usr"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_bundle_is_recognised_on_macos_only() {
        let inside = Path::new("/Applications/MeowServer.app/Contents/MacOS");
        assert_eq!(in_app_bundle(inside), cfg!(target_os = "macos"));
        assert!(!in_app_bundle(Path::new("/Users/me/MeowServer")));
        assert!(!in_app_bundle(Path::new("/Users/me/Contents/MacOS")));
    }

    #[test]
    fn linux_package_is_recognised_on_linux_only() {
        let linux = cfg!(target_os = "linux");
        assert_eq!(linux_package(Path::new("/usr/lib/meowserver"), false), linux);
        assert_eq!(linux_package(Path::new("/tmp/.mount_MeowSeAbc/usr/bin"), true), linux);
        assert!(!linux_package(Path::new("/home/me/MeowServer"), false));
    }
}
