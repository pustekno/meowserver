# 0004 — `php-cgi` pool on Windows

Status: accepted (2026-10-25)

## Context

Windows has no PHP-FPM, and `php-cgi` serves one request at a time per
process. PRD §9.4 asks for a pool that the web server balances across.

## Decision

- N `php-cgi` processes per PHP version (default 4, `php.pool_size`), each on
  its own port from 19000 upward (20 ports reserved per version).
- `PHP_FCGI_MAX_REQUESTS=1000`: each process exits after 1000 requests and
  the supervisor restarts it immediately (`Restart::Always`), limiting leaks.
- Caddy balances with `php_fastcgi <all ports>` plus `lb_try_duration 5s`, so
  a request that hits a recycling process is retried on another.

## Measurement (Windows 11, PHP 8.4.27 NTS, Caddy 2.11.7)

`ab -n 10000 -c 32` against a small PHP page:

- **0 failed requests**, ~3,200 requests/s, longest request 271 ms.
- Every process recycled 4 times during the run; no 502 in Caddy's log.

## Consequences

- Nginx (`upstream` with `fastcgi_next_upstream error timeout`) and Apache
  (`mod_proxy_balancer`) need equivalent retry settings; checked when their
  templates are added.
