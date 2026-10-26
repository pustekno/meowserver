# MeowServer

A lightweight local web development environment for Windows, macOS, and
Linux. Drop a project into `www/` (or add any folder) and open it at
`project.localhost`.

No ads. No telemetry. No account.

[Bahasa Indonesia](README.id.md)

> **Status: pre-release.** Everything below works and is tested on
> Windows 11. On macOS (Apple silicon) a build from source runs Caddy,
> PHP, and Mailpit, which it downloads; MySQL, MariaDB, Nginx, Apache,
> PostgreSQL, and Redis have no download there yet, but a build linked
> into `bin/<service>/<version>/` (e.g. from Homebrew) is used. Linux
> builds compile but have not been run yet. See
> [`docs/PRD.md`](docs/PRD.md) §16 for the plan.

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
- A tray icon (≈14 MB of RAM on Windows) and a small native window. No Electron, no
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

## License and attribution

MeowServer is free software under the **GNU GPL 3.0 or later**, with one
additional term under section 7(b) ([LICENSE](LICENSE), [NOTICE](NOTICE)).
You may use it for anything, study and change it, fork it, and pass it on,
for free or for a fee.

If you pass on a copy or a modified version (a fork), you must:

1. **Keep the attribution, word for word,** in the About screen and in the
   documentation that comes with it (such as the README):

   > Originally created by Pustekno (https://pustekno.id) and Puskomedia
   > (https://www.puskomedia.id).

   You may add your own names next to it, never in place of it. MeowServer
   shows it under **Settings → About**.
2. **Keep it GPL:** your version stays under the same license, with its
   source code available to everyone you give it to.
3. **Rename it:** a modified version may not use the name "MeowServer" or
   its logo, nor look official or endorsed
   ([TRADEMARK.md](TRADEMARK.md)). Renaming does not remove the attribution
   in point 1.

Not required by the license, but asked of everyone who builds on it: say
that your project is based on MeowServer wherever you present it,
including announcements and social media posts, not only inside the
repository. A fork should be easy to recognize as one.

## Credits

- **Originally created by Pustekno (https://pustekno.id) and Puskomedia
  (https://www.puskomedia.id).** Founder and lead maintainer: Akbar
  Bahaulloh. Everyone else is in [AUTHORS](AUTHORS) and the git history.
- **Built with Claude** (Anthropic): much of the code and documentation was
  written with Claude through Claude Code, and reviewed and tested by the
  maintainers. Commits it helped with carry a `Co-Authored-By: Claude` line.
- **Icon** designed with Google Stitch.
- Written in **Rust**; the window uses **Slint** under its GPL-3.0 license.

MeowServer does not include the programs it runs. It downloads each one
from its official source when you ask, unmodified and under its own
license:
[Caddy](https://caddyserver.com),
[Nginx](https://nginx.org),
[Apache HTTP Server](https://httpd.apache.org) (Windows builds by
[Apache Lounge](https://www.apachelounge.com)),
[PHP](https://www.php.net),
[MySQL](https://www.mysql.com),
[MariaDB](https://mariadb.org),
[PostgreSQL](https://www.postgresql.org) (Windows builds by EDB),
[Redis](https://redis.io) (Windows port by
[redis-windows](https://github.com/redis-windows/redis-windows)),
[Mailpit](https://mailpit.axllent.org),
[Composer](https://getcomposer.org),
[Node.js](https://nodejs.org),
[Python](https://www.python.org) (builds by
[python-build-standalone](https://github.com/astral-sh/python-build-standalone)),
[Go](https://go.dev),
[Adminer](https://www.adminer.org),
[phpMyAdmin](https://www.phpmyadmin.net),
[Xdebug](https://xdebug.org), and PHP extensions from
[PECL](https://pecl.php.net). Thank you to everyone who makes them.
