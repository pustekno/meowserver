# 0001 — Rust as the implementation language

Status: accepted (2026-10-25)

## Context

MeowServer must be lighter than the services it manages, run on Windows,
macOS, and Linux, and have a native GUI without a webview (PRD §3, §9.1).

## Decision

Rust, edition 2024, stable toolchain.

## Consequences

- No garbage collector or runtime; Phase 0 measured the supervisor + tray at
  13.8 MB working set and 2.6 MB private memory (ADR 0003).
- Mature crates exist for everything needed: tray (`tray-icon`), HTTP
  (`ureq`), archives (`zip`, `tar`), IPC (`interprocess`), GUI (ADR 0002).
- Steeper learning curve for contributors than Go. Mitigated by keeping
  service configs as plain templates in `templates/` so many fixes need no
  Rust at all.
