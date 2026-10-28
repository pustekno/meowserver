// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! The macOS app bundle and disk image.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::{BINARIES, DOCS, sibling};

/// `MeowServer.app` beside the portable folder, then a `.dmg` holding it
/// and a link to /Applications. The app keeps its data in
/// `~/Library/Application Support/MeowServer` (meow_core::layout).
pub fn dmg(root: &Path, folder: &Path, name: &str) -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    let stage = folder.parent().unwrap().join(format!("{name}-dmg"));
    let _ = std::fs::remove_dir_all(&stage);
    let app = stage.join("MeowServer.app");
    let contents = app.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS"))?;
    std::fs::create_dir_all(contents.join("Resources"))?;
    for bin in BINARIES {
        std::fs::copy(folder.join(bin), contents.join("MacOS").join(bin))
            .with_context(|| format!("copying {bin}"))?;
    }
    for doc in DOCS {
        std::fs::copy(root.join(doc), contents.join("Resources").join(doc))?;
    }
    std::fs::write(contents.join("Resources").join("MeowServer.icns"), icns()?)?;
    std::fs::write(contents.join("Info.plist"), info_plist(version))?;
    run(Command::new("codesign").args(["--force", "--deep", "--sign", "-"]).arg(&app))?;
    std::os::unix::fs::symlink("/Applications", stage.join("Applications"))?;

    let image = sibling(folder, ".dmg");
    let _ = std::fs::remove_file(&image);
    run(Command::new("hdiutil")
        .args(["create", "-quiet", "-fs", "HFS+", "-format", "UDZO", "-volname"])
        .arg(format!("MeowServer {version}"))
        .arg("-srcfolder")
        .arg(&stage)
        .arg(&image))?;
    std::fs::remove_dir_all(&stage)?;
    Ok(image)
}

fn run(cmd: &mut Command) -> Result<()> {
    let status = cmd.status().with_context(|| format!("running {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("{:?} failed", cmd.get_program());
    }
    Ok(())
}

fn info_plist(version: &str) -> String {
    let numeric = version.split(['-', '+']).next().unwrap_or(version);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>MeowServer</string>
	<key>CFBundleExecutable</key>
	<string>meowserver</string>
	<key>CFBundleIconFile</key>
	<string>MeowServer</string>
	<key>CFBundleIdentifier</key>
	<string>id.pustekno.meowserver</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>MeowServer</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{numeric}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>LSMinimumSystemVersion</key>
	<string>12.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#
    )
}

/// `MeowServer.icns`: the cat head as PNGs at every size macOS asks for.
fn icns() -> Result<Vec<u8>> {
    use meow_core::branding;
    const TYPES: [(&[u8; 4], u32); 10] = [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
        (b"ic10", 1024),
        (b"ic11", 32),
        (b"ic12", 64),
        (b"ic13", 256),
    ];
    let mut body = Vec::new();
    for (kind, size) in TYPES {
        let image = png(size, &branding::cat(size, branding::ORANGE))?;
        body.extend(kind);
        body.extend((image.len() as u32 + 8).to_be_bytes());
        body.extend(image);
    }
    let mut icns = b"icns".to_vec();
    icns.extend((body.len() as u32 + 8).to_be_bytes());
    icns.extend(body);
    Ok(icns)
}

/// A minimal RGBA PNG.
fn png(size: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        out.extend(kind);
        out.extend(data);
        out.extend(crc.sum().to_be_bytes());
    }
    let mut header = Vec::new();
    header.extend(size.to_be_bytes());
    header.extend(size.to_be_bytes());
    header.extend([8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    for row in rgba.chunks(size as usize * 4) {
        z.write_all(&[0])?; // no filter
        z.write_all(row)?;
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &z.finish()?);
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}
