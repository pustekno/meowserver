// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Live checks against every download source (PRD §9.5). Ignored by
//! default because they need the network; CI runs them weekly with
//! `cargo test -p meow-core --test upstream -- --ignored`.

use meow_core::http::Http;
use meow_core::provider;
use meow_core::service::Kind;

#[test]
#[ignore = "needs the network"]
fn every_provider_lists_releases_with_urls() {
    let http = Http::new();
    let mut failures = Vec::new();
    for kind in Kind::ALL {
        let Some(p) = provider::for_service(kind) else {
            println!("{:<9} no download on this OS (system packages)", kind.id());
            continue;
        };
        match p.releases(&http) {
            Ok(releases) if !releases.is_empty() && releases.iter().all(|r| !r.assets.is_empty()) => {
                let newest = &releases[0];
                println!("{:<9} {} releases, newest {}", kind.id(), releases.len(), newest.version);
            }
            Ok(_) => failures.push(format!("{}: no releases", kind.id())),
            Err(e) => failures.push(format!("{}: {e:#}", kind.id())),
        }
    }
    assert!(failures.is_empty(), "providers failed:\n{}", failures.join("\n"));
}

#[test]
#[ignore = "needs the network"]
fn xdebug_page_lists_current_php() {
    if cfg!(not(windows)) {
        return;
    }
    let index = provider::xdebug::index(&Http::new()).unwrap();
    let (version, asset) = provider::xdebug::for_php(&index, "8.4").unwrap();
    assert!(asset.sha256.is_some(), "xdebug.org stopped publishing SHA-256");
    println!("xdebug {version} for PHP 8.4");
}
