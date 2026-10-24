// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Tray icons: orange when services run, grey when stopped.

use meow_core::branding;
use tray_icon::Icon;

const SIZE: u32 = 32;

pub fn running() -> Icon {
    Icon::from_rgba(branding::cat(SIZE, branding::ORANGE), SIZE, SIZE).expect("icon size matches")
}

pub fn stopped() -> Icon {
    Icon::from_rgba(branding::cat(SIZE, branding::GREY), SIZE, SIZE).expect("icon size matches")
}
