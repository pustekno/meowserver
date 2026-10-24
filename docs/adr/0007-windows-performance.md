# 0007 — Why local sites felt slow in Laragon, and what MeowServer does instead

Status: accepted (2026-10-25)

## Context

The owner's team reported Laragon sites (Laravel and WordPress) loading
slowly, worst on the first page after a restart: 36–90 s once, several
seconds often. An investigation on the owner's laptop (Windows 11,
Laragon 8.6.1, Apache 2.4.66 + `mod_fcgid`, PHP 8.4.12, MySQL 8.4.3) found:

1. **Another server held port 80 on 127.0.0.1.** A forgotten
   `php spark serve --port 80` bound 127.0.0.1:80 while Apache held
   0.0.0.0:80. On Windows the more specific bind wins silently, so every
   `*.test` site went to PHP's single-threaded built-in server.
2. **OPcache was off** in every Laragon PHP: `php_opcache.dll` existed but
   was never loaded, so every request recompiled every file.
3. **`mod_fcgid` stops idle `php-cgi`** after 300 s (`FcgidIdleTimeout`), so
   the first request after a short break starts cold again.
4. **Microsoft Defender** real-time scanning reads every file PHP opens on
   first use after a reboot; a Laravel `vendor/` has thousands. Not
   measured (needs administrator rights), but consistent with the symptoms
   and with `composer create-project laravel/laravel` taking 10 minutes on
   the same machine.

## Decision

| Cause | MeowServer |
|---|---|
| Port taken on another address | Before starting a service, every listener on its port is checked, on all addresses, and the owning program is named (`engine::ensure_port_free`, `meow doctor`). |
| OPcache off | On by default, sized for large projects: 256 MB, 50,000 files, 32 MB interned strings; `validate_timestamps=1`, `revalidate_freq=0` so edits show immediately. |
| Cold start after a reboot | `opcache.file_cache` in `data/php/opcache/<line>` keeps compiled scripts on disk across restarts. |
| Idle PHP stopped | The `php-cgi` pool never stops for idleness (ADR 0004). |
| Defender scanning | `meow defender exclude` adds exclusions for the MeowServer folder and project folders after one elevation prompt; `meow doctor` suggests it when real-time protection is on; undone by "Undo system changes". Dev Drive is suggested as an alternative. |
| Slow Windows file lookups | `realpath_cache_size=4096K`, `realpath_cache_ttl=600`. |

## Measurement

Fresh Laravel 13 (`composer create-project`), welcome page, MeowServer with
Caddy 2.11.7 and PHP 8.4.27 (4 `php-cgi`), 5 requests each, seconds:

| Scenario | 1st | 2nd–5th |
|---|---|---|
| OPcache off (Laragon's setting) | 0.35 | 0.13–0.14 |
| OPcache on, empty disk cache (very first start) | 0.91 | 0.02–0.05 |
| **OPcache on, after restarting MeowServer** | **0.08** | 0.02–0.04 |

The disk cache makes the first request after a restart about 10× faster
than a cold start, and steady-state requests are about 4× faster than with
OPcache off.

## Consequences

- `data/php/opcache` grows with the projects used (18 MB for one Laravel
  app); it is safe to delete at any time.
- Defender exclusions trade some protection for speed; they stay opt-in,
  are listed by `meow defender show`, and are removed by
  `meow defender remove`.
