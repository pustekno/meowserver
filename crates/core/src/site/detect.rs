// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Framework and document root detection (PRD F10).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    Laravel,
    Symfony,
    CodeIgniter,
    Yii,
    Craft,
    CakePhp,
    WordPress,
    Php,
    Static,
    Proxy,
    // Projects that run their own server (PRD F43); MeowServer starts it on
    // request and puts the site's name in front of it.
    Next,
    Nuxt,
    Vite,
    Node,
    Django,
    Flask,
    FastApi,
    Go,
}

impl Framework {
    /// The runtime of a project that runs its own server.
    pub fn runtime(self) -> Option<crate::service::Kind> {
        use crate::service::Kind;
        match self {
            Framework::Next | Framework::Nuxt | Framework::Vite | Framework::Node => Some(Kind::Node),
            Framework::Django | Framework::Flask | Framework::FastApi => Some(Kind::Python),
            Framework::Go => Some(Kind::Go),
            _ => None,
        }
    }
}

/// Returns the framework and the folder to serve. Rules are checked in
/// order; the first match wins.
pub fn detect(project: &Path) -> (Framework, PathBuf) {
    let has = |rel: &str| project.join(rel).exists();
    let rules: [(bool, Framework, &str); 7] = [
        (has("artisan") && has("public/index.php"), Framework::Laravel, "public"),
        (has("bin/console") && has("public/index.php"), Framework::Symfony, "public"),
        (has("spark") && has("public/index.php"), Framework::CodeIgniter, "public"),
        (has("craft") && has("web/index.php"), Framework::Craft, "web"),
        (has("yii") && has("web/index.php"), Framework::Yii, "web"),
        (has("webroot/index.php"), Framework::CakePhp, "webroot"),
        (has("wp-load.php") || has("wp-config.php"), Framework::WordPress, ""),
    ];
    for (matched, framework, root) in rules {
        if matched {
            let docroot = if root.is_empty() { project.to_path_buf() } else { project.join(root) };
            return (framework, docroot);
        }
    }
    if has("public/index.php") {
        return (Framework::Php, project.join("public"));
    }
    if !has("index.php")
        && let Some(app) = app(project)
    {
        return (app, project.to_path_buf());
    }
    if !has("index.php") && (has("index.html") || has("index.htm")) {
        return (Framework::Static, project.to_path_buf());
    }
    (Framework::Php, project.to_path_buf())
}

/// A project that runs its own development server.
fn app(project: &Path) -> Option<Framework> {
    let read = |rel: &str| std::fs::read_to_string(project.join(rel)).unwrap_or_default().to_lowercase();
    if project.join("go.mod").is_file() {
        return Some(Framework::Go);
    }
    if project.join("manage.py").is_file() {
        return Some(Framework::Django);
    }
    let python = [read("requirements.txt"), read("pyproject.toml"), read("Pipfile")].join("\n");
    if python.contains("fastapi") {
        return Some(Framework::FastApi);
    }
    if python.contains("flask") {
        return Some(Framework::Flask);
    }
    // As written: JSON keys such as `devDependencies` are case-sensitive.
    let package = std::fs::read_to_string(project.join("package.json")).ok()?;
    let package: serde_json::Value = serde_json::from_str(&package).ok()?;
    let scripts = package.get("scripts")?;
    if scripts.get("dev").is_none() && scripts.get("start").is_none() {
        return None;
    }
    let uses = |name: &str| {
        ["dependencies", "devDependencies"]
            .iter()
            .any(|k| package.get(*k).and_then(|d| d.get(name)).is_some())
    };
    Some(if uses("next") {
        Framework::Next
    } else if uses("nuxt") {
        Framework::Nuxt
    } else if uses("vite") {
        Framework::Vite
    } else {
        Framework::Node
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "meow-detect-{}-{}",
            std::process::id(),
            files.join("_").replace(['/', '.'], "-")
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for f in files {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
        dir
    }

    #[test]
    fn detects_common_frameworks() {
        let cases: &[(&[&str], Framework, &str)] = &[
            (&["artisan", "public/index.php"], Framework::Laravel, "public"),
            (&["spark", "public/index.php"], Framework::CodeIgniter, "public"),
            (&["wp-load.php", "index.php"], Framework::WordPress, ""),
            (&["webroot/index.php"], Framework::CakePhp, "webroot"),
            (&["index.html"], Framework::Static, ""),
            (&["index.php"], Framework::Php, ""),
        ];
        for (files, framework, root) in cases {
            let dir = project(files);
            let (got, docroot) = detect(&dir);
            assert_eq!(got, *framework, "{files:?}");
            let expected = if root.is_empty() { dir.clone() } else { dir.join(root) };
            assert_eq!(docroot, expected, "{files:?}");
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    fn with(files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "meow-app-{}-{}",
            std::process::id(),
            files[0].0.replace(['/', '.'], "-")
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for (name, body) in files {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        dir
    }

    #[test]
    fn projects_with_their_own_server() {
        let cases: [(&[(&str, &str)], Framework); 7] = [
            (
                &[("package.json", r#"{"scripts": {"dev": "next dev"}, "dependencies": {"next": "15"}}"#)],
                Framework::Next,
            ),
            (
                &[("package.json", r#"{"scripts": {"dev": "vite"}, "devDependencies": {"vite": "7"}}"#)],
                Framework::Vite,
            ),
            (&[("package.json", r#"{"scripts": {"start": "node index.js"}}"#)], Framework::Node),
            (&[("manage.py", "")], Framework::Django),
            (
                &[(
                    "requirements.txt",
                    "fastapi
uvicorn
",
                )],
                Framework::FastApi,
            ),
            (
                &[(
                    "pyproject.toml",
                    "[project]
dependencies = [\"Flask>=3\"]
",
                )],
                Framework::Flask,
            ),
            (
                &[(
                    "go.mod",
                    "module app

go 1.22
",
                )],
                Framework::Go,
            ),
        ];
        for (files, expected) in cases {
            let dir = with(files);
            assert_eq!(detect(&dir).0, expected, "{files:?}");
            std::fs::remove_dir_all(&dir).unwrap();
        }
        // Laravel ships a package.json for Vite; it stays a PHP site.
        let dir = with(&[
            ("artisan", ""),
            ("public/index.php", ""),
            ("package.json", r#"{"scripts": {"dev": "vite"}, "devDependencies": {"vite": "7"}}"#),
        ]);
        assert_eq!(detect(&dir).0, Framework::Laravel);
        std::fs::remove_dir_all(&dir).unwrap();
        // A package.json without a server script is not an app.
        let dir = with(&[("package.json", r#"{"scripts": {"build": "tsc"}}"#)]);
        assert_eq!(detect(&dir).0, Framework::Php);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
