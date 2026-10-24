// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! The cat-head icon, drawn in code so no image decoder is needed. Used by
//! the tray and the window. Placeholder until the real logo exists.

/// The attribution every copy must show (NOTICE, GPLv3 section 7(b)).
pub const ATTRIBUTION: &str =
    "Originally created by Pustekno (https://pustekno.id) and Puskomedia (https://www.puskomedia.id).";

pub const ORANGE: [u8; 3] = [0xF2, 0x8C, 0x28];
pub const GREY: [u8; 3] = [0x8A, 0x8A, 0x8A];

/// RGBA pixels of a `size`×`size` cat head in `rgb`.
pub fn cat(size: u32, rgb: [u8; 3]) -> Vec<u8> {
    let scale = size as f32 / 32.0;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
            let head = (fx - 16.0).powi(2) + (fy - 19.0).powi(2) <= 11.5f32.powi(2);
            let left_ear = inside_triangle((fx, fy), (5.0, 2.0), (5.0, 16.0), (16.0, 10.0));
            let right_ear = inside_triangle((fx, fy), (27.0, 2.0), (27.0, 16.0), (16.0, 10.0));
            let eye = (fx - 11.5).powi(2) + (fy - 18.0).powi(2) <= 4.0
                || (fx - 20.5).powi(2) + (fy - 18.0).powi(2) <= 4.0;
            if (head || left_ear || right_ear) && !eye {
                let i = ((y * size + x) * 4) as usize;
                rgba[i..i + 3].copy_from_slice(&rgb);
                rgba[i + 3] = 0xFF;
            }
        }
    }
    rgba
}

fn inside_triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let sign = |p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let (d1, d2, d3) = (sign(p, a, b), sign(p, b, c), sign(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}
