// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Finding and reading service logs (PRD F31). These stay on the user's
//! machine; they are never part of a problem report (PRD F38).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::layout::Layout;

/// Current (not rotated) log files whose name starts with `name`, newest
/// first. `web` matches all three web servers.
pub fn find(layout: &Layout, name: &str) -> Vec<PathBuf> {
    let prefixes: Vec<&str> = match name {
        "web" => vec!["caddy", "nginx", "apache"],
        "postgresql" => vec!["postgres"],
        other => vec![other],
    };
    let Ok(entries) = std::fs::read_dir(layout.logs()) else { return Vec::new() };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .filter(|p| {
            let file = p.file_name().unwrap().to_string_lossy();
            prefixes.iter().any(|pre| file == format!("{pre}.log") || file.starts_with(&format!("{pre}-")))
        })
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.into_iter().map(|(_, p)| p).collect()
}

/// The last `n` lines of a file, reading at most the final 256 KB.
pub fn tail(path: &Path, n: usize) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else { return Vec::new() };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(256 * 1024);
    let _ = file.seek(SeekFrom::Start(start));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0); // probably cut in the middle
    }
    let skip = lines.len().saturating_sub(n);
    lines.split_off(skip)
}
