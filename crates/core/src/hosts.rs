// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Hosts file entries for every site (PRD F16). MeowServer owns one marked
//! block and never touches other lines. Writing needs administrator rights,
//! so the app asks the OS to run `meow privileged hosts-sync` elevated.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::Config;
use crate::layout::Layout;
use crate::site;

const BEGIN: &str = "# BEGIN MeowServer (managed automatically; changes inside this block are replaced)";
const END: &str = "# END MeowServer";

pub fn path() -> PathBuf {
    if let Some(p) = std::env::var_os("MEOW_HOSTS_FILE") {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join("drivers").join("etc").join("hosts")
    } else {
        PathBuf::from("/etc/hosts")
    }
}

/// Host names that should resolve to loopback.
pub fn wanted(layout: &Layout, config: &Config) -> Vec<String> {
    let mut hosts: Vec<String> = site::discover(layout, config).sites.into_iter().map(|s| s.host).collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

/// Only local development names may ever be written (PRD §8.3).
pub fn is_valid(host: &str) -> bool {
    let local = host.ends_with(".localhost") || host.ends_with(".test");
    local
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// `current` with MeowServer's block replaced by entries for `hosts`, or
/// removed when `hosts` is empty. Other lines and line endings are kept.
pub fn render(current: &str, hosts: &[String]) -> String {
    let newline = if current.contains("\r\n") { "\r\n" } else { "\n" };
    let mut kept: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in current.lines() {
        match line.trim() {
            l if l == BEGIN || l.starts_with("# BEGIN MeowServer") => inside = true,
            l if l == END => inside = false,
            _ if inside => {}
            _ => kept.push(line),
        }
    }
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    let mut out = kept.join(newline);
    if !hosts.is_empty() {
        if !out.is_empty() {
            out.push_str(newline);
            out.push_str(newline);
        }
        out.push_str(BEGIN);
        out.push_str(newline);
        for host in hosts {
            out.push_str(&format!("127.0.0.1 {host}{newline}::1 {host}{newline}"));
        }
        out.push_str(END);
    }
    out.push_str(newline);
    out
}

pub fn is_current(layout: &Layout, config: &Config) -> bool {
    let current = std::fs::read_to_string(path()).unwrap_or_default();
    render(&current, &wanted(layout, config)) == current
}

/// Writes the block. Must run with rights to change the hosts file.
pub fn write(hosts: &[String]) -> Result<()> {
    if let Some(bad) = hosts.iter().find(|h| !is_valid(h)) {
        bail!("refusing to write {bad:?} to the hosts file");
    }
    let file = path();
    let current =
        std::fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display()))?;
    let next = render(&current, hosts);
    if next == current {
        return Ok(());
    }
    let backup = file.with_file_name("hosts.meowserver.bak");
    if !backup.exists() {
        let _ = std::fs::write(&backup, &current);
    }
    std::fs::write(&file, next).with_context(|| format!("cannot write {}", file.display()))
}

pub enum Sync {
    UpToDate,
    Written,
    /// The user declined the elevation prompt; sites still work in browsers.
    Declined,
}

/// Brings the hosts file up to date, asking for elevation only if needed.
/// `meow` is the CLI executable used for the elevated step.
pub fn sync(layout: &Layout, config: &Config, meow: &Path) -> Result<Sync> {
    let hosts = if config.general.manage_hosts { wanted(layout, config) } else { Vec::new() };
    let current = std::fs::read_to_string(path()).unwrap_or_default();
    if render(&current, &hosts) == current {
        return Ok(Sync::UpToDate);
    }
    if write(&hosts).is_ok() {
        return Ok(Sync::Written);
    }
    let args = vec![
        "privileged".to_owned(),
        "hosts-sync".to_owned(),
        "--home".to_owned(),
        layout.root().display().to_string(),
    ];
    match crate::platform::run_elevated(meow, &args) {
        Ok(0) => Ok(Sync::Written),
        Ok(code) => bail!("updating the hosts file failed (exit code {code})"),
        Err(_) => Ok(Sync::Declined),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_replaces_and_removes_block() {
        let original = "127.0.0.1 localhost\r\n# my entry\r\n10.0.0.5 nas\r\n";
        let with = render(original, &["blog.localhost".into()]);
        assert!(
            with.starts_with("127.0.0.1 localhost\r\n# my entry\r\n10.0.0.5 nas\r\n\r\n# BEGIN MeowServer")
        );
        assert!(with.contains("127.0.0.1 blog.localhost\r\n::1 blog.localhost\r\n# END MeowServer\r\n"));

        let replaced = render(&with, &["shop.localhost".into()]);
        assert!(!replaced.contains("blog.localhost"));
        assert!(replaced.contains("shop.localhost"));
        assert_eq!(render(&replaced, &["shop.localhost".into()]), replaced, "stable");

        assert_eq!(render(&replaced, &[]), original, "removing restores the original");
    }

    #[test]
    fn only_local_names_are_valid() {
        assert!(is_valid("my-shop.localhost"));
        assert!(is_valid("api.v2.test"));
        assert!(!is_valid("google.com"));
        assert!(!is_valid("evil.localhost\n1.2.3.4 bank.com"));
        assert!(!is_valid("-bad.localhost"));
    }
}
