// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Problem reports (PRD F38). Crashes are written to `logs/crash-*.log`;
//! a report is assembled for the user to read and, if they choose, submit
//! as a GitHub issue. Nothing is ever sent automatically.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::layout::Layout;
use crate::logs;
use crate::service::{self, Kind};

const NEW_ISSUE: &str = "https://github.com/pustekno/meowserver/issues/new";
/// Browsers and GitHub reject very long URLs; leave room for encoding.
const MAX_BODY: usize = 6000;

/// Records panics in `logs/crash-<time>.log`, then lets the default hook
/// run (which prints to stderr when there is a console).
pub fn install_panic_hook(layout: &Layout) {
    let dir = layout.logs();
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let thread = std::thread::current().name().unwrap_or("unnamed").to_owned();
        let backtrace = std::backtrace::Backtrace::force_capture();
        let text = format!(
            "MeowServer {} crashed\nprogram: {}\nthread: {thread}\n{info}\n\nbacktrace:\n{backtrace}\n",
            crate::VERSION,
            std::env::args().next().unwrap_or_default(),
        );
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(format!("crash-{secs}.log")), text);
        log::error!("crashed: {info}");
        default(info);
    }));
}

/// The newest crash log not yet acknowledged with [`acknowledge`].
pub fn unreported_crash(layout: &Layout) -> Option<PathBuf> {
    let seen = std::fs::read_to_string(layout.tmp().join("crash-seen")).unwrap_or_default();
    let latest = latest_crash(layout)?;
    let name = latest.file_name()?.to_string_lossy().into_owned();
    (name != seen.trim()).then_some(latest)
}

pub fn acknowledge(layout: &Layout) {
    if let Some(latest) = latest_crash(layout) {
        let _ = std::fs::create_dir_all(layout.tmp());
        let _ = std::fs::write(
            layout.tmp().join("crash-seen"),
            latest.file_name().unwrap_or_default().to_string_lossy().as_bytes(),
        );
    }
}

fn latest_crash(layout: &Layout) -> Option<PathBuf> {
    std::fs::read_dir(layout.logs())
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("crash-")))
        .max()
}

/// A Markdown report: version, OS, installed and active versions, the
/// latest crash, and the end of `meowd.log`. Paths under the home folder are
/// shortened to `~`. Site logs (PHP, web server, databases) are left out.
pub fn build(layout: &Layout) -> String {
    let config = Config::load(layout).unwrap_or_default();
    let mut out = String::new();
    let _ = writeln!(out, "**What happened?**\n\n<!-- Describe what you did and what went wrong. -->\n");
    let _ = writeln!(out, "**Environment**\n");
    let _ = writeln!(out, "- MeowServer {}", crate::VERSION);
    let _ = writeln!(out, "- OS: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    let _ = writeln!(out, "- Web server: {:?}", config.general.web_server);
    for kind in Kind::ALL {
        let installed = service::installed(layout, kind);
        if installed.is_empty() {
            continue;
        }
        let versions: Vec<String> = installed.iter().map(|i| i.version.to_string()).collect();
        let active = config.version_selector(kind.id()).unwrap_or("newest");
        let _ = writeln!(out, "- {}: {} (active: {active})", kind.id(), versions.join(", "));
    }
    if let Some(crash) = latest_crash(layout) {
        let text = std::fs::read_to_string(&crash).unwrap_or_default();
        let short: String = text.lines().take(60).collect::<Vec<_>>().join("\n");
        let _ = writeln!(
            out,
            "\n**Latest crash** (`{}`)\n\n```\n{short}\n```",
            crash.file_name().unwrap().to_string_lossy()
        );
    }
    let tail = logs::tail(&layout.logs().join("meowd.log"), 200);
    if !tail.is_empty() {
        let _ = writeln!(out, "\n**meowd.log (last {} lines)**\n\n```\n{}\n```", tail.len(), tail.join("\n"));
    }
    redact_home(&out)
}

/// A link that opens a new issue with the report filled in. Long reports
/// are cut from the middle of the log so the URL stays usable.
pub fn issue_url(title: &str, body: &str) -> String {
    let body = if body.len() > MAX_BODY {
        let mut cut = MAX_BODY;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}\n```\n\n_(report shortened; please attach the full logs if they help)_", &body[..cut])
    } else {
        body.to_owned()
    };
    format!("{NEW_ISSUE}?title={}&body={}&labels=bug", encode(title), encode(&body))
}

fn redact_home(text: &str) -> String {
    let mut out = text.to_owned();
    for var in ["USERPROFILE", "HOME"] {
        if let Some(home) = std::env::var_os(var) {
            let home = home.to_string_lossy().into_owned();
            if home.len() > 3 {
                out = out.replace(&home, "~").replace(&home.replace('\\', "/"), "~");
            }
        }
    }
    out
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

pub fn crash_file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_for_urls() {
        assert_eq!(super::encode("a b&c/ü"), "a%20b%26c%2F%C3%BC");
    }

    #[test]
    fn long_bodies_are_cut() {
        let url = super::issue_url("t", &"x".repeat(20_000));
        assert!(url.len() < 7_000);
    }
}
