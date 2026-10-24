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
    if !has("index.php") && (has("index.html") || has("index.htm")) {
        return (Framework::Static, project.to_path_buf());
    }
    (Framework::Php, project.to_path_buf())
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
}
