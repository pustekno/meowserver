// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Redis (PRD S5): no authentication, loopback only, nothing persisted.

use std::path::PathBuf;

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
    data_dir: PathBuf,
    custom_conf: Option<PathBuf>,
}

pub fn plan(layout: &Layout, config: &Config) -> Result<ProcessSpec> {
    let Some(installed) = super::select(layout, Kind::Redis, config.version_selector("redis")) else {
        bail!("Redis is not installed. Run `meow install redis`.");
    };
    let data = layout.data().join("redis");
    std::fs::create_dir_all(&data)?;
    let custom = layout.custom().join("redis").join("redis.conf");
    let ctx = Context {
        version: installed.version.to_string(),
        bind: "127.0.0.1 ::1",
        port: config.port("redis", DEFAULT_PORT),
        data_dir: data,
        custom_conf: custom.exists().then_some(custom),
    };
    let conf = layout.generated().join("redis").join("redis.conf");
    write_generated(&conf, &template::render("redis/redis.conf", ctx)?)?;
    Ok(ProcessSpec {
        id: "redis".into(),
        program: installed.exe("redis-server"),
        args: vec![conf.into()],
        env: Vec::new(),
        cwd: Some(installed.dir.clone()),
        log: layout.logs().join("redis.log"),
        restart: Restart::OnFailure,
        graceful_stop: None,
    })
}
