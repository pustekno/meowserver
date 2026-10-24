# Security policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report privately through GitHub:
https://github.com/pustekno/meowserver/security/advisories/new

Include what you found, how to reproduce it, and which OS and MeowServer
version you used. You will get a reply within 7 days. Once a fix is released,
the advisory is published with credit to you unless you prefer otherwise.

## Scope

MeowServer is a **local development tool**, not a production server. Reports
are most useful when they concern:

- a service reachable from the network when LAN access is off;
- the elevated helper running anything other than its fixed commands;
- downloads installed without their official checksum being verified, or
  archives writing outside their target folder;
- the local certificate authority being usable for names other than local
  development sites;
- another local user being able to control MeowServer through its IPC
  channel.

## Supported versions

Before v1.0, only the latest pre-release receives fixes.
