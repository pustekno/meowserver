# 0006 — Where Xdebug comes from

Status: accepted (2026-10-25)

## Context

PRD F20 includes an Xdebug toggle; Phase 0 had to find a source.

## Findings

- xdebug.org publishes Windows DLLs for every supported PHP line, matching
  php.net's builds:
  `https://xdebug.org/files/php_xdebug-<xdebug>-<php major.minor>-nts-<vs16|vs17>-x86_64.dll`
  (e.g. `php_xdebug-3.5.3-8.4-nts-vs17-x86_64.dll`). The download page lists
  them; no checksums are published.
- static-php-cli builds (macOS/Linux) are fully static and cannot load
  shared extensions.

## Decision

- **Windows:** download the DLL matching the PHP line and the compiler tag
  (`vs16`/`vs17`, taken from the PHP build) into that version's `ext/`.
  Marked "no official checksum" like other unchecked downloads.
- **macOS/Linux:** Xdebug is offered only for system PHP installs
  (Homebrew/apt), where it is installed with the system's package manager;
  MeowServer only toggles the `zend_extension` line.
