// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! The folders MeowServer uses, all under one root (PRD §12).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    /// `MEOW_HOME` if set, otherwise the folder holding the running executable.
    pub fn discover() -> Result<Self> {
        if let Some(home) = std::env::var_os("MEOW_HOME") {
            return Ok(Self::at(PathBuf::from(home)));
        }
        let exe = std::env::current_exe().context("cannot locate the running executable")?;
        let dir = exe.parent().context("the executable has no parent folder")?;
        Ok(Self::at(dir.to_path_buf()))
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root }
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
