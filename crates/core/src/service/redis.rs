// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Redis (PRD S5): no authentication, loopback only, nothing persisted.

use anyhow::{Result, bail};
use serde::Serialize;

use super::{Kind, write_generated};
use crate::config::Config;
use crate::layout::Layout;
use crate::supervisor::{ProcessSpec, Restart};
use crate::template;

pub const DEFAULT_PORT: u16 = 6379;

#[derive(Serialize)]
struct Context {
    version: String,
    bind: &'static str,
    port: u16,
    custom: Option<String>,
}

pub fn plan(layout: &Layout, config: &Config) -> Result<ProcessSpec> {
    let Some(installed) = super::select(layout, Kind::Redis, config.version_selector("redis")) else {
        bail!("Redis is not installed. Run `meow install redis`.");
    };
    let data = layout.data().join("redis");
    std::fs::create_dir_all(&data)?;
    // The Windows build (msys2) misreads absolute Windows paths, so Redis
    // runs inside its data folder and every path it sees is relative, and
    // user overrides are copied in rather than included.
    let custom = std::fs::read_to_string(layout.custom().join("redis").join("redis.conf")).ok();
    let ctx = Context {
        version: installed.version.to_string(),
        bind: "127.0.0.1 ::1",
        port: config.port("redis", DEFAULT_PORT),
        custom,
    };
    let conf = layout.generated().join("redis").join("redis.conf");
    write_generated(&conf, &template::render("redis/redis.conf", ctx)?)?;
    // data/redis → ../../etc/generated/redis/redis.conf
    let relative =
        std::path::Path::new("..").join("..").join("etc").join("generated").join("redis").join("redis.conf");
    let relative = relative.to_string_lossy().replace('\\', "/");
    Ok(ProcessSpec {
        id: "redis".into(),
        program: installed.exe("redis-server"),
        args: vec![relative.into()],
        env: Vec::new(),
        cwd: Some(data),
        log: layout.logs().join("redis.log"),
        restart: Restart::OnFailure,
        graceful_stop: None,
    })
}
