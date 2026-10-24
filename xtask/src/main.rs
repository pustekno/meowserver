// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `cargo xtask dist`: builds release binaries and packs the portable
//! folder (PRD §12, §15) into `dist/`: a `.zip` on Windows, `.tar.gz`
//! elsewhere. Installers and signing come on top of this folder.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

const BINARIES: [&str; 3] = ["meowd", "meowserver", "meow"];
const DOCS: [&str; 6] = ["LICENSE", "NOTICE", "AUTHORS", "README.md", "README.id.md", "TRADEMARK.md"];

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
    let _ = std::fs::remove_dir_all(&out);
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

    let archive = if cfg!(windows) { pack_zip(&out)? } else { pack_tar_gz(&out)? };
    let size = std::fs::metadata(&archive)?.len();
    println!("{} ({:.1} MB)", archive.display(), size as f64 / 1_048_576.0);
    Ok(())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives in the workspace").to_path_buf()
}

/// `dir` plus a suffix. Not `with_extension`: the version's dots would be
/// taken for an extension.
fn sibling(dir: &Path, suffix: &str) -> PathBuf {
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
