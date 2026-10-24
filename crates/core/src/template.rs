// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Renders service configs from the plain files in `templates/`, which are
//! compiled into the binary so the app stays a single portable folder.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use minijinja::Environment;
use minijinja::syntax::SyntaxConfig;
use serde::Serialize;

const TEMPLATES: &[(&str, &str)] = &[
    ("caddy/Caddyfile", include_str!("../../../templates/caddy/Caddyfile.j2")),
    ("nginx/nginx.conf", include_str!("../../../templates/nginx/nginx.conf.j2")),
    ("apache/httpd.conf", include_str!("../../../templates/apache/httpd.conf.j2")),
    ("index/index.html", include_str!("../../../templates/index/index.html.j2")),
    ("mysql/my.ini", include_str!("../../../templates/mysql/my.ini.j2")),
    ("redis/redis.conf", include_str!("../../../templates/redis/redis.conf.j2")),
    ("php/php.ini", include_str!("../../../templates/php/php.ini.j2")),
    ("php/php-fpm.conf", include_str!("../../../templates/php/php-fpm.conf.j2")),
];

fn env() -> &'static Environment<'static> {
    static ENV: OnceLock<Environment<'static>> = OnceLock::new();
    ENV.get_or_init(|| {
        let mut env = Environment::new();
        env.set_syntax(
            SyntaxConfig::builder()
                .trim_blocks(true)
                .lstrip_blocks(true)
                .keep_trailing_newline(true)
                .build()
                .expect("default delimiters are valid"),
        );
        env.add_filter("quote", quote);
        env.add_filter("slashes", slashes);
        for (name, source) in TEMPLATES {
            env.add_template(name, source).expect("bundled templates parse");
        }
        env
    })
}

pub fn render(name: &str, ctx: impl Serialize) -> Result<String> {
    let template = env().get_template(name).with_context(|| format!("no template named {name}"))?;
    template.render(minijinja::value::Serde(ctx)).with_context(|| format!("cannot render {name}"))
}

/// A double-quoted string, as Caddy, Nginx, Apache, and php.ini all accept.
fn quote(value: String) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Forward slashes, which every managed program accepts on Windows too.
fn slashes(value: String) -> String {
    value.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_templates_parse() {
        for (name, _) in super::TEMPLATES {
            super::env().get_template(name).unwrap();
        }
    }

    #[test]
    fn quote_escapes() {
        assert_eq!(super::quote(r#"a"b\c"#.into()), r#""a\"b\\c""#);
    }
}
