# 0006 — Where Xdebug comes from

Status: accepted (2026-10-25), updated after implementation

## Context

PRD F20 includes an Xdebug toggle; Phase 0 had to find a source.

## Findings

- xdebug.org publishes Windows DLLs for every supported PHP line on
  `https://xdebug.org/download/historical`, each link carrying its SHA-256
  in the `title` attribute:
  `<a title="SHA256:&nbsp;<hash>" href='/files/php_xdebug-…dll'>`.
- File names differ by era: `php_xdebug-3.5.3-8.4-nts-vs17-x86_64.dll` for
  PHP 8, `php_xdebug-3.1.6-7.4-vc15-nts-x86_64.dll` for PHP 7.4.
- The server breaks on reused HTTP connections ("invalid HTTP version" on
  the second request), so MeowServer's HTTP client does not keep
  connections alive.
- static-php-cli builds (macOS/Linux) are fully static and cannot load
  shared extensions.

## Decision

- **Windows:** download the newest stable non-thread-safe x64 DLL for the
  PHP line into that version's `ext/php_xdebug.dll`, verified against the
  published SHA-256. Loaded as a `zend_extension` only while
  `php.xdebug = true` (off by default), with `xdebug.mode=debug,develop`,
  `start_with_request=trigger`, client port 9003.
- **macOS/Linux:** Xdebug is offered only for system PHP installs
  (Homebrew/apt), installed with the system's package manager.

Verified on Windows 11: Xdebug 3.5.3 on PHP 8.4.27 and 3.1.6 on PHP 7.4.33,
toggled on and off from the CLI and the window.
