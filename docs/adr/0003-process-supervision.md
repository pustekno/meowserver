# 0003 — One Job Object per service, created suspended

Status: accepted (2026-10-25)

## Context

PRD §8.2: if `meowd` crashes or is killed, every service process must die
with it, and stopping one service must stop everything it started (Apache
and Nginx start worker processes).

## Decision

- **Windows:** every service is spawned with `CREATE_SUSPENDED`, assigned to
  its own Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, then resumed.
  Assigning before the first instruction runs means early child processes
  cannot escape the job. Stopping a service terminates its job; when `meowd`
  dies, Windows closes all its handles and every job kills its processes.
- **Linux:** each service leads its own process group, with
  `PR_SET_PDEATHSIG` so the kernel signals it if `meowd` dies.
- **macOS:** process groups; there is no parent-death signal, so stale
  processes are cleaned up on the next start (Phase 4).
- Service output goes straight to a log file handle, so no thread copies
  pipes.

## Measurement (Windows 11, Phase 0)

- `meowd` running Caddy + 4 `php-cgi`, then `Stop-Process -Force` on
  `meowd`: 5 child processes before, **0 after 0.8 s**, port 18080 free.
- `meowd` with tray, release build, idle: **13.8 MB** working set, 2.6 MB
  private, **0 ms CPU** over 10 s. Binaries: `meowd.exe` 1.9 MB,
  `meow.exe` 2.8 MB.
- Start All (Caddy + PHP pool): **319 ms**; first PHP response 592 ms later.

## Consequences

- Terminals and the GUI started from the tray must not be services: they are
  spawned detached (`spawn_detached`) so they survive `meowd` quitting.
