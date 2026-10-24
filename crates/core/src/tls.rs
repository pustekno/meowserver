// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Local HTTPS (PRD F18, §8.3). One certificate authority per MeowServer
//! folder signs one certificate that lists every site. The CA carries X.509
//! name constraints, so even a leaked key cannot vouch for real websites:
//! only `localhost`, `*.localhost`, `*.test`, and loopback addresses.

use std::net::Ipv6Addr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use rcgen::{
    BasicConstraints, CertificateParams, CidrSubnet, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
    GeneralSubtree, IsCa, Issuer, KeyPair, KeyUsagePurpose, NameConstraints,
};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::layout::Layout;
use crate::platform::{Os, os};

/// Certificates issued for sites are renewed this long before they expire.
const RENEW_BEFORE: Duration = Duration::days(30);
/// 397 days: within what every browser accepts for a server certificate.
const SITE_VALIDITY: Duration = Duration::days(397);
const CA_VALIDITY: Duration = Duration::days(3650);

/// Paths the web servers need.
#[derive(Debug, Clone, Serialize)]
pub struct Files {
    pub cert: PathBuf,
    pub key: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct CaInfo {
    /// Unique per MeowServer folder, so several installs never clash in a
    /// trust store.
    common_name: String,
}

#[derive(Serialize, Deserialize, PartialEq)]
struct SiteInfo {
    names: Vec<String>,
    not_after: i64,
}

fn dir(layout: &Layout) -> PathBuf {
    layout.ssl()
}

pub fn ca_cert_path(layout: &Layout) -> PathBuf {
    dir(layout).join("meowserver-ca.crt")
}

fn ca_key_path(layout: &Layout) -> PathBuf {
    dir(layout).join("meowserver-ca.key")
}

/// Makes sure a CA exists and a site certificate covers `hosts` (plus
/// `localhost` and loopback). Reissues only when the names change or the
/// certificate is close to expiring.
pub fn ensure(layout: &Layout, hosts: &[String]) -> Result<Files> {
    let dir = dir(layout);
    std::fs::create_dir_all(&dir)?;
    let (ca_params, ca_key) = load_or_create_ca(layout)?;

    let mut names: Vec<String> = vec!["localhost".into(), "127.0.0.1".into(), "::1".into()];
    names.extend(hosts.iter().cloned());
    names.sort();
    names.dedup();

    let files = Files { cert: dir.join("sites.crt"), key: dir.join("sites.key") };
    let info_path = dir.join("sites.json");
    let current: Option<SiteInfo> =
        std::fs::read(&info_path).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let now = OffsetDateTime::now_utc();
    let fresh = current.as_ref().is_some_and(|c| {
        c.names == names
            && OffsetDateTime::from_unix_timestamp(c.not_after).is_ok_and(|t| t - RENEW_BEFORE > now)
    });
    if fresh && files.cert.exists() && files.key.exists() {
        return Ok(files);
    }

    let mut params = CertificateParams::new(names.clone())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::OrganizationName, "MeowServer");
    dn.push(DnType::CommonName, "MeowServer local sites");
    params.distinguished_name = dn;
    params.not_before = now - Duration::days(1);
    params.not_after = now + SITE_VALIDITY;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    let key = KeyPair::generate()?;
    let issuer = Issuer::new(ca_params, ca_key);
    let cert = params.signed_by(&key, &issuer)?;

    // Full chain, so servers that send it let clients build the path.
    let ca_pem = std::fs::read_to_string(ca_cert_path(layout))?;
    write_private(&files.key, key.serialize_pem().as_bytes())?;
    std::fs::write(&files.cert, format!("{}{}", cert.pem(), ca_pem))?;
    let info = SiteInfo { names, not_after: params.not_after.unix_timestamp() };
    std::fs::write(&info_path, serde_json::to_vec_pretty(&info)?)?;
    Ok(files)
}

/// The CA's parameters are rebuilt the same way every time; only its
/// unique name is stored. That is all rcgen needs to sign with it.
fn ca_params(common_name: &str, not_before: OffsetDateTime) -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::OrganizationName, "MeowServer");
    dn.push(DnType::CommonName, common_name);
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.not_before = not_before;
    params.not_after = not_before + CA_VALIDITY;
    params.name_constraints = Some(NameConstraints {
        permitted_subtrees: vec![
            GeneralSubtree::DnsName("localhost".into()),
            GeneralSubtree::DnsName("test".into()),
            GeneralSubtree::IpAddress(CidrSubnet::V4([127, 0, 0, 0], [255, 0, 0, 0])),
            GeneralSubtree::IpAddress(CidrSubnet::V6(Ipv6Addr::LOCALHOST.octets(), [0xff; 16])),
        ],
        excluded_subtrees: Vec::new(),
    });
    params
}

fn load_or_create_ca(layout: &Layout) -> Result<(CertificateParams, KeyPair)> {
    let info_path = dir(layout).join("ca.json");
    if let (Ok(info), Ok(key)) = (std::fs::read(&info_path), std::fs::read_to_string(ca_key_path(layout))) {
        let info: CaInfo = serde_json::from_slice(&info).context("ssl/ca.json is damaged")?;
        let key = KeyPair::from_pem(&key).context("the CA key is damaged")?;
        return Ok((ca_params(&info.common_name, OffsetDateTime::now_utc()), key));
    }
    let suffix = crate::http::to_hex(&sha_of(layout.root().to_string_lossy().as_bytes()))[..8].to_owned();
    let common_name = format!("MeowServer local CA {suffix}");
    let now = OffsetDateTime::now_utc() - Duration::days(1);
    let params = ca_params(&common_name, now);
    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    write_private(&ca_key_path(layout), key.serialize_pem().as_bytes())?;
    std::fs::write(ca_cert_path(layout), cert.pem())?;
    std::fs::write(&info_path, serde_json::to_vec_pretty(&CaInfo { common_name })?)?;
    // A new CA invalidates any certificate signed by an old one.
    let _ = std::fs::remove_file(dir(layout).join("sites.json"));
    Ok((params, key))
}

fn sha_of(bytes: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.update(OffsetDateTime::now_utc().unix_timestamp().to_le_bytes());
    h.finalize().to_vec()
}

/// Private keys are readable by the current user only where the OS lets
/// us say so simply (Unix). On Windows the folder's ACL applies.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn trusted_marker(layout: &Layout) -> PathBuf {
    dir(layout).join("trusted")
}

pub fn is_trusted(layout: &Layout) -> bool {
    trusted_marker(layout).exists()
}

/// Adds the CA to the user's trust store (Windows, macOS) or the system
/// store (Linux, elevated). Windows and macOS show their own confirmation.
pub fn trust(layout: &Layout) -> Result<()> {
    let ca = ca_cert_path(layout);
    if !ca.exists() {
        load_or_create_ca(layout)?;
    }
    let ok = match os() {
        Os::Windows => run("certutil", &["-user", "-addstore", "Root", &ca.to_string_lossy()]),
        Os::MacOs => {
            let home = std::env::var("HOME").unwrap_or_default();
            let keychain = format!("{home}/Library/Keychains/login.keychain-db");
            run("security", &["add-trusted-cert", "-r", "trustRoot", "-k", &keychain, &ca.to_string_lossy()])
        }
        Os::Linux => {
            let meow = std::env::current_exe()?.with_file_name("meow");
            let args = vec![
                "privileged".into(),
                "ca-install".into(),
                "--home".into(),
                layout.root().display().to_string(),
            ];
            crate::platform::run_elevated(&meow, &args).is_ok_and(|c| c == 0)
        }
    };
    if !ok {
        bail!("the certificate was not added (declined, or the tool failed)");
    }
    nss_add(&ca);
    std::fs::write(trusted_marker(layout), b"")?;
    Ok(())
}

/// Removes the CA from trust stores (PRD F36).
pub fn untrust(layout: &Layout) -> Result<()> {
    let name = std::fs::read(dir(layout).join("ca.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<CaInfo>(&b).ok())
        .map(|i| i.common_name)
        .context("no MeowServer CA here")?;
    match os() {
        Os::Windows => {
            run("certutil", &["-user", "-delstore", "Root", &name]);
        }
        Os::MacOs => {
            run("security", &["delete-certificate", "-c", &name]);
        }
        Os::Linux => {
            let meow = std::env::current_exe()?.with_file_name("meow");
            let args = vec![
                "privileged".into(),
                "ca-remove".into(),
                "--home".into(),
                layout.root().display().to_string(),
            ];
            let _ = crate::platform::run_elevated(&meow, &args);
        }
    }
    let _ = std::fs::remove_file(trusted_marker(layout));
    Ok(())
}

/// Linux system store; run elevated by `meow privileged ca-install`.
pub fn install_system_ca(layout: &Layout, add: bool) -> Result<()> {
    let target = Path::new("/usr/local/share/ca-certificates/meowserver.crt");
    if add {
        std::fs::copy(ca_cert_path(layout), target)?;
    } else {
        let _ = std::fs::remove_file(target);
    }
    if !run("update-ca-certificates", &[]) {
        bail!("update-ca-certificates failed");
    }
    Ok(())
}

/// Firefox and Chromium on Linux read the NSS store; add the CA there too
/// when `certutil` (from libnss3-tools) is available. Best effort.
fn nss_add(ca: &Path) {
    if os() != Os::Linux {
        return;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let db = format!("sql:{home}/.pki/nssdb");
    run("certutil", &["-A", "-d", &db, "-t", "C,,", "-n", "MeowServer", "-i", &ca.to_string_lossy()]);
}

fn run(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_and_reuses_site_certificate() {
        let root = std::env::temp_dir().join(format!("meow-tls-{}", std::process::id()));
        let layout = Layout::at(root.clone());
        let first = ensure(&layout, &["blog.localhost".into()]).unwrap();
        let pem = std::fs::read_to_string(&first.cert).unwrap();
        assert_eq!(pem.matches("BEGIN CERTIFICATE").count(), 2, "leaf + CA");

        ensure(&layout, &["blog.localhost".into()]).unwrap();
        assert_eq!(std::fs::read_to_string(&first.cert).unwrap(), pem, "same names: reused");

        ensure(&layout, &["blog.localhost".into(), "shop.test".into()]).unwrap();
        assert_ne!(std::fs::read_to_string(&first.cert).unwrap(), pem, "new name: reissued");
        std::fs::remove_dir_all(root).unwrap();
    }
}
