// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Outbound HTTP. Only used when the user asks for a download or an update
//! check (PRD §3, principle 1).

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct Http {
    agent: ureq::Agent,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(format!("MeowServer/{} (+https://github.com/pustekno/meowserver)", crate::VERSION))
            .https_only(true)
            // Requests are few; reusing connections only invites trouble
            // with servers that mishandle keep-alive (xdebug.org does).
            .max_idle_connections(0)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build();
        Self { agent: config.into() }
    }

    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        self.get(url)
            .call()
            .with_context(|| format!("request to {url} failed"))?
            .body_mut()
            .read_json()
            .with_context(|| format!("unexpected response from {url}"))
    }

    pub fn get_text(&self, url: &str) -> Result<String> {
        self.get(url)
            .call()
            .with_context(|| format!("request to {url} failed"))?
            .body_mut()
            .read_to_string()
            .with_context(|| format!("unexpected response from {url}"))
    }

    /// GitHub's API allows 60 requests an hour per address without a
    /// token, which shared CI runners use up. `MEOW_GITHUB_TOKEN`, when
    /// set, goes to api.github.com only; a token people keep for other
    /// tools (`GITHUB_TOKEN`) is never sent.
    fn get(&self, url: &str) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        let request = self.agent.get(url);
        match std::env::var("MEOW_GITHUB_TOKEN") {
            Ok(token) if !token.is_empty() && url.starts_with("https://api.github.com/") => {
                request.header("Authorization", format!("Bearer {token}"))
            }
            _ => request,
        }
    }

    /// Streams `url` into `dest`, reporting `(received, total)` as it goes.
    /// Returns the SHA-256 of the bytes written, as lowercase hex.
    pub fn download(
        &self,
        url: &str,
        dest: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<String> {
        let response = self.agent.get(url).call().with_context(|| format!("download of {url} failed"))?;
        let body = response.into_body();
        let total = body.content_length();
        let mut reader = body.into_reader();
        let mut file =
            std::fs::File::create(dest).with_context(|| format!("cannot create {}", dest.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut received = 0u64;
        loop {
            let n = reader.read(&mut buf).with_context(|| format!("download of {url} interrupted"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])?;
            received += n as u64;
            progress(received, total);
        }
        file.sync_all()?;
        Ok(to_hex(&hasher.finalize()))
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}
