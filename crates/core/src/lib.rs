// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! MeowServer core: everything except the user interfaces.
//!
//! `meowd`, `meow`, and `meowserver` only call into this crate (PRD §11).

pub mod config;
pub mod download;
pub mod engine;
pub mod http;
pub mod ipc;
pub mod layout;
pub mod logging;
pub mod platform;
pub mod provider;
pub mod service;
pub mod site;
pub mod supervisor;
pub mod template;
pub mod version;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
