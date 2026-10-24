# 0005 — Local DNS resolver (PRD F17)

Status: **proposed — needs a manual test with administrator rights**

## Context

Windows does not resolve `*.localhost` at the OS level (PRD §9.6), so hosts
entries are written per site, which costs an elevation prompt each time.
A resolver run by `meowd`, with the OS pointed at it once, would remove
those prompts.

## Findings so far (Windows 11 Home Single Language)

- `Add-DnsClientNrptRule` and `Get-DnsClientNrptPolicy` are present and
  work without errors on the Home edition; the DNS Client service runs.
- Whether the DNS Client **honours** a local NRPT rule on Home has not been
  verified: adding a rule needs administrator rights.

## Test procedure

1. Run a DNS responder on `127.0.0.1:53` answering `*.localhost` with
   `127.0.0.1` (`meowd` will provide this as `meow dns serve --test`).
2. In an elevated PowerShell:
   `Add-DnsClientNrptRule -Namespace ".localhost" -NameServers "127.0.0.1"`
3. In a normal PowerShell:
   `[System.Net.Dns]::GetHostAddresses("probe.localhost")`,
   `php -r 'var_dump(gethostbyname("probe.localhost"));'`,
   `node -e "require('dns').lookup('probe.localhost',console.log)"`.
4. Remove the rule: `Get-DnsClientNrptRule | ? Namespace -eq ".localhost" |
   Remove-DnsClientNrptRule -Force`.

## Decision

Pending. If all three lookups in step 3 return `127.0.0.1`, F17 ships in
v1.0 on Windows; otherwise Windows keeps hosts entries (F16).
