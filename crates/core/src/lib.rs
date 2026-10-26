// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! MeowServer core: everything except the user interfaces.
//!
//! `meowd`, `meow`, and `meowserver` only call into this crate (PRD §11).

pub mod app;
pub mod branding;
pub mod config;
pub mod create;
pub mod database;
pub mod defender;
pub mod doctor;
pub mod download;
pub mod engine;
pub mod hosts;
pub mod http;
pub mod import;
pub mod ipc;
pub mod layout;
pub mod logging;
pub mod logs;
pub mod pecl;
pub mod platform;
pub mod provider;
pub mod report;
pub mod service;
pub mod setup;
pub mod site;
pub mod supervisor;
pub mod system;
pub mod template;
pub mod terminal;
pub mod tls;
pub mod update;
pub mod version;
pub mod warm;
pub mod watch;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
