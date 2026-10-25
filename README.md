# MeowServer

A lightweight local web development environment for Windows, macOS, and
Linux. Drop a project into `www/` (or add any folder) and open it at
`project.localhost`.

No ads. No telemetry. No account.

[Bahasa Indonesia](README.id.md)

> **Status: pre-release.** Everything below works and is tested on
> Windows 11. macOS and Linux builds compile but have not been run yet.
> See [`docs/PRD.md`](docs/PRD.md) §16 for the plan.

## What it does

- Runs **Caddy** (default), **Nginx**, or **Apache** with **PHP**,
  **MySQL**, **MariaDB**, **PostgreSQL**, **Redis**, and **Mailpit**.
- Several PHP versions side by side, chosen per project. OPcache with a disk
  cache, so the first page after a restart is fast too.
- Every folder in `www/` becomes a site within a second. Projects elsewhere
  (e.g. `D:\projects\app`) are added with one click, no symlinks or
  junctions.
- Local HTTPS from one certificate authority that can only vouch for
  `.localhost`, `.test`, and this computer.
- Downloads services from their official sources and verifies checksums.
- Imports from Laragon, XAMPP, WampServer, or FlyEnv (found automatically), including MySQL/MariaDB data.
- Database backup, restore, and version upgrades with an automatic backup.
- A tray icon (≈14 MB of RAM) and a small native window. No Electron, no
  webview.

## Quick start (Windows)

1. Run `MeowServer-<version>-windows-x86_64-setup.exe`. It installs for
   your user only (no administrator rights) into `C:\MeowServer`, or a
   folder you choose. Prefer no installer? Unzip
   `MeowServer-<version>-windows-x86_64.zip` anywhere instead.
2. Start MeowServer. On the Home page choose **Install recommended
   stack** (Caddy, PHP, MySQL 8.4), or import from Laragon, XAMPP,
   WampServer, or FlyEnv when one is found.
3. Put a project in `www\`, or use **Sites → Add project**. Open
   `http://<name>.localhost`.

**Updating:** run the newer setup. It finds your MeowServer folder, quits
MeowServer, and replaces only the program files; your projects (`www`),
databases (`data`), settings, and downloaded versions (`bin`) are kept. A
portable folder updates the same way: choose it as the install folder.
Uninstalling also keeps them; delete the folder yourself when you are done.

The same from a terminal:

```
meow install caddy
meow install php
meow install mysql 8.4
meow up
meow link D:\projects\my-app      # → http://my-app.localhost
meow new laravel blog             # → http://blog.localhost
meow ssl trust                    # https:// without warnings
meow doctor                       # what is wrong, and how to fix it
```

`meow --help` lists every command.

## Folders

```
MeowServer/
├── meowd(.exe)        tray and service supervisor
├── meowserver(.exe)   window
├── meow(.exe)         command line
├── meow.toml          settings and per-site settings
├── bin/               downloaded services, one folder per version
├── etc/generated/     configs MeowServer writes (do not edit)
├── etc/custom/        your overrides, never touched
├── data/              database files, per service and version
├── www/               your projects
├── logs/
└── ssl/               local certificate authority
```

## Download

The installer and the portable zip are on the
[Releases page](https://github.com/pustekno/meowserver/releases). Versions
before 1.0 are pre-releases. Updating keeps your projects, databases, and
settings: run the newer setup, or press Update in Settings.

## Privacy

MeowServer sends nothing about you or your projects: no telemetry, no
install counting, no ads. The only requests it makes on its own are the
optional daily update check to GitHub (off by default) and the downloads
you ask for. After two weeks of use, Home asks once whether you would like
to share MeowServer or star it on GitHub; "Don't show again" ends that.

## Community

- Questions, ideas, and setups to share:
  [Discussions](https://github.com/pustekno/meowserver/discussions)
- Something broken: Settings → Report a problem, or
  [open an issue](https://github.com/pustekno/meowserver/issues/new/choose)
- Contributing: see [CONTRIBUTING.md](CONTRIBUTING.md)

English or Indonesian are both fine everywhere.

## Building from source

```
cargo build --release
cargo xtask dist        # portable archive in dist/
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

GPL-3.0-or-later with an additional attribution term under section 7(b).
See [LICENSE](LICENSE) and [NOTICE](NOTICE). The MeowServer name and logo
are covered by [TRADEMARK.md](TRADEMARK.md).

Originally created by **Pustekno** (https://pustekno.id) and **Puskomedia**
(https://www.puskomedia.id).
