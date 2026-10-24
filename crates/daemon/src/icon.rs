// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Tray icons drawn in code: a cat head, orange when services run and grey
//! when stopped. Placeholder until the real logo exists.

use tray_icon::Icon;

const SIZE: u32 = 32;

pub fn running() -> Icon {
    cat([0xF2, 0x8C, 0x28])
}

pub fn stopped() -> Icon {
    cat([0x8A, 0x8A, 0x8A])
}

fn cat(rgb: [u8; 3]) -> Icon {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let head = (fx - 16.0).powi(2) + (fy - 19.0).powi(2) <= 11.5f32.powi(2);
            let left_ear = inside_triangle((fx, fy), (5.0, 2.0), (5.0, 16.0), (16.0, 10.0));
            let right_ear = inside_triangle((fx, fy), (27.0, 2.0), (27.0, 16.0), (16.0, 10.0));
            let eye = (fx - 11.5).powi(2) + (fy - 18.0).powi(2) <= 4.0
                || (fx - 20.5).powi(2) + (fy - 18.0).powi(2) <= 4.0;
            if (head || left_ear || right_ear) && !eye {
                let i = ((y * SIZE + x) * 4) as usize;
                rgba[i..i + 3].copy_from_slice(&rgb);
                rgba[i + 3] = 0xFF;
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("icon dimensions match the buffer")
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
