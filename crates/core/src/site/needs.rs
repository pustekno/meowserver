// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! What a project needs, read from its own files (PRD F42): the PHP
//! versions `composer.json` allows, and the database its settings name.
//! Only read; MeowServer never edits a project's configuration.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::layout::Layout;
use crate::service::{self, Kind, mysql, postgres};

use super::Site;

/// PHP lines MeowServer can install, oldest first.
pub const PHP_LINES: [(u32, u32); 12] =
    [(5, 6), (7, 0), (7, 1), (7, 2), (7, 3), (7, 4), (8, 0), (8, 1), (8, 2), (8, 3), (8, 4), (8, 5)];

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Needs {
    /// The PHP requirement as written, e.g. `^7.4|^8.0`.
    pub php_constraint: Option<String>,
    /// The lines of [`PHP_LINES`] it allows, newest first. Empty when the
    /// project says nothing, or nothing known matches.
    pub php: Vec<String>,
    pub database: Option<Database>,
    /// For a project that runs its own server: its runtime and version.
    pub runtime: Option<Runtime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Runtime {
    pub kind: Kind,
    /// As written (`>=18`, `3.12`, `>=1.22`); empty when the project does
    /// not say.
    pub constraint: String,
}

/// Lines offered when a project's runtime is not installed, newest first.
/// Node: even majors (the long-term ones).
fn candidates(kind: Kind) -> Vec<String> {
    match kind {
        Kind::Node => [26, 24, 22, 20, 18].iter().map(u32::to_string).collect(),
        Kind::Python => (9..=14).rev().map(|m| format!("3.{m}")).collect(),
        Kind::Go => (20..=27).rev().map(|m| format!("1.{m}")).collect(),
        _ => Vec::new(),
    }
}

impl Runtime {
    /// Whether `line` (`22`, `3.12`, `1.27`) fits the constraint.
    pub fn allows(&self, line: &str) -> bool {
        if self.constraint.is_empty() {
            return true;
        }
        let mut parts = line.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        let major = parts.next().unwrap_or(0);
        match parts.next() {
            Some(minor) => allows(&self.constraint, major, minor),
            // A Node major: any minor of it will do.
            None => allows(&self.constraint, major, 0) || allows(&self.constraint, major, 99),
        }
    }

    /// The newest line to install when nothing installed fits.
    pub fn suggest(&self) -> String {
        candidates(self.kind).into_iter().find(|l| self.allows(l)).unwrap_or_default()
    }
}

/// The runtime version a project asks for, from the files each ecosystem
/// uses for it.
fn runtime(project: &Path, kind: Kind) -> Runtime {
    let read = |rel: &str| std::fs::read_to_string(project.join(rel)).unwrap_or_default();
    let first_line = |text: String| text.lines().next().unwrap_or_default().trim().to_owned();
    let constraint = match kind {
        Kind::Node => {
            let pinned = first_line(read(".nvmrc")) + &first_line(read(".node-version"));
            let pinned = pinned.trim_start_matches('v');
            if pinned.starts_with(|c: char| c.is_ascii_digit()) {
                // `20` or `20.11.1`: that major.
                format!("^{}", pinned.split('.').next().unwrap_or(pinned))
            } else {
                serde_json::from_str::<serde_json::Value>(&read("package.json"))
                    .ok()
                    .and_then(|p| p.pointer("/engines/node").and_then(|v| v.as_str()).map(str::to_owned))
                    .unwrap_or_default()
            }
        }
        Kind::Python => {
            let pinned = first_line(read(".python-version"));
            if pinned.starts_with(|c: char| c.is_ascii_digit()) {
                let line: Vec<&str> = pinned.split('.').take(2).collect();
                format!("{}.*", line.join("."))
            } else {
                read("pyproject.toml")
                    .lines()
                    .find_map(|l| {
                        let (key, value) = l.split_once('=')?;
                        (key.trim() == "requires-python")
                            .then(|| value.trim().trim_matches(['"', '\'']).to_owned())
                    })
                    .unwrap_or_default()
            }
        }
        // `go 1.22` in go.mod is the oldest Go that builds the module.
        Kind::Go => read("go.mod")
            .lines()
            .find_map(|l| l.trim().strip_prefix("go ").map(|v| format!(">={}", v.trim())))
            .unwrap_or_default(),
        _ => String::new(),
    };
    Runtime { kind, constraint }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Database {
    /// MySQL-compatible (MySQL or MariaDB) or PostgreSQL.
    pub family: Family,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Mysql,
    Postgres,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Family::Mysql => "MySQL",
            Family::Postgres => "PostgreSQL",
        }
    }

    /// The servers that can serve it, the usual one first.
    pub fn kinds(self) -> &'static [Kind] {
        match self {
            Family::Mysql => &[Kind::Mysql, Kind::Mariadb],
            Family::Postgres => &[Kind::Postgres],
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Family::Mysql => 3306,
            Family::Postgres => 5432,
        }
    }
}

pub fn read(project: &Path, framework: super::Framework) -> Needs {
    let mut needs =
        Needs { runtime: framework.runtime().map(|kind| runtime(project, kind)), ..Needs::default() };
    if let Some(constraint) = php_constraint(project) {
        needs.php = PHP_LINES
            .iter()
            .rev()
            .filter(|(major, minor)| allows(&constraint, *major, *minor))
            .map(|(major, minor)| format!("{major}.{minor}"))
            .collect();
        needs.php_constraint = Some(constraint);
    }
    needs.database = database(project);
    needs
}

/// Something a site needs that MeowServer does not provide yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "problem", rename_all = "snake_case")]
pub enum Problem {
    /// No installed PHP version fits; `suggest` is the line to install.
    Php { constraint: String, running: String, suggest: String },
    /// The project uses a database server that is not installed.
    DatabaseMissing { family: Family },
    /// No installed version of the project's runtime fits.
    Runtime { kind: Kind, constraint: String, suggest: String },
    /// Installed, but nothing of MeowServer's listens on the project's port.
    DatabasePort { family: Family, port: u16, served: Vec<u16> },
}

impl Problem {
    /// What to install to fix it: (service id, version selector).
    pub fn install(&self) -> Option<(&'static str, String)> {
        match self {
            Problem::Php { suggest, .. } => Some(("php", suggest.clone())),
            Problem::DatabaseMissing { family } => Some((family.kinds()[0].id(), String::new())),
            Problem::Runtime { kind, suggest, .. } => Some((kind.id(), suggest.clone())),
            Problem::DatabasePort { .. } => None,
        }
    }

    /// In English, for the CLI and Doctor; the window translates its own.
    pub fn describe(&self, site: &str) -> String {
        match self {
            Problem::Php { constraint, running, suggest } if running == "none" => {
                format!("{site} needs PHP {constraint} and no PHP is installed (meow install php {suggest})")
            }
            Problem::Php { constraint, running, suggest } => format!(
                "{site} needs PHP {constraint} but would run on PHP {running}; install PHP {suggest} (meow install php {suggest})"
            ),
            Problem::DatabaseMissing { family } => format!(
                "{site} uses {}, which is not installed (meow install {})",
                family.name(),
                family.kinds()[0].id()
            ),
            Problem::Runtime { kind, constraint, suggest } if constraint.is_empty() => format!(
                "{site} runs on {}, which is not installed (meow install {} {suggest})",
                runtime_name(*kind),
                kind.id()
            ),
            Problem::Runtime { kind, constraint, suggest } => format!(
                "{site} needs {} {constraint}, and no installed version fits (meow install {} {suggest})",
                runtime_name(*kind),
                kind.id()
            ),
            Problem::DatabasePort { family, port, served } => format!(
                "{site} connects to {} on port {port}, but MeowServer's runs on {}; change the port in the project's \
                 settings, or run a version on {port} (meow db enable)",
                family.name(),
                served.iter().map(u16::to_string).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

pub fn runtime_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Node => "Node.js",
        Kind::Python => "Python",
        Kind::Go => "Go",
        _ => kind.id(),
    }
}

/// The newest installed line of `runtime` that fits the project.
pub fn pick_runtime(layout: &Layout, runtime: &Runtime) -> Option<String> {
    let mut installed: Vec<crate::version::Version> =
        service::installed(layout, runtime.kind).into_iter().map(|i| i.version).collect();
    installed.sort();
    installed.into_iter().rev().map(|v| line_of(runtime.kind, &v)).find(|l| runtime.allows(l))
}

/// How versions are picked: Node by major, the others by major.minor.
pub fn line_of(kind: Kind, version: &crate::version::Version) -> String {
    if kind == Kind::Node { version.major().to_string() } else { version.line() }
}

/// What `site` needs that is missing, judged from its own files.
pub fn problems(layout: &Layout, config: &Config, site: &Site) -> Vec<Problem> {
    let mut out = Vec::new();
    let needs = &site.needs;
    if let Some(runtime) = &needs.runtime
        && site.runtime.is_none()
    {
        out.push(Problem::Runtime {
            kind: runtime.kind,
            constraint: runtime.constraint.clone(),
            suggest: runtime.suggest(),
        });
    }
    if let Some(constraint) = &needs.php_constraint
        && !needs.php.is_empty()
    {
        let running = site.php.clone().or_else(|| {
            service::select(layout, Kind::Php, config.version_selector("php")).map(|i| i.version.line())
        });
        let fits = running
            .as_deref()
            .is_some_and(|r| needs.php.iter().any(|l| r == l || r.starts_with(&format!("{l}."))));
        if !fits {
            out.push(Problem::Php {
                constraint: constraint.clone(),
                running: running.unwrap_or_else(|| "none".into()),
                suggest: needs.php[0].clone(),
            });
        }
    }
    if let Some(db) = &needs.database {
        let mut served = Vec::new();
        for kind in db.family.kinds() {
            if service::select(layout, *kind, config.version_selector(kind.id())).is_none() {
                continue;
            }
            let default = if *kind == Kind::Postgres { postgres::DEFAULT_PORT } else { mysql::DEFAULT_PORT };
            served.push(config.port(kind.id(), default));
            if let Some(settings) = config.services.get(kind.id()) {
                served.extend(settings.extra.values());
            }
        }
        if served.is_empty() {
            out.push(Problem::DatabaseMissing { family: db.family });
        } else if !served.contains(&db.port) {
            out.push(Problem::DatabasePort { family: db.family, port: db.port, served });
        }
    }
    out
}

/// `config.platform.php` (the version the project is locked to) wins over
/// `require.php` (the range it accepts).
fn php_constraint(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project.join("composer.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let platform = json.pointer("/config/platform/php").and_then(|v| v.as_str());
    let require = json.pointer("/require/php").and_then(|v| v.as_str());
    // A platform version is exact (`8.1.2`); what matters is its line.
    let platform = platform.map(|v| {
        let line: Vec<&str> = v.trim().split('.').take(2).collect();
        format!("{}.*", line.join("."))
    });
    platform.or(require.map(str::to_owned)).filter(|c| !c.trim().is_empty())
}

/// Whether the Composer constraint allows some release of `major.minor`.
/// Checks the line's first and a late patch release, which is enough for
/// how projects write PHP requirements (`^7.4`, `>=8.1 <8.4`, `8.2.*`,
/// `^7.3|^8.0`, `~8.1.0`).
pub fn allows(constraint: &str, major: u32, minor: u32) -> bool {
    [0, 99].iter().any(|patch| matches(constraint, (major, minor, *patch)))
}

type V = (u32, u32, u32);

fn matches(constraint: &str, v: V) -> bool {
    constraint.split("||").flat_map(|part| part.split('|')).any(|alternative| {
        // A hyphen range: `7.4 - 8.1`.
        if let Some((low, high)) = alternative.split_once(" - ") {
            return parse(low).is_some_and(|(l, _)| v >= l)
                && parse(high).is_some_and(|(h, parts)| v < upper_of_hyphen(h, parts));
        }
        alternative
            .split([',', ' '])
            .filter(|c| !c.trim().is_empty())
            .all(|comparator| satisfies(comparator.trim(), v))
    })
}

fn satisfies(comparator: &str, v: V) -> bool {
    // `~=` is Python's compatible-release operator, like Composer's `~`.
    let comparator = comparator.replace("~=", "~");
    let comparator = comparator.as_str();
    let (op, rest) = ["^", "~", ">=", "<=", "!=", ">", "<", "=="]
        .iter()
        .find_map(|op| comparator.strip_prefix(op).map(|r| (*op, r)))
        .unwrap_or(("=", comparator));
    let rest = rest.trim().trim_start_matches(['v', 'V']);
    if rest == "*" {
        return true;
    }
    let Some((base, parts)) = parse(rest) else { return true };
    match op {
        ">=" => v >= base,
        "<=" => v <= base,
        ">" => v > base,
        "<" => v < base,
        "!=" => v != base,
        "^" => v >= base && v < caret_upper(base),
        "~" => v >= base && v < tilde_upper(base, parts),
        // `8.2`, `8.2.*`, `8.2.3`: within what is written.
        _ => v >= base && v < wildcard_upper(base, parts),
    }
}

/// `7.4.1` → ((7,4,1), 3); `8.*` → ((8,0,0), 1). Returns how many numbers
/// were given, which decides ranges.
fn parse(text: &str) -> Option<(V, usize)> {
    let mut numbers = Vec::new();
    for piece in text.trim().split('.') {
        if piece == "*" || piece.eq_ignore_ascii_case("x") {
            break;
        }
        let digits: String = piece.chars().take_while(char::is_ascii_digit).collect();
        numbers.push(digits.parse::<u32>().ok()?);
        if digits.len() != piece.len() {
            break; // `8.2.0-beta`, `8.2@dev`
        }
    }
    let get = |i: usize| numbers.get(i).copied().unwrap_or(0);
    (!numbers.is_empty()).then(|| ((get(0), get(1), get(2)), numbers.len().min(3)))
}

fn caret_upper((major, minor, patch): V) -> V {
    match (major, minor) {
        (0, 0) => (0, 0, patch + 1),
        (0, _) => (0, minor + 1, 0),
        _ => (major + 1, 0, 0),
    }
}

fn tilde_upper((major, minor, _): V, parts: usize) -> V {
    if parts >= 3 { (major, minor + 1, 0) } else { (major + 1, 0, 0) }
}

fn wildcard_upper((major, minor, patch): V, parts: usize) -> V {
    match parts {
        1 => (major + 1, 0, 0),
        2 => (major, minor + 1, 0),
        _ => (major, minor, patch + 1),
    }
}

fn upper_of_hyphen(high: V, parts: usize) -> V {
    if parts >= 3 { (high.0, high.1, high.2 + 1) } else { wildcard_upper(high, parts) }
}

/// The database the project's settings point at.
fn database(project: &Path) -> Option<Database> {
    let env = std::fs::read_to_string(project.join(".env")).unwrap_or_default();
    let value = |key: &str| -> Option<String> {
        env.lines().find_map(|line| {
            let (k, v) = line.trim().split_once('=')?;
            (k.trim() == key)
                .then(|| v.split(" #").next().unwrap_or(v).trim().trim_matches(['"', '\'']).to_owned())
        })
    };
    let family_of = |driver: &str| match driver.to_ascii_lowercase().as_str() {
        "mysql" | "mariadb" | "mysqli" | "pdo_mysql" => Some(Family::Mysql),
        "pgsql" | "postgres" | "postgre" | "postgresql" | "pdo_pgsql" => Some(Family::Postgres),
        _ => None,
    };
    let port_or = |family: Family, port: Option<String>| -> u16 {
        port.and_then(|p| p.parse().ok()).unwrap_or(family.default_port())
    };

    // Laravel.
    if let Some(family) = value("DB_CONNECTION").as_deref().and_then(family_of) {
        return Some(Database { family, port: port_or(family, value("DB_PORT")) });
    }
    // CodeIgniter 4.
    if let Some(family) = value("database.default.DBDriver").as_deref().and_then(family_of) {
        return Some(Database { family, port: port_or(family, value("database.default.port")) });
    }
    // Symfony and others: DATABASE_URL=postgresql://user:pass@127.0.0.1:5432/app
    if let Some(url) = value("DATABASE_URL") {
        let (scheme, rest) = url.split_once("://")?;
        let family = family_of(scheme)?;
        let host = rest.rsplit_once('@').map_or(rest, |(_, h)| h);
        let host = host.split(['/', '?']).next().unwrap_or(host);
        let port = host.rsplit_once(':').map(|(_, p)| p.to_owned());
        return Some(Database { family, port: port_or(family, port) });
    }
    // WordPress: MySQL, with an optional port after the host.
    if let Ok(config) = std::fs::read_to_string(project.join("wp-config.php")) {
        let host = config.lines().find(|l| l.contains("'DB_HOST'") || l.contains("\"DB_HOST\""));
        let port = host.and_then(|l| {
            let quoted: Vec<&str> = l.split(['\'', '"']).collect();
            quoted.get(3).and_then(|h| h.rsplit_once(':')).map(|(_, p)| p.to_owned())
        });
        return Some(Database { family: Family::Mysql, port: port_or(Family::Mysql, port) });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(constraint: &str) -> Vec<String> {
        PHP_LINES
            .iter()
            .filter(|(a, b)| allows(constraint, *a, *b))
            .map(|(a, b)| format!("{a}.{b}"))
            .collect()
    }

    #[test]
    fn composer_php_constraints() {
        assert_eq!(lines("^7.4"), ["7.4"]);
        assert_eq!(lines("^8.1"), ["8.1", "8.2", "8.3", "8.4", "8.5"]);
        assert_eq!(lines("^7.3|^8.0"), ["7.3", "7.4", "8.0", "8.1", "8.2", "8.3", "8.4", "8.5"]);
        assert_eq!(lines(">=8.1 <8.4"), ["8.1", "8.2", "8.3"]);
        assert_eq!(lines(">=7.2,<8.0"), ["7.2", "7.3", "7.4"]);
        assert_eq!(lines("8.2.*"), ["8.2"]);
        assert_eq!(lines("~8.1.0"), ["8.1"]);
        assert_eq!(lines("~7.4"), ["7.4"]);
        assert_eq!(lines("7.4 - 8.1"), ["7.4", "8.0", "8.1"]);
        assert_eq!(
            lines(">=5.6"),
            ["5.6", "7.0", "7.1", "7.2", "7.3", "7.4", "8.0", "8.1", "8.2", "8.3", "8.4", "8.5"]
        );
        assert_eq!(lines("^8.2 || ^8.3"), ["8.2", "8.3", "8.4", "8.5"]);
    }

    fn project(files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("meow-needs-{}-{}", std::process::id(), files[0].1.len()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
        dir
    }

    #[test]
    fn reads_laravel_needs() {
        let dir = project(&[
            ("composer.json", r#"{"require": {"php": "^7.4", "laravel/framework": "^8.0"}}"#),
            (".env", "APP_NAME=Shop\nDB_CONNECTION=pgsql\nDB_HOST=127.0.0.1\nDB_PORT=5433\n"),
        ]);
        let needs = read(&dir, crate::site::Framework::Php);
        assert_eq!(needs.php, ["7.4"]);
        assert_eq!(needs.database, Some(Database { family: Family::Postgres, port: 5433 }));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn platform_php_and_other_frameworks() {
        let dir = project(&[
            ("composer.json", r#"{"require": {"php": ">=7.2"}, "config": {"platform": {"php": "8.1.2"}}}"#),
            (".env", "database.default.DBDriver = MySQLi\ndatabase.default.port = 3307\n"),
        ]);
        let needs = read(&dir, crate::site::Framework::Php);
        assert_eq!(needs.php, ["8.1"], "the locked platform version wins");
        assert_eq!(needs.database, Some(Database { family: Family::Mysql, port: 3307 }));
        std::fs::remove_dir_all(&dir).unwrap();

        let dir = project(&[(
            ".env",
            "DATABASE_URL=\"postgresql://app:secret@127.0.0.1:5432/app?serverVersion=16\"\n",
        )]);
        assert_eq!(
            read(&dir, crate::site::Framework::Php).database,
            Some(Database { family: Family::Postgres, port: 5432 })
        );
        std::fs::remove_dir_all(&dir).unwrap();

        let dir = project(&[("wp-config.php", "<?php\ndefine( 'DB_HOST', 'localhost:3308' );\n")]);
        assert_eq!(
            read(&dir, crate::site::Framework::Php).database,
            Some(Database { family: Family::Mysql, port: 3308 })
        );
        assert!(read(&dir, crate::site::Framework::Php).php.is_empty(), "WordPress has no composer.json");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn runtime_versions_from_project_files() {
        let check = |files: &[(&str, &str)], framework, yes: &[&str], no: &[&str]| {
            let dir = project(files);
            let runtime = read(&dir, framework).runtime.expect("a runtime");
            for line in yes {
                assert!(runtime.allows(line), "{files:?} should allow {line}");
            }
            for line in no {
                assert!(!runtime.allows(line), "{files:?} should not allow {line}");
            }
            std::fs::remove_dir_all(&dir).unwrap();
        };
        use crate::site::Framework::{Django, Go, Node};
        check(
            &[
                (
                    ".nvmrc",
                    "v20.11.1
",
                ),
                ("package.json", "{}"),
            ],
            Node,
            &["20"],
            &["18", "22"],
        );
        check(&[("package.json", r#"{"engines": {"node": ">=18"}}"#)], Node, &["18", "22", "24"], &["16"]);
        check(
            &[("package.json", r#"{"engines": {"node": "^20 || ^22"}}"#)],
            Node,
            &["20", "22"],
            &["18", "24"],
        );
        check(
            &[(
                ".python-version",
                "3.12.4
",
            )],
            Django,
            &["3.12"],
            &["3.11", "3.13"],
        );
        check(
            &[(
                "pyproject.toml",
                "[project]
requires-python = \">=3.10,<3.13\"
",
            )],
            Django,
            &["3.10", "3.12"],
            &["3.9", "3.13"],
        );
        check(
            &[(
                "pyproject.toml",
                "requires-python = \"~=3.11\"
",
            )],
            Django,
            &["3.11", "3.14"],
            &["3.10"],
        );
        check(
            &[(
                "go.mod",
                "module app

go 1.22.3
",
            )],
            Go,
            &["1.22", "1.27"],
            &["1.21"],
        );
        // Nothing said: any version will do, and the newest is offered.
        let dir = project(&[("package.json", "{}")]);
        let runtime = read(&dir, Node).runtime.unwrap();
        assert!(runtime.allows("24"));
        assert_eq!(runtime.suggest(), "26");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
