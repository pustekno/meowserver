// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `cargo xtask dist`: builds release binaries and packs the portable
//! folder (PRD §12, §15) into `dist/`: a `.zip` on Windows, `.tar.gz`
//! elsewhere. On Windows it also builds the installer from
//! `packaging/windows/meowserver.iss` when Inno Setup 6 is installed (or
//! `ISCC` points at `ISCC.exe`). On macOS it also builds `MeowServer.app`
//! (signed ad hoc) inside a `.dmg`. Signing comes on top of these.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

#[cfg(target_os = "macos")]
mod macos;

pub(crate) const BINARIES: [&str; 3] = ["meowd", "meowserver", "meow"];
pub(crate) const DOCS: [&str; 6] =
    ["LICENSE", "NOTICE", "AUTHORS", "README.md", "README.id.md", "TRADEMARK.md"];

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("dist") => dist(),
        _ => {
            eprintln!("usage: cargo xtask dist");
            std::process::exit(2);
        }
    }
}

fn dist() -> Result<()> {
    let root = workspace_root();
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--release", "-p", "meow-cli", "-p", "meow-daemon", "-p", "meow-app"])
        .current_dir(&root)
        .status()?;
    if !status.success() {
        bail!("release build failed");
    }

    let version = env!("CARGO_PKG_VERSION");
    let name = format!("MeowServer-{version}-{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let out = root.join("dist").join(&name);
    // A folder left over from trying the build may hold meow.toml, data,
    // or downloads; none of that may end up in a package.
    if out.exists() {
        std::fs::remove_dir_all(&out).with_context(|| {
            format!("cannot clear {}; quit MeowServer running from it (meow quit)", out.display())
        })?;
    }
    std::fs::create_dir_all(out.join("www"))?;

    for bin in BINARIES {
        let file = format!("{bin}{}", std::env::consts::EXE_SUFFIX);
        std::fs::copy(root.join("target").join("release").join(&file), out.join(&file))
            .with_context(|| format!("copying {file}"))?;
    }
    for doc in DOCS {
        std::fs::copy(root.join(doc), out.join(doc)).with_context(|| format!("copying {doc}"))?;
    }
    std::fs::write(
        out.join("www").join("README.txt"),
        "Every folder you put here becomes a site: www/blog -> http://blog.localhost\n",
    )?;

    if cfg!(windows) {
        std::fs::write(out.join("meowserver.ico"), icon())?;
    }

    let archive = if cfg!(windows) { pack_zip(&out)? } else { pack_tar_gz(&out)? };
    report(&archive)?;
    #[cfg(target_os = "macos")]
    report(&macos::dmg(&root, &out, &name)?)?;
    if cfg!(windows) {
        match iscc() {
            Some(iscc) => report(&installer(&root, &iscc, &out, &name)?)?,
            None => println!("Inno Setup 6 not found: installer skipped (install it or set ISCC)"),
        }
    }
    Ok(())
}

fn report(file: &Path) -> Result<()> {
    let size = std::fs::metadata(file)?.len();
    println!("{} ({:.1} MB)", file.display(), size as f64 / 1_048_576.0);
    Ok(())
}

/// `ISCC.exe`: from `ISCC`, `PATH`, or Inno Setup's per-user and
/// per-machine install folders.
fn iscc() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("ISCC") {
        return Some(PathBuf::from(path));
    }
    let mut candidates: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for (var, sub) in [("LOCALAPPDATA", "Programs"), ("ProgramFiles(x86)", ""), ("ProgramFiles", "")] {
        if let Some(base) = std::env::var_os(var) {
            candidates.push(PathBuf::from(base).join(sub).join("Inno Setup 6"));
        }
    }
    candidates.into_iter().map(|dir| dir.join("ISCC.exe")).find(|exe| exe.is_file())
}

fn installer(root: &Path, iscc: &Path, folder: &Path, name: &str) -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    // Windows file versions are numbers only: 0.2.0-beta.1 -> 0.2.0.
    let numeric = version.split(['-', '+']).next().unwrap_or(version);
    let output = folder.parent().unwrap();
    let base = format!("{name}-setup");
    let status = Command::new(iscc)
        .arg("/Q")
        .arg(format!("/DVersion={version}"))
        .arg(format!("/DNumericVersion={numeric}"))
        .arg(format!("/DSource={}", folder.display()))
        .arg(format!("/DOutput={}", output.display()))
        .arg(format!("/DBaseName={base}"))
        .arg(root.join("packaging").join("windows").join("meowserver.iss"))
        .status()
        .with_context(|| format!("running {}", iscc.display()))?;
    if !status.success() {
        bail!("Inno Setup failed");
    }
    Ok(output.join(format!("{base}.exe")))
}

/// `meowserver.ico`: the window's cat head at the sizes Windows asks for,
/// as 32-bit images with alpha (no PNG encoder needed).
fn icon() -> Vec<u8> {
    use meow_core::branding;
    const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];
    let images: Vec<Vec<u8>> = SIZES
        .iter()
        .map(|&size| {
            let rgba = branding::cat(size, branding::ORANGE);
            let mut dib = Vec::new();
            dib.extend(40u32.to_le_bytes()); // BITMAPINFOHEADER
            dib.extend((size as i32).to_le_bytes());
            dib.extend((size as i32 * 2).to_le_bytes()); // colour rows + mask rows
            dib.extend(1u16.to_le_bytes());
            dib.extend(32u16.to_le_bytes());
            dib.extend([0u8; 24]); // BI_RGB; sizes and palette unused
            for y in (0..size).rev() {
                for x in 0..size {
                    let i = ((y * size + x) * 4) as usize;
                    dib.extend([rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
                }
            }
            // The AND mask, all zero: alpha already says what is transparent.
            dib.extend(vec![0u8; (size.div_ceil(32) * 4 * size) as usize]);
            dib
        })
        .collect();

    let mut ico = Vec::new();
    ico.extend([0, 0, 1, 0]);
    ico.extend((SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * SIZES.len() as u32;
    for (&size, image) in SIZES.iter().zip(&images) {
        let side = if size >= 256 { 0 } else { size as u8 };
        ico.extend([side, side, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((image.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        ico.extend(image);
    }
    ico
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives in the workspace").to_path_buf()
}

/// `dir` plus a suffix. Not `with_extension`: the version's dots would be
/// taken for an extension.
pub(crate) fn sibling(dir: &Path, suffix: &str) -> PathBuf {
    let mut name = dir.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(files(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}

fn pack_zip(dir: &Path) -> Result<PathBuf> {
    let archive = sibling(dir, ".zip");
    let mut zip = zip::ZipWriter::new(File::create(&archive)?);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let base = dir.parent().unwrap();
    for file in files(dir)? {
        let name = file.strip_prefix(base)?.to_string_lossy().replace('\\', "/");
        zip.start_file(name, options)?;
        std::io::copy(&mut File::open(&file)?, &mut zip)?;
    }
    zip.finish()?;
    Ok(archive)
}

fn pack_tar_gz(dir: &Path) -> Result<PathBuf> {
    let archive = sibling(dir, ".tar.gz");
    let gz = flate2::write::GzEncoder::new(File::create(&archive)?, flate2::Compression::best());
    let mut tar = tar::Builder::new(gz);
    tar.append_dir_all(dir.file_name().unwrap(), dir)?;
    tar.into_inner()?.finish()?;
    Ok(archive)
}
