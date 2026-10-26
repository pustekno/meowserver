# MeowServer

A lightweight local web development environment for Windows, macOS, and
Linux. Drop a project into `www/` (or add any folder) and open it at
`project.localhost`.

No ads. No telemetry. No account.

[Bahasa Indonesia](README.id.md)

> **Status: pre-release.** Everything below works and is tested on
> Windows 11. macOS and Linux builds compile but have not been run yet.
> See [`docs/PRD.md`](docs/PRD.md) §16 for the plan.

## Download

Get `MeowServer-<version>-windows-x86_64-setup.exe` (or the portable
`.zip`) from the [Releases page](https://github.com/pustekno/meowserver/releases).
Versions before 1.0 are marked as pre-releases.

MeowServer is not code-signed yet, so Windows may say "Windows protected
your PC": choose **More info → Run anyway**. The few steps that change
Windows itself (hosts entries for `.test`, trusting the HTTPS certificate,
Defender exclusions) ask for administrator rights once and show "Unknown
publisher" for the same reason.

## Quick start (Windows)

1. Run the setup. It installs for your user only, without administrator
   rights, into `C:\MeowServer` or a folder you choose. It asks for a web
   server (Caddy, or Nginx or Apache if you prefer) and a database (MySQL
   8.4 as in Laragon, or MariaDB or PostgreSQL), then downloads them with
   PHP and Composer from their official sources.
2. Open MeowServer; everything is ready to start. If Laragon, XAMPP,
   WampServer, or FlyEnv is found, Home shows what would be imported, and
   nothing is copied until you press Import.
3. Put a project in `www\`, or use **Sites → Add project**, and open
   `http://<name>.localhost`.

The portable zip, or a setup run offline, offers **Install recommended
stack** on Home instead.

## What it does

**Web and PHP**
- Caddy (default), Nginx, or Apache. Installing another one on the Versions
  page switches to it; Settings switches back.
- Several PHP versions side by side, chosen per project or picked
  automatically from `composer.json`. PECL extensions (imagick, redis,
  apcu, …) in one click, and Xdebug.
- Sites are compiled into OPcache in the background as soon as they are
  added, so the first visit is fast too.
- Every folder in `www/` becomes a site within a second, links and
  junctions included. Projects elsewhere (`D:\projects\app`) are added with
  one click. Each project uses `.localhost` (default) or `.test`.
- Local HTTPS from a certificate authority that only works for
  `.localhost`, `.test`, and this computer.

**Databases**
- MySQL, MariaDB, and PostgreSQL, with several MySQL or MariaDB versions
  running at once, each on its own port.
- Open any of them in **phpMyAdmin** or **Adminer** (Adminer also does
  PostgreSQL) from the Databases page or Home's **Admin** button, already
  logged in. HeidiSQL is offered when it is installed.
- Backup, restore, and version upgrades with an automatic backup first.

**Other services and languages**
- Redis and Mailpit (catches the e-mail your projects send).
- Node.js, Python, and Go projects: detected, given the right version, and
  their dev server reachable at the project's name.
- Turning on a service that is not installed on Home downloads it first.

**Moving from another tool**
- Imports projects and databases from Laragon, XAMPP, WampServer, or
  FlyEnv, found automatically. Projects stored inside the old tool are
  copied, so it can be removed afterwards.
- Each project's needs (PHP version, database, Node version) are read from
  its files; Home says what is missing, with an install button.
- When another program holds a port (Laragon's MySQL on 3306), MeowServer
  moves to a free one and goes back once it is free.

**Small and private**
- Downloads come from official sources only, with checksums verified.
- A tray icon (≈14 MB of RAM) and a small native window. No Electron, no
  webview. Helper programs run without console windows.

## Databases

The Databases page lists each server with its host, port, and user (`root`
for MySQL and MariaDB, `postgres` for PostgreSQL, no password; reachable
from this computer only). **Open in:** opens it in your browser at
`http://phpmyadmin.localhost` or `http://adminer.localhost`; the first click
downloads the tool.

## Updating

In MeowServer: **Settings → Check for updates → Update now**. Or run the
newer setup yourself. Either way only the program files are replaced; your
projects (`www`), databases (`data`), settings (`meow.toml`), and downloaded
versions (`bin`) are kept. A portable folder updates the same way: choose
it as the install folder. Uninstalling keeps them too; delete the folder
yourself when you are done.

## From a terminal

```
meow up                            # start the services
meow link D:\projects\my-app       # → http://my-app.localhost
meow site my-app --domain test     # → http://my-app.test
meow new laravel blog              # → http://blog.localhost
meow install php 8.3               # another PHP version
meow site blog --php 8.3           # use it for one project
meow ext install imagick           # a PECL extension
meow dev start my-next-app         # a Node, Python, or Go dev server
meow ssl trust                     # https:// without warnings
meow doctor                        # what is wrong, and how to fix it
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

## Privacy

MeowServer sends nothing about you or your projects: no telemetry, no
install counting, no ads. The only requests it makes on its own are the
optional daily update check to GitHub (off by default) and the downloads
you ask for. After two weeks of use, Home asks once whether you would like
to share MeowServer or star it on GitHub; "Don't show again" ends that.

## Community

- Questions, ideas, and setups to share:
  [Discussions](https://github.com/pustekno/meowserver/discussions)
- Something broken: **Settings → Report a problem**, or
  [open an issue](https://github.com/pustekno/meowserver/issues/new/choose)
- Contributing: see [CONTRIBUTING.md](CONTRIBUTING.md)

English or Indonesian are both fine everywhere.

## Building from source

```
cargo build --release
cargo xtask dist        # portable zip and setup in dist/
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

GPL-3.0-or-later with an additional attribution term under section 7(b).
See [LICENSE](LICENSE) and [NOTICE](NOTICE). The MeowServer name and logo
are covered by [TRADEMARK.md](TRADEMARK.md).

Originally created by **Pustekno** (https://pustekno.id) and **Puskomedia**
(https://www.puskomedia.id).
