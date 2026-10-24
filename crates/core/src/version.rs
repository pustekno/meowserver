// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Dotted numeric versions such as `8.4.27`, compared component by component.

use std::fmt;
use std::str::FromStr;

use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(Vec<u32>);

impl Version {
    /// `major.minor`, the unit users pick versions by (PRD F2).
    pub fn line(&self) -> String {
        self.0.iter().take(2).map(u32::to_string).collect::<Vec<_>>().join(".")
    }

    pub fn major(&self) -> u32 {
        self.0[0]
    }

    /// True if `selector` (e.g. `8`, `8.4`, `8.4.27`) is a prefix of this version.
    pub fn matches(&self, selector: &str) -> bool {
        match selector.parse::<Version>() {
            Ok(sel) => self.0.starts_with(&sel.0),
            Err(_) => false,
        }
    }
}

impl FromStr for Version {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim().trim_start_matches('v');
        let parts = s.split('.').map(|p| p.parse::<u32>()).collect::<Result<Vec<_>, _>>();
        match parts {
            Ok(parts) if !parts.is_empty() => Ok(Version(parts)),
            _ => bail!("not a version: {s:?}"),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(u32::to_string).collect();
        f.write_str(&parts.join("."))
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// The highest version matching `selector`, or the highest overall without one.
pub fn pick<'a>(
    versions: impl IntoIterator<Item = &'a Version>,
    selector: Option<&str>,
) -> Option<&'a Version> {
    versions.into_iter().filter(|v| selector.is_none_or(|s| v.matches(s))).max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        s.parse().unwrap()
    }

    #[test]
    fn orders_numerically() {
        assert!(v("8.10.0") > v("8.9.30"));
        assert!(v("2.11.7") > v("2.11"));
    }

    #[test]
    fn strips_leading_v() {
        assert_eq!(v("v24.1.0"), v("24.1.0"));
    }

    #[test]
    fn matches_by_prefix() {
        assert!(v("8.4.27").matches("8.4"));
        assert!(v("8.4.27").matches("8"));
        assert!(!v("8.4.27").matches("8.3"));
        assert!(!v("8.40.1").matches("8.4"));
    }

    #[test]
    fn picks_highest_match() {
        let all = [v("7.4.33"), v("8.3.20"), v("8.4.1"), v("8.4.27")];
        assert_eq!(pick(&all, Some("8.4")), Some(&v("8.4.27")));
        assert_eq!(pick(&all, None), Some(&v("8.4.27")));
        assert_eq!(pick(&all, Some("8.2")), None);
    }

    #[test]
    fn line_is_major_minor() {
        assert_eq!(v("8.4.27").line(), "8.4");
        assert_eq!(v("17").line(), "17");
    }
}
