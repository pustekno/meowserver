// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! PNG icons for the macOS and Linux packages.

use anyhow::Result;

/// A minimal RGBA PNG.
pub fn encode(size: u32, rgba: &[u8]) -> Result<Vec<u8>> {
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
