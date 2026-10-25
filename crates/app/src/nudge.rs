// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! The one "Like MeowServer? Share it" card on Home (PRD F46). Not an ad:
//! nothing from anyone else, nothing sent. It shows once after two weeks
//! of use; "Later" brings it back after three months, "Don't show again"
//! ends it. The dates live in `data/app-state.json` on this computer only.

use std::time::{SystemTime, UNIX_EPOCH};

use meow_core::layout::Layout;
use serde::{Deserialize, Serialize};

const DAY: u64 = 24 * 60 * 60;
const FIRST: u64 = 14 * DAY;
const LATER: u64 = 90 * DAY;

#[derive(Default, Serialize, Deserialize)]
struct State {
    /// When MeowServer's window first opened here (Unix seconds).
    first_run: u64,
    /// "Later": not before this time.
    share_after: Option<u64>,
    share_never: bool,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn load(layout: &Layout) -> State {
    let path = layout.data().join("app-state.json");
    match std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()) {
        Some(state) => state,
        None => {
            let state = State { first_run: now(), ..State::default() };
            save(layout, &state);
            state
        }
    }
}

fn save(layout: &Layout, state: &State) {
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = std::fs::create_dir_all(layout.data());
        let _ = std::fs::write(layout.data().join("app-state.json"), text);
    }
}

pub fn share_due(layout: &Layout) -> bool {
    let state = load(layout);
    !state.share_never && now() >= state.share_after.unwrap_or(state.first_run + FIRST)
}

pub fn share_later(layout: &Layout) {
    let mut state = load(layout);
    state.share_after = Some(now() + LATER);
    save(layout, &state);
}

pub fn share_never(layout: &Layout) {
    let mut state = load(layout);
    state.share_never = true;
    save(layout, &state);
}
