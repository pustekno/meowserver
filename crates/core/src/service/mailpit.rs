// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Mailpit catches mail sent by PHP (PRD S6, F22).

use anyhow::{Result, bail};

use super::Kind;
use crate::config::Config;
use crate::layout::Layout;
use crate::supervisor::{ProcessSpec, Restart};

pub const SMTP_PORT: u16 = 1025;
pub const WEB_PORT: u16 = 8025;

pub fn web_url(config: &Config) -> String {
    format!("http://127.0.0.1:{}/", config.web_port("mailpit", WEB_PORT))
}

pub fn plan(layout: &Layout, config: &Config) -> Result<ProcessSpec> {
    let Some(installed) = super::select(layout, Kind::Mailpit, config.version_selector("mailpit")) else {
        bail!("Mailpit is not installed. {}", crate::provider::install_advice(Kind::Mailpit, None, layout));
    };
    let data = layout.data().join("mailpit");
    std::fs::create_dir_all(&data)?;
    let smtp = config.port("mailpit", SMTP_PORT);
    let web = config.web_port("mailpit", WEB_PORT);
    Ok(ProcessSpec {
        id: "mailpit".into(),
        program: installed.exe("mailpit"),
        args: vec![
            "--listen".into(),
            format!("127.0.0.1:{web}").into(),
            "--smtp".into(),
            format!("127.0.0.1:{smtp}").into(),
            "--database".into(),
            data.join("mailpit.db").into(),
        ],
        env: Vec::new(),
        cwd: Some(installed.dir.clone()),
        log: layout.logs().join("mailpit.log"),
        restart: Restart::OnFailure,
        graceful_stop: None,
    })
}
