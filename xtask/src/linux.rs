// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! The Linux packages: a `.deb` installing into `/usr/lib/meowserver`, and
//! an AppImage. Both keep their data in `~/.local/share/meowserver`
//! (meow_core::layout).

use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::{BINARIES, DOCS, files, run, sibling};

const ICON_SIZES: [u32; 5] = [32, 48, 64, 128, 256];

/// What the window and tray load: GTK and fontconfig come with every
/// desktop; the tray's indicator library and libxdo often do not, and the
/// window opens libxkbcommon-x11 at run time (not seen by the linker).
const DEB_DEPENDS: &str = "libgtk-3-0t64 | libgtk-3-0, libfontconfig1, libxkbcommon-x11-0, \
    libayatana-appindicator3-1 | libappindicator3-1, libxdo3";

/// Shared libraries the AppImage carries so the tray works where they are
/// not installed. GTK itself stays the system's.
const APPIMAGE_LIBS: [&str; 8] = [
    "libxdo.so.3",
    "libxkbcommon-x11.so.0",
    "libxcb-xkb.so.1",
    "libayatana-appindicator3.so.1",
    "libayatana-indicator3.so.7",
    "libayatana-ido3-0.4.so.0",
    "libdbusmenu-glib.so.4",
    "libdbusmenu-gtk3.so.4",
];

const APP_RUN: &str = r#"#!/bin/sh
# MeowServer AppImage. `MeowServer.AppImage meow status` runs the CLI,
# `MeowServer.AppImage meowd` the tray; no argument opens the window.
HERE="$(dirname "$(readlink -f "$0")")"
export LD_LIBRARY_PATH="$HERE/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
case "$1" in
  meowd) shift; exec "$HERE/usr/bin/meowd" "$@" ;;
  meow) shift; "$HERE/usr/bin/meow" "$@" ;;
  *) "$HERE/usr/bin/meowserver" "$@" ;;
esac
status=$?
# meowd (tray and services) may have been started from this AppImage, whose
# files exist only while it is open: keep it open until meowd exits.
while pgrep -f "^$HERE/usr/bin/meowd" >/dev/null 2>&1; do sleep 2; done
exit $status
"#;

pub fn packages(root: &Path, folder: &Path, name: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if found("dpkg-deb") {
        out.push(deb(root, folder)?);
    } else {
        println!("dpkg-deb not found: .deb skipped");
    }
    match appimagetool() {
        Some(tool) => out.push(appimage(root, folder, name, &tool)?),
        None => println!("appimagetool not found: AppImage skipped (install it or set APPIMAGETOOL)"),
    }
    Ok(out)
}

fn deb(root: &Path, folder: &Path) -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    let stage = sibling(folder, "-deb");
    let _ = std::fs::remove_dir_all(&stage);
    let lib = stage.join("usr/lib/meowserver");
    std::fs::create_dir_all(&lib)?;
    std::fs::create_dir_all(stage.join("usr/bin"))?;
    for bin in BINARIES {
        std::fs::copy(folder.join(bin), lib.join(bin)).with_context(|| format!("copying {bin}"))?;
    }
    // /proc/self/exe resolves the link, so the binaries still find each
    // other and know they are installed (meow_core::layout).
    symlink("../lib/meowserver/meow", stage.join("usr/bin/meow"))?;
    symlink("../lib/meowserver/meowserver", stage.join("usr/bin/meowserver"))?;
    let docs = stage.join("usr/share/doc/meowserver");
    std::fs::create_dir_all(&docs)?;
    for doc in DOCS {
        std::fs::copy(root.join(doc), docs.join(doc))?;
    }
    std::fs::copy(root.join("LICENSE"), docs.join("copyright"))?;
    write_desktop_files(&stage.join("usr/share"), "meowserver")?;

    let size_kb: u64 =
        files(&stage)?.iter().filter_map(|f| std::fs::symlink_metadata(f).ok()).map(|m| m.len()).sum::<u64>()
            / 1024;
    std::fs::create_dir_all(stage.join("DEBIAN"))?;
    std::fs::write(
        stage.join("DEBIAN/control"),
        format!(
            "Package: meowserver\n\
             Version: {}\n\
             Architecture: {arch}\n\
             Maintainer: Akbar Bahaulloh <pusko123@gmail.com>\n\
             Installed-Size: {size_kb}\n\
             Depends: {DEB_DEPENDS}\n\
             Section: devel\n\
             Priority: optional\n\
             Homepage: https://github.com/pustekno/meowserver\n\
             Description: Lightweight local web development environment\n \
             Serves every folder in ~/www at project.localhost with Caddy, PHP,\n \
             and MariaDB or MySQL, downloaded on demand from official sources.\n",
            version.replace('-', "~")
        ),
    )?;
    let deb = sibling(folder, ".deb");
    run(Command::new("dpkg-deb").args(["--root-owner-group", "-Zxz", "--build"]).arg(&stage).arg(&deb))?;
    std::fs::remove_dir_all(&stage)?;
    Ok(deb)
}

fn appimage(root: &Path, folder: &Path, name: &str, tool: &Path) -> Result<PathBuf> {
    let dir = sibling(folder, ".AppDir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("usr/bin"))?;
    std::fs::create_dir_all(dir.join("usr/lib"))?;
    for bin in BINARIES {
        std::fs::copy(folder.join(bin), dir.join("usr/bin").join(bin))
            .with_context(|| format!("copying {bin}"))?;
    }
    for doc in DOCS {
        std::fs::copy(root.join(doc), dir.join(doc))?;
    }
    for lib in APPIMAGE_LIBS {
        match system_library(lib) {
            Some(path) => {
                std::fs::copy(&path, dir.join("usr/lib").join(lib))
                    .with_context(|| format!("copying {lib}"))?;
            }
            None => println!("{lib} not installed here: the AppImage relies on the system's"),
        }
    }
    let app_run = dir.join("AppRun");
    std::fs::write(&app_run, APP_RUN)?;
    std::fs::set_permissions(&app_run, std::fs::Permissions::from_mode(0o755))?;
    write_desktop_files(&dir.join("usr/share"), "meowserver")?;
    std::fs::copy(dir.join("usr/share/applications/meowserver.desktop"), dir.join("meowserver.desktop"))?;
    std::fs::copy(
        dir.join("usr/share/icons/hicolor/256x256/apps/meowserver.png"),
        dir.join("meowserver.png"),
    )?;
    symlink("meowserver.png", dir.join(".DirIcon"))?;

    let image = folder.with_file_name(format!("{name}.AppImage"));
    let _ = std::fs::remove_file(&image);
    let mut cmd = Command::new(tool);
    // appimagetool is itself an AppImage; CI runners have no FUSE.
    cmd.env("APPIMAGE_EXTRACT_AND_RUN", "1").env("ARCH", std::env::consts::ARCH).arg("--no-appstream");
    if let Some(runtime) = std::env::var_os("APPIMAGE_RUNTIME") {
        cmd.arg("--runtime-file").arg(runtime);
    }
    run(cmd.arg(&dir).arg(&image))?;
    std::fs::remove_dir_all(&dir)?;
    Ok(image)
}

/// `share/applications/meowserver.desktop` and the icon at the sizes
/// desktops look for.
fn write_desktop_files(share: &Path, icon: &str) -> Result<()> {
    std::fs::create_dir_all(share.join("applications"))?;
    std::fs::write(
        share.join("applications/meowserver.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=MeowServer\n\
             Comment=Local web development environment\nExec=meowserver\nIcon={icon}\n\
             Categories=Development;\nTerminal=false\nStartupWMClass=meowserver\n"
        ),
    )?;
    for size in ICON_SIZES {
        let dir = share.join(format!("icons/hicolor/{size}x{size}/apps"));
        std::fs::create_dir_all(&dir)?;
        let rgba = meow_core::branding::cat(size, meow_core::branding::ORANGE);
        std::fs::write(dir.join(format!("{icon}.png")), crate::png::encode(size, &rgba)?)?;
    }
    Ok(())
}

fn found(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|dir| dir.join(program).is_file()))
}

fn appimagetool() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("APPIMAGETOOL") {
        return Some(PathBuf::from(path));
    }
    found("appimagetool").then(|| PathBuf::from("appimagetool"))
}

/// Where the dynamic linker finds `soname`, from `ldconfig -p`.
fn system_library(soname: &str) -> Option<PathBuf> {
    let out = Command::new("ldconfig").arg("-p").output().ok()?;
    let arch = if cfg!(target_arch = "x86_64") { "x86-64" } else { "AArch64" };
    String::from_utf8_lossy(&out.stdout).lines().find_map(|line| {
        let (left, path) = line.trim().split_once(" => ")?;
        (left.starts_with(&format!("{soname} ")) && left.contains(arch)).then(|| PathBuf::from(path))
    })
}
