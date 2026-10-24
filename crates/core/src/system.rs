// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Changes MeowServer makes outside its own folder, and undoing them all in
//! one step (PRD F1 login item, F36 undo).

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::config::Config;
use crate::layout::Layout;
use crate::platform::{Os, os};
use crate::{defender, hosts, tls};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const LOGIN_NAME: &str = "MeowServer";

/// Starts `meowd` when the user logs in, or stops doing so (PRD F1).
pub fn set_login_item(layout: &Layout, enabled: bool) -> Result<()> {
    let meowd = std::env::current_exe()?.with_file_name(format!("meowd{}", crate::platform::EXE_SUFFIX));
    match os() {
        Os::Windows => {
            let ok = if enabled {
                // MEOW_HOME is not inherited at login; meowd finds its folder
                // next to itself, which is the portable layout.
                let value = format!("\"{}\"", meowd.display());
                run("reg", &["add", RUN_KEY, "/v", LOGIN_NAME, "/t", "REG_SZ", "/d", &value, "/f"])
            } else {
                run("reg", &["delete", RUN_KEY, "/v", LOGIN_NAME, "/f"]) || !login_item_enabled()
            };
            if !ok {
                bail!("could not change the login item");
            }
        }
        Os::MacOs => {
            let plist = home()?.join("Library/LaunchAgents/id.pustekno.meowserver.plist");
            if enabled {
                std::fs::create_dir_all(plist.parent().unwrap())?;
                std::fs::write(&plist, launch_agent(&meowd, layout.root()))?;
            } else {
                let _ = std::fs::remove_file(&plist);
            }
        }
        Os::Linux => {
            let desktop = home()?.join(".config/autostart/meowserver.desktop");
            if enabled {
                std::fs::create_dir_all(desktop.parent().unwrap())?;
                let body = format!(
                    "[Desktop Entry]\nType=Application\nName=MeowServer\nExec=env MEOW_HOME=\"{}\" \"{}\"\nX-GNOME-Autostart-enabled=true\n",
                    layout.root().display(),
                    meowd.display()
                );
                std::fs::write(&desktop, body)?;
            } else {
                let _ = std::fs::remove_file(&desktop);
            }
        }
    }
    Ok(())
}

pub fn login_item_enabled() -> bool {
    match os() {
        Os::Windows => run("reg", &["query", RUN_KEY, "/v", LOGIN_NAME]),
        Os::MacOs => {
            home().is_ok_and(|h| h.join("Library/LaunchAgents/id.pustekno.meowserver.plist").exists())
        }
        Os::Linux => home().is_ok_and(|h| h.join(".config/autostart/meowserver.desktop").exists()),
    }
}

/// Removes everything MeowServer changed outside its folder: hosts entries,
/// the CA in trust stores, Defender exclusions, the login item. Returns what
/// was done; keeps going when one step fails.
pub fn undo(layout: &Layout, meow: &Path) -> Vec<String> {
    let mut notes = Vec::new();
    let mut config = Config::load(layout).unwrap_or_default();

    config.general.manage_hosts = false;
    match hosts::sync(layout, &config, meow) {
        Ok(hosts::Sync::Declined) => {
            notes.push("hosts file: not changed (administrator rights declined)".into())
        }
        Ok(_) => notes.push("hosts file: MeowServer entries removed".into()),
        Err(e) => notes.push(format!("hosts file: {e:#}")),
    }
    if tls::is_trusted(layout) {
        match tls::untrust(layout) {
            Ok(()) => notes.push("local certificate authority: removed from the trust store".into()),
            Err(e) => notes.push(format!("local certificate authority: {e:#}")),
        }
    }
    if !defender::recorded(layout).paths.is_empty() {
        let args = vec![
            "privileged".into(),
            "defender-remove".into(),
            "--home".into(),
            layout.root().display().to_string(),
        ];
        match crate::platform::run_elevated(meow, &args) {
            Ok(0) => notes.push("Defender: exclusions removed".into()),
            Ok(code) => notes.push(format!("Defender: removing exclusions failed (exit code {code})")),
            Err(_) => notes.push("Defender: not changed (administrator rights declined)".into()),
        }
    }
    if login_item_enabled() {
        match set_login_item(layout, false) {
            Ok(()) => notes.push("login item: removed".into()),
            Err(e) => notes.push(format!("login item: {e:#}")),
        }
    }
    config.general.start_on_login = false;
    if let Err(e) = config.save(layout) {
        notes.push(format!("meow.toml: {e:#}"));
    }
    if notes.is_empty() {
        notes.push("nothing to undo".into());
    }
    notes
}

fn launch_agent(meowd: &Path, root: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>id.pustekno.meowserver</string>
  <key>ProgramArguments</key><array><string>{}</string></array>
  <key>EnvironmentVariables</key><dict><key>MEOW_HOME</key><string>{}</string></dict>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
        meowd.display(),
        root.display()
    )
}

fn home() -> Result<std::path::PathBuf> {
    std::env::var_os("HOME").map(Into::into).context("HOME is not set")
}

fn run(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
