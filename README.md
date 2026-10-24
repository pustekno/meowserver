# MeowServer

A lightweight local web development environment for Windows, macOS, and
Linux. Drop a project into `www/` (or add any folder) and open it at
`project.localhost`.

No ads. No telemetry. No account.

[Bahasa Indonesia](README.id.md)

> **Status: in development.** MeowServer is not usable yet. Follow the
> progress in [`docs/PRD.md`](docs/PRD.md) §16.

## What it does

- Runs **Caddy** (default), **Nginx**, or **Apache** with **PHP**,
  **MySQL**, **MariaDB**, **PostgreSQL**, **Redis**, and **Mailpit**.
- Several PHP versions side by side, chosen per project.
- Every folder in `www/` becomes a site automatically. Projects elsewhere
  (e.g. `D:\projects\app`) are added with one click, no symlinks.
- Local HTTPS with one trusted certificate authority.
- Downloads services from their official sources and verifies checksums.
- Imports an existing Laragon setup, including MySQL data.
- A tray icon and a small native window. No Electron, no webview.

## Why

Laragon is Windows-only and has moved to paid licensing with in-app
promotions. FlyEnv is cross-platform but built on Electron. MeowServer aims
for the Laragon experience on every desktop OS at a fraction of the memory.

## Building from source

```
cargo build --release
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

GPL-3.0-or-later with an additional attribution term under section 7(b).
See [LICENSE](LICENSE) and [NOTICE](NOTICE). The MeowServer name and logo
are covered by [TRADEMARK.md](TRADEMARK.md).

Originally created by **Pustekno** (https://pustekno.id) and **Puskomedia**
(https://www.puskomedia.id).
