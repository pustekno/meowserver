# MeowServer — Product Requirements Document

| | |
|---|---|
| Status | Approved v1.0 (2026-10-25) |
| Date | 2026-10-25 |
| Owner | Akbar Bahaulloh |
| Repository | https://github.com/pustekno/meowserver |

---

## 1. Summary

MeowServer is a local web development environment, a replacement for Laragon
that is **lightweight, ad-free, telemetry-free, open source**, and runs on
Windows, macOS, and Linux.

MeowServer is not a web server. It is a **manager**: it downloads, configures,
starts, and stops existing programs (Caddy, Nginx, Apache, PHP, MySQL,
MariaDB, and more), and takes care of the tedious parts: virtual hosts, name
resolution, local HTTPS, and switching versions.

Originally created by **Pustekno** (https://pustekno.id) and **Puskomedia**
(https://www.puskomedia.id).

## 2. Problem

| Tool | Problem |
|---|---|
| Laragon | Windows only. Recent versions require a paid license and show promotions inside the app. |
| FlyEnv | Cross-platform and open source, but built on Electron: it ships its own Chromium and uses a lot of RAM and disk. |
| XAMPP / WAMP / MAMP | One PHP version per install, no automatic virtual hosts, dated UX. MAMP Pro is paid. |
| Laravel Herd, ServBay | Freemium or paid, focused on macOS or one ecosystem. |
| Docker | Powerful, but heavy for "drop a folder in, open `project.localhost`". |

What is missing: the Laragon experience (drop a folder in `www/`, open it in
the browser immediately) with the smallest possible footprint and no
commercial interruptions, on every desktop OS.

## 3. Principles (non-negotiable)

1. **No ads, no telemetry, no account.** The app talks to the internet only
   when the user asks it to download something or check for updates. Every
   outbound request is listed in §9.5 and §15.
2. **Lightweight.** The manager must never weigh more than the services it
   manages.
3. **Portable.** One folder holds everything. Move the folder and it still
   works. Delete the folder and it is gone, apart from system changes the user
   approved, which can be undone with one action (F36).
4. **Nothing hidden.** All configuration is plain text that can be read and
   overridden.
5. **CLI first, GUI on top.** Every feature is reachable from the CLI. The GUI
   is a thin layer over the same core.
6. **Never touch project files.** MeowServer reads project folders but never
   modifies, moves, or deletes them.

## 4. Users

### 4.1 Personas

- **Primary — the Laragon user.** A PHP developer (Laravel, WordPress,
  CodeIgniter, plain PHP) on Windows, often on a mid- or low-spec laptop, who
  keeps projects on another drive (e.g. `D:\projects`) and has years of data
  in MySQL.
- **Secondary — the cross-platform team.** Developers on macOS or Linux who
  want the same tool as Windows teammates, including teams whose production
  stack is Ubuntu + Caddy + PHP (e.g. ASIS).
- **Not a target:** production deployment, public-facing servers, container
  orchestration.

### 4.2 Key scenarios

1. **Switch from Laragon.** Import existing projects and MySQL 8.4 data
   (about 8 GB in the owner's case) without dump/import, and have every site
   working the same day.
2. **Open an existing project that lives anywhere.** Click "Add project",
   pick `D:\projects\asis`, and get `asis.localhost` — no shortcuts, junctions,
   or moving folders.
3. **Start a new project.** Create a Laravel or WordPress site from the GUI in
   under a minute.
4. **Run old and new projects side by side.** One site on PHP 7.4, another
   on PHP 8.4, at the same time.
5. **See everything at a glance.** One list of all projects, their URLs, PHP
   versions, and where they live.

## 5. Scope for v1.0

There is no MVP. The first public release (v1.0) ships when every feature
below works on all three operating systems; the bar is that a Laragon user can
switch without losing anything they rely on. Internal builds are produced per
phase (§16) for daily use by the team.

### 5.1 Services

| ID | Service | Notes |
|---|---|---|
| S1 | **Caddy** (default), Nginx, Apache | Switch web server without changing projects |
| S2 | PHP, multiple versions side by side | `php-cgi` pool on Windows, `php-fpm` on macOS/Linux (§9.4) |
| S3 | MySQL, MariaDB | Separate data directories per service and major version |
| S4 | PostgreSQL | |
| S5 | Redis | |
| S6 | Mailpit | Captures PHP `mail()`; replaces Laragon's Mail Catcher |
| S7 | Node.js (version manager), Composer | Tools, not services; added to the terminal `PATH` |

MySQL is required because a MySQL 8.x data directory cannot be used by
MariaDB without a dump and import. Without MySQL, Laragon users with large
databases cannot switch.

### 5.2 Features

Each feature has a "done when" line. A feature counts as done only when that
line holds on Windows, macOS, and Linux, unless the line says otherwise.

#### Services and versions

**F1 — Start and stop.** Start All / Stop All, plus start, stop, and restart
per service. Optionally start MeowServer on login and start chosen services
automatically.
*Done when:* Start All brings up web server, PHP, and MySQL in under 3 seconds
on the reference machine (§8.1); Stop All leaves no child process running.

**F2 — Multiple versions.** Several versions of each service installed side by
side. The active version is chosen by major.minor (e.g. PHP `8.4`); the highest
installed patch of that line is used.
*Done when:* switching the active PHP version from the tray takes effect for
all sites that do not pin a version, without restarting other services.

**F3 — Install from official sources.** List, install, and remove versions
from the official sources in §9.5. Verify official checksums before
extracting. Mark downloads without an official checksum as such.
*Done when:* a checksum mismatch aborts the install and leaves nothing behind;
an archive containing paths outside its target folder is rejected.

**F4 — System binaries.** On macOS and Linux, detect services installed with
Homebrew, apt, or dnf and manage them like downloaded ones (MeowServer owns
their config and process, not their installation).
*Done when:* a Homebrew Nginx and an apt PostgreSQL appear in the version list
as "system" and can be started from MeowServer.

**F5 — Bring your own binary.** Any build placed in `bin/<service>/<version>/`
is detected.
*Done when:* a manually extracted PHP build appears in the version list after
a refresh.

#### Sites

**F6 — Automatic sites from `www/`.** Every subfolder of `www/` becomes
`<folder>.localhost`.
*Done when:* creating `www/blog` makes `blog.localhost` reachable within
3 seconds without any action.

**F7 — Project list.** One screen listing every site from every source
(`www/`, parked folders, added projects). Columns: name, URL, folder path,
source, detected framework, PHP version, HTTPS. Actions per site: open in
browser, open folder, open in editor, open terminal here, change PHP version,
toggle HTTPS, edit settings, remove from MeowServer (for added projects).
Search by name or path.
*Done when:* a user with 50 sites can find one by typing part of its name and
open it in one click.

**F8 — Add project from any folder.** An "Add project" button opens a folder
picker; dropping a folder onto the window does the same. The site name is
suggested from the folder name and can be edited, with a live preview of the
URL. No symlinks or junctions are created: the path is stored in `meow.toml`.
CLI: `meow link <path> [--name <name>]`, `meow unlink <name>`.
*Done when:* adding `D:\projects\asis` makes `asis.localhost` work without
admin rights, and removing it from MeowServer leaves the folder untouched.

**F9 — Parked folders.** Register a folder whose every subfolder becomes a
site, like `www/` (e.g. park `D:\projects`).
CLI: `meow park <dir>`, `meow unpark <dir>`.
*Done when:* a new subfolder in a parked folder becomes a site automatically,
the same way as in `www/`.

**F10 — Document root detection.** Detect the framework and document root:
Laravel, Symfony, CodeIgniter 4 (`public/`); Yii 2, Craft (`web/`); CakePHP
(`webroot/`); WordPress, plain PHP, static HTML (the folder itself). The user
can override the document root per site.
*Done when:* a fresh Laravel, WordPress, and CodeIgniter 4 project each open
their front page with pretty URLs on all three web servers.

**F11 — PHP version per site.** Pin a site to a PHP version.
*Done when:* `old.localhost` on PHP 7.4 and `new.localhost` on PHP 8.4 serve
requests at the same time, on all three web servers.

**F12 — Proxy sites.** A site that forwards to a local port, for Node, Vite,
or any other dev server (`app.localhost` → `127.0.0.1:3000`), WebSockets
included.
*Done when:* a Vite dev server with hot reload works through
`app.localhost`.

**F13 — Quick create.** New project from a template: Laravel, WordPress,
empty PHP, or any `composer create-project` package. Created in `www/` or a
chosen folder; for WordPress, a database is created too.
*Done when:* a new Laravel project opens at its URL with no manual steps.

**F14 — Local index page.** `http://localhost` shows a generated page listing
all sites with links, active versions, and service status. Static HTML, no PHP
required.
*Done when:* the page updates when a site is added or removed.

#### Name resolution and HTTPS

**F15 — Site suffix.** `.localhost` by default; `.test` as an option.
`.local` is not offered (it conflicts with mDNS on macOS).
*Done when:* changing the suffix updates every site, the index page, and the
hosts entries in one step.

**F16 — Automatic hosts entries.** Browsers and cURL resolve `*.localhost` on
their own, but the operating system resolver does not on Windows: PHP
`file_get_contents()`, `SoapClient`, and Node.js fail (tested 2026-10-24 on
Windows 11; see §9.6). MeowServer therefore writes hosts entries for every
site, for both suffixes. Sites are usable in the browser immediately; the
hosts entries follow once the user approves the elevation prompt. Declining
the prompt does not block anything.
*Done when:* after approval, `file_get_contents('http://blog.localhost')` from
PHP and `fetch('http://blog.localhost')` from Node both succeed.

**F17 — Local DNS resolver (conditional).** `meowd` answers queries for the
site suffix with the loopback address, and the OS is pointed at it once at
setup (macOS `/etc/resolver`, Linux systemd-resolved, Windows NRPT). This
removes the per-project elevation prompt. Ships in v1.0 only if the Phase 0
research shows NRPT works on Windows Home editions; otherwise F16 remains the
mechanism on Windows.
*Done when:* a new project resolves for every tool with no elevation prompt.

**F18 — Local HTTPS.** One MeowServer certificate authority issues a
certificate per site for all three web servers. Installing the CA into the
trust store is one action (including Firefox's NSS store where present). Every
site is served over both HTTP and HTTPS; "force HTTPS" is a per-site toggle.
*Done when:* `https://blog.localhost` shows no warning in Chrome, Edge,
Firefox, and Safari after the one-time trust step, with any of the three web
servers.

**F19 — LAN access (opt-in).** By default every service listens on loopback
only. The user can expose the web server to the local network (e.g. to test
on a phone); the GUI shows the LAN address. Exposing a database requires
setting a password first.
*Done when:* with LAN access off, no MeowServer service accepts a connection
on a non-loopback address.

#### PHP

**F20 — Extensions.** Turn PHP extensions on and off per PHP version,
including Xdebug (source of Xdebug builds decided in Phase 0).
*Done when:* toggling an extension restarts only the affected PHP pool.

**F21 — php.ini quick settings.** Common settings (memory limit, upload size,
max execution time, display errors) editable from the GUI; anything else goes
in the user override directory (§9.9).
*Done when:* a value set in the GUI shows up in `phpinfo()` after one click.

**F22 — Mail capture.** PHP's mail settings point at Mailpit; the Mailpit inbox
opens from the tray.
*Done when:* `mail()` from any site appears in the Mailpit inbox.

#### Databases

**F23 — Initialize on first start.** A new data directory is created on first
start, with `root` and no password (MySQL/MariaDB) or `postgres` with trust
authentication (PostgreSQL), bound to loopback only (F19).
*Done when:* `mysql -u root` connects right after the first Start All.

**F24 — Data directory per version.** Data lives in `data/<service>-<major>`.
MeowServer refuses to start a server on a data directory created by a
different service or major version, and explains why.
*Done when:* pointing MariaDB at a MySQL 8.4 data directory shows a clear
error instead of starting.

**F25 — Upgrade data in place.** When the user moves to a newer major version,
offer to upgrade the existing data: stop, back up the data directory, start
the new version, run the upgrade step, and keep the backup until the user
deletes it.
*Done when:* a MySQL 8.0 data directory upgrades to 8.4 and the backup can be
restored with one action.

**F26 — Logical backup and restore.** `meow db backup <service> [db]` and
`meow db restore`, using the service's own dump tools.
*Done when:* a backup restores into a fresh data directory with identical
row counts.

**F27 — Import from Laragon.** Detect a Laragon install, then reuse its
`www/` (as a parked folder, F9) and its MySQL/MariaDB data. The user chooses
between copying the data (default when there is enough free space) or using
it in place. Laragon must not be running at the same time.
*Done when:* the owner's Laragon setup with MySQL 8.4 data runs on MeowServer
without dump or import.

**F28 — Open in database tool.** A button that opens the configured database
GUI (HeidiSQL, DBeaver, TablePlus, Adminer, or a custom command) with the
connection filled in where the tool supports it.
*Done when:* the button opens HeidiSQL connected to the running MySQL.

#### Tools and app

**F29 — Terminal.** Open a terminal (Windows Terminal or PowerShell; macOS
Terminal; the desktop's default on Linux) with `PATH` set to the active
versions of `php`, `composer`, `mysql`, `node`, and others.
*Done when:* `php -v` in that terminal prints the active version, and a
system-wide PHP elsewhere on `PATH` does not shadow it.

**F30 — Tray.** Status, Start/Stop All, site list, version switching, open
`www`, terminal, Mailpit, logs, open window, quit. See §6.4.

**F31 — Logs.** One log per service, rotated at 10 MB with three files kept.
Web access logs are off by default. Logs open from the GUI or with
`meow logs <service> [-f]`.
*Done when:* a crashing service shows its last log lines in the GUI.

**F32 — Doctor.** `meow doctor` and a GUI equivalent check for: ports in use
(with the owning process), missing hosts entries, untrusted CA, data directory
mismatches, broken binaries, and Laragon still running.
*Done when:* every check has a fix suggestion, and one-click fixes where it is
safe.

**F33 — Share.** Share a site through cloudflared or ngrok when the user has
installed one. Not bundled.

**F34 — Languages.** GUI in English and Indonesian, switchable in Settings and
offered on first run. The initial choice follows the OS language when a
translation exists, otherwise English. Translation files are plain text so others
can add languages. The CLI is English only.

**F35 — CLI.** `meow` covers every GUI feature (§13).

**F36 — Undo system changes.** One action (GUI and `meow uninstall --system`)
removes everything MeowServer changed outside its folder: hosts entries, the
CA in trust stores, resolver configuration, login item. The installer's
uninstaller runs it.
*Done when:* after running it, a diff of hosts, trust stores, and login items
against a pre-install snapshot is empty.

**F37 — Update notification.** Off by default; offered on first run. When on,
checks GitHub Releases at most once a day and shows a notification. No
self-update in v1.0.

**F38 — Problem reports.** Errors in MeowServer itself are logged to
`logs/meowd.log`; a crash also writes `logs/crash-<time>.log` with the panic
message and backtrace. On the next start the tray and GUI offer "Report this
problem". `meow report` and the GUI's Help → Report a problem build a report
(MeowServer version, OS, active services and versions, the last 200 lines of
`meowd.log`, the latest crash log), show it for review, and open a pre-filled
GitHub issue in the browser. Nothing is sent unless the user submits the
issue; paths under the user's home folder are shortened to `~`. Errors from
the user's own sites (PHP, web server, database logs) are viewable in the app
but never part of a report.
*Done when:* a forced panic in `meowd` leads, after restart, to a pre-filled
issue page containing the panic message.

### 5.3 Out of scope

- Production use. No hardening for public exposure.
- Bundled database GUI. F28 opens the user's tool instead.
- Memcached: no maintained Windows build. Reconsider after v1.0.
- Code editor, file manager, plugin marketplace.
- Docker or containers.
- Cloud sync, accounts, paid features.
- Self-update (reconsider after v1.0).

## 6. User experience

### 6.1 First run

1. **Language:** English or Indonesian.
2. **Laragon detected?** Offer import (F27). Otherwise skip.
3. **Projects:** where `www/` lives (default inside the MeowServer folder) and,
   optionally, folders to park (e.g. `D:\projects`).
4. **Stack:** web server (Caddy preselected), PHP (latest stable preselected),
   database (MySQL 8.4 preselected). Other services can be added later.
5. **Download** with progress, then **Start All**, then open
   `http://localhost` (F14).
6. **Two questions, no preselected answer:** trust the local CA for HTTPS
   (F18)? Check for updates (F37)?

### 6.2 Main window

A single window with a sidebar:

| Screen | Content |
|---|---|
| **Home** | Services with status, version, port, Start/Stop, log shortcut. Start All / Stop All. Warnings from Doctor. |
| **Sites** | The project list (F7) with "Add project" and "New project" buttons (§6.3). |
| **Versions** | Per service: installed versions, active version, available versions, install/remove. |
| **PHP** | Per version: extensions (F20), quick settings (F21). |
| **Databases** | Status, data directory, backup/restore, open in tool. |
| **Settings** | General (language, autostart, editor, terminal), Network (suffix, LAN access, hosts, DNS), HTTPS, Updates, Undo system changes, About. |

**About** shows the attribution required by the license (§10).

### 6.3 Sites screen

```
┌ Sites ─────────────────────────────────────────── [+ Add project] [New project] ┐
│ 🔍 Search…                                                                       │
│                                                                                  │
│ Name        URL                       Folder                  PHP   HTTPS        │
│ asis        https://asis.localhost    D:\projects\asis        8.3   ✓     ⋯      │
│ blog        http://blog.localhost     www\blog                8.4         ⋯      │
│ legacy-crm  http://legacy-crm.local…  D:\old\crm              7.4         ⋯      │
│ frontend    http://frontend.localhost → 127.0.0.1:3000        —           ⋯      │
└──────────────────────────────────────────────────────────────────────────────────┘
  ⋯ = Open in browser · Open folder · Open in editor · Terminal here ·
      PHP version · HTTPS · Settings · Remove from MeowServer
```

- **Add project** → folder picker → name (editable, URL preview) → Add.
  Dropping a folder on the window opens the same dialog.
- Name conflicts are shown inline before the user confirms.
- "Remove from MeowServer" never deletes files; for sites in `www/` it is not
  offered (the folder itself defines the site).

### 6.4 Tray menu

```
● MeowServer — running (Caddy, PHP 8.4, MySQL 8.4)
  Start All / Stop All
  Open MeowServer
  ─────────
  Sites               ▸  asis.localhost, blog.localhost, …
  Web server          ▸  ● Caddy  ○ Nginx  ○ Apache
  PHP                 ▸  ○ 7.4  ○ 8.3  ● 8.4
  Services            ▸  MySQL ✓, Redis ✗, Mailpit ✓ …
  ─────────
  Open www folder
  Terminal
  Mail inbox
  ─────────
  Quit (stops all services)
```

### 6.5 Errors

Every error shown to the user says what failed, why (if known), and what to do
next, with a button for the fix when one exists. Example: "Port 80 is used by
`httpd.exe` (PID 4412, from C:\laragon). [Stop Laragon] [Use port 8080]".

## 7. Glossary

| Term | Meaning |
|---|---|
| Site | A URL MeowServer serves, backed by a folder or a proxy target |
| `www/` | MeowServer's default projects folder |
| Parked folder | A folder whose subfolders all become sites |
| Added project | A single folder registered as a site, wherever it lives |
| Active version | The version used by default for a service |
| `meowd` | The background process that owns all services (§9.3) |

## 8. Non-functional requirements

### 8.1 Performance

Targets for the manager only, excluding the managed services. Measured in
Phase 0 and in every release on the reference machine (Windows 11, 4-core
laptop CPU, 8 GB RAM, SSD).

| Metric | Target |
|---|---|
| RAM, `meowd` idle (supervisor + tray, window closed) | ≤ 15 MB |
| RAM, `meowd` + GUI window open | ≤ 40 MB total |
| CPU idle | ~0%: no busy polling, no animation while idle |
| Download size (without service binaries) | ≤ 15 MB |
| Window open time | < 300 ms |
| Start All (web server + PHP + MySQL, already installed) | < 3 s |
| New folder in `www/` to reachable site | < 3 s |

### 8.2 Reliability

- If `meowd` crashes or is killed, every child process dies with it. No
  orphaned `mysqld` holding a port.
- Database servers are always stopped cleanly first (`mysqladmin shutdown`,
  `pg_ctl stop -m fast`) and killed only after a timeout.
- A crashing service is restarted up to 3 times in a minute, then marked
  failed with its last log lines.
- `meow.toml` is written atomically (write to temp file, then rename).

### 8.3 Security

- **Loopback by default.** Every service binds to `127.0.0.1` and `::1` only
  (F19). A database without a password must never be reachable from the
  network.
- **IPC is private.** The named pipe (Windows) is restricted to the current
  user; the Unix socket is mode `0600`.
- **Downloads.** HTTPS only, official checksums verified, archives with
  absolute paths or `..` entries rejected.
- **Elevated helper.** Runs one fixed command per invocation, validates every
  input (e.g. host names against a strict pattern), and never executes
  user-supplied strings.
- **Local CA.** The private key is readable by the current user only. The CA
  certificate should carry X.509 name constraints limiting it to the site
  suffixes, `localhost`, and loopback addresses, so a leaked key cannot be used
  against real websites (browser support verified in Phase 0).
- `SECURITY.md` describes private vulnerability reporting through GitHub
  Security Advisories.

### 8.4 Privacy

No analytics, crash reporting, or usage pings. The only outbound requests are
the downloads the user triggers (§9.5) and the opt-in update check (F37).

### 8.5 Platforms

Windows 10+ (x64, ARM64), macOS 12+ (Intel, Apple Silicon), Linux x64/ARM64
with glibc.

### 8.6 Accessibility

All GUI actions reachable by keyboard; screen reader support through the GUI
toolkit's accessibility layer (a criterion in the toolkit choice, §9.2);
respects the OS light/dark setting and scaling.

## 9. Technical decisions

### 9.1 Language: Rust

| | Rust | Go | C / Zig |
|---|---|---|---|
| Binary size and RAM | Smallest (no GC/runtime) | Small, but runtime + GC | Smallest |
| Lightweight native GUI | Slint, iced, egui | Fyne, Gio (more limited) | Build your own / GTK / Win32 per OS |
| Cross-compiling | Good (per-target setup) | Easiest | Painful |
| Memory safety | Yes | Yes | No (C) / partial (Zig) |
| Development speed | Medium | Fast | Slow |
| Needed ecosystem (tray, cert generation, archives, HTTP) | Complete | Complete | Thin |

**Decision: Rust.** It gives the lightest result for a native GUI without a
webview while staying memory-safe. C or Zig could be marginally smaller, but
building a cross-platform GUI and tray by hand costs far more than the
difference is worth.

The GUI matters more than the language. With a webview (Tauri, Wails), Go and
Rust alike would use tens to hundreds of MB for the WebView2/WebKit processes.
The trade-off is a steeper learning curve; recorded in ADR 0001.

### 9.2 GUI: native toolkit, no webview

**Decision (Phase 0, ADR 0002): Slint with the software renderer.** Measured
23 MB working set and 0% idle CPU; GPU renderers cost ~100 MB more and were
rejected. Slint won over iced on accessibility and start time. The candidates
as originally evaluated:

- **Slint**: declarative `.slint` files separate from logic, built for small
  devices, accessibility through AccessKit. Available under GPLv3, which
  matches this project's license (§10) with no extra conditions.
- **iced**: MIT, Elm architecture, software renderer (tiny-skia) available so
  a GPU is not required.

The tray uses the `tray-icon` crate and lives in `meowd`, not in the GUI
process. The GUI is a separate process started on demand that exits fully
when closed (§9.3), so the GUI library is never loaded while the user only
uses the tray.

Rejected: Electron (the reason this project exists), Tauri/Wails (still a
webview), Qt (licensing and size), GTK (looks foreign on Windows and macOS).

### 9.3 Process architecture

```
┌──────────────┐                                     ┌───────────────────┐
│  meow (CLI)  │ ──┐                                 │      meowd        │
└──────────────┘   │  IPC (named pipe / Unix socket) │ supervisor + tray │
┌──────────────┐   ├───────────────────────────────▶ │  (no GUI toolkit) │
│  meowserver  │ ──┘                                 └─────────┬─────────┘
│  (GUI window,│                                               │ spawn, watch, stop
│   optional)  │                ┌──────────────┬───────────────┼───────────────┬──────────────┐
└──────────────┘                ▼              ▼               ▼               ▼              ▼
                     caddy/nginx/httpd   php-cgi/fpm   mysqld/mariadbd    postgres     redis, mailpit
```

- `meowd` is the supervisor and tray: the only long-lived process and the
  only owner of child processes. No OS service is installed.
- `meowserver` (GUI) and `meow` (CLI) are both **clients** of `meowd`. The
  tray's "Open" starts `meowserver`; closing the window ends that process. If
  `meowd` is not running, any client starts it. `meow up --headless` runs
  `meowd` without a tray (Linux, dev servers).
- Children are bound to `meowd`: a **Job Object** on Windows, a process group
  plus `PR_SET_PDEATHSIG` on Linux, a process group plus a watchdog on macOS.
  Because of the split, a GUI crash never takes services down.
- **IPC protocol:** newline-delimited JSON over the pipe/socket. Requests get
  one response; clients can subscribe to an event stream (status changes, log
  lines, download progress). Every message carries a protocol version;
  mismatched clients get a clear "please restart MeowServer" error.
- **Watching folders:** `www/` and parked folders are watched with the OS
  file-change APIs; on change, configs are regenerated and the web server is
  reloaded (not restarted) where it supports reload.

### 9.4 PHP through FastCGI

All three web servers talk to PHP over FastCGI, Apache included
(`mod_proxy_fcgi`, not `mod_php`). This is what makes per-site PHP versions
(F11) work the same way on every web server.

- **macOS/Linux:** `php-fpm`, one pool per PHP version in use.
- **Windows:** there is no FPM, so `meowd` runs its own `php-cgi` pool: N
  processes per version (default 4) on consecutive ports,
  `PHP_FCGI_MAX_REQUESTS` so each process is recycled, dead processes replaced
  immediately, and the web server spreads requests across the ports (upstream
  in Nginx, multiple upstreams in Caddy's `php_fastcgi`, `mod_proxy_balancer`
  in Apache).
- FastCGI ports come from 19000–19999 to stay clear of Xdebug's 9003 and
  other common dev ports.

### 9.5 Binary sources

Service binaries are **not bundled**. They are downloaded on demand from
official sources. There is no self-maintained registry: each source has a
small *provider* that reads the version list from upstream.

Endpoint check, 2026-10-24:

| Service | Version list source | Windows | macOS | Linux | Official checksum |
|---|---|---|---|---|---|
| Caddy | GitHub Releases API | ✅ | ✅ | ✅ | ✅ asset digest |
| PHP | `downloads.php.net/~windows/releases/releases.json` | ✅ | — | — | ✅ SHA-256 |
| PHP | static-php-cli (`dl.static-php.dev`, JSON listing) | — | ✅ | ✅ | — |
| Nginx | `nginx.org/download/` listing | ✅ | system | system | — (PGP) |
| Apache | Apache Lounge download page | ✅ | system | system | — |
| MariaDB | `downloads.mariadb.org` REST API | ✅ | system | ✅ | ✅ SHA-256 |
| MySQL | no API; curated list in this repo | ✅ | system | system | MD5 on site |
| PostgreSQL | `postgresql.org/versions.json` + EDB zip | ✅ | system | system | — |
| Redis | GitHub `redis-windows/redis-windows` (community port) | ✅ | system | system | ✅ asset digest |
| Mailpit | GitHub Releases API | ✅ | ✅ | ✅ | ✅ asset digest |
| Node.js | `nodejs.org/dist/index.json` | ✅ | ✅ | ✅ | ✅ SHASUMS256 |
| Composer | `getcomposer.org/download/latest-stable/` | ✅ | ✅ | ✅ | ✅ SHA-256 |

"system" means MeowServer uses the binary installed by Homebrew/apt/dnf (F4)
and still manages its config and process.

Rules:
- Official checksums are verified before extraction. Without one, the
  download is HTTPS-only, flagged in the GUI as "no official checksum", and its
  hash is recorded on first download.
- MySQL is the only hand-curated version list (a small file in this repo),
  because Oracle offers no API.
- A weekly CI job calls every endpoint and fails if a format changes.

### 9.6 Name resolution

Test on Windows 11, 2026-10-24, resolving `foo.localhost`:

| Client | Result |
|---|---|
| Chrome, Edge, Firefox | ✅ resolve to loopback internally |
| `curl`, PHP cURL (Guzzle, Laravel `Http`) | ✅ libcurl resolves `*.localhost` itself |
| PHP `file_get_contents()`, `SoapClient`, `gethostbyname()` | ❌ "No such host is known" |
| Node.js (`fetch`, Vite proxy, Nuxt/Next SSR) | ❌ `ENOTFOUND` |

Hence: `.localhost` by default because browsers need nothing and treat it as a
secure context (camera, service workers, clipboard APIs work over plain HTTP),
plus hosts entries (F16) so every other client works too, and the local DNS
resolver (F17) to remove the per-project elevation prompt if it proves viable
on Windows.

### 9.7 Elevation

Needed only to: write the hosts file, install the CA into system trust stores,
configure the OS resolver (F17), and bind ports below 1024 on Linux.
`meow` relaunches itself elevated for one narrow command
(e.g. `meow privileged hosts-sync`) and exits.

| OS | Mechanism |
|---|---|
| Windows | UAC (`ShellExecute` with `runas`). The CA goes into the user store (`certutil -user`), which needs no UAC. |
| macOS | Authorization Services / `osascript … with administrator privileges` |
| Linux | `pkexec`; for ports 80/443, `setcap cap_net_bind_service` on the web server binary, or fall back to 8080/8443 |

The main app **never** runs elevated.

### 9.8 Web servers and HTTPS

Caddy is the default because it is the only one with official binaries for all
three OSes, its config is short enough to read and override, and it matches
the owner's production stack (Ubuntu + Caddy + PHP, including ASIS). Nginx and
Apache are fully supported for people coming from Laragon or running them in
production (e.g. `.htaccess` on Apache).

MeowServer creates one local CA and issues a certificate per site for all
three web servers. Caddy is configured to use the same CA as its local PKI
root, so the user trusts one CA no matter which web server is active.

### 9.9 Configuration

Three layers, applied in order:

1. **Generated** — `etc/generated/`, rewritten on every start and change.
   **Never edit**; changes are lost.
2. **User overrides** — `etc/custom/`, included by the generated config and
   never touched by MeowServer. A file at
   `etc/custom/<webserver>/sites/<name>.conf` replaces the generated block for
   that site. This fixes the classic Laragon problem of manual edits vanishing
   after a version switch.
3. **PHP** — the generated `php.ini`, then `PHP_INI_SCAN_DIR` pointing at
   `etc/custom/php/` and `etc/custom/php/<major.minor>/`.

`meow.toml` holds app settings and per-site settings, so project folders stay
clean. It carries a `schema` number; on upgrade, MeowServer migrates it
forward and keeps a copy of the previous file.

```toml
schema = 1

[general]
language = "en"              # en | id
suffix = "localhost"         # localhost | test
web_server = "caddy"         # caddy | nginx | apache
www = "www"                  # relative to the MeowServer folder, or absolute
parked = ["D:/projects"]
start_on_login = false
autostart = ["web", "php", "mysql"]
editor = "code"
lan_access = false

[versions]
php = "8.4"
mysql = "8.4"
node = "22"

[services.mysql]
port = 3306

[php]
extensions = ["curl", "fileinfo", "gd", "intl", "mbstring", "mysqli",
              "openssl", "pdo_mysql", "pdo_sqlite", "sodium", "zip"]

[php.ini]
memory_limit = "512M"
upload_max_filesize = "128M"
post_max_size = "128M"

[sites.asis]                 # an added project (F8)
path = "D:/projects/asis"
php = "8.3"
force_https = true

[sites.legacy-crm]
path = "D:/old/crm"
php = "7.4"
root = "public_html"         # overrides detection (F10)

[sites.frontend]             # a proxy site (F12)
proxy = "http://127.0.0.1:3000"
```

### 9.10 Default ports and credentials

| Service | Port | Credentials |
|---|---|---|
| HTTP / HTTPS | 80 / 443 | — |
| MySQL | 3306 | `root`, no password, loopback only |
| MariaDB | 3306 | `root`, no password, loopback only. If MySQL is also enabled, the GUI asks to move one to 3307. |
| PostgreSQL | 5432 | `postgres`, trust auth, loopback only |
| Redis | 6379 | none, loopback only |
| Mailpit | SMTP 1025, web 8025 | none |
| PHP FastCGI | 19000–19999 | — |
| Caddy admin API | loopback, port allocated by `meowd` | — |
| `meowd` IPC | named pipe / Unix socket, no TCP port | current user only |

## 10. License and governance

### 10.1 License

**GPL-3.0-or-later, with an additional attribution term under section 7(b).**

- Anyone may use, modify, and distribute MeowServer, commercially included,
  as long as derivatives stay GPL with source available.
- Section 7(b) of the GPLv3 explicitly allows requiring preservation of author
  attributions. This keeps the Pustekno and Puskomedia credit visible.
- No custom license: Linux distributions and contributors avoid them.

**Copyright holders:** the two companies behind the Pustekno and Puskomedia
brands, both owned by Akbar Bahaulloh. A brand cannot hold copyright, so the
legal names are used:

```
Copyright (C) 2026 PT Pusat Teknologi Nusantara (Pustekno)
                   and PT Puskomedia Indonesia Kreatif (Puskomedia)
```

Code written by Akbar Bahaulloh is written on behalf of both companies.
Akbar Bahaulloh is recorded as founder and lead maintainer in `AUTHORS`.

**Additional term**, kept in `NOTICE`. `LICENSE` holds the unmodified GPL
text so tools such as GitHub detect the license correctly. Section 7 requires
the relevant source files to point to the additional terms, so every source
file carries a three-line header referring to `NOTICE` (see
`CONTRIBUTING.md`):

```
ADDITIONAL TERMS UNDER SECTION 7(b) OF THE GNU GPL VERSION 3

Any conveyed copy or modified version of this program must preserve the
following author attribution, unaltered, in the Appropriate Legal Notices
displayed by its user interface (for example, an "About" screen) and in
its accompanying documentation:

    "Originally created by Pustekno (https://pustekno.id)
     and Puskomedia (https://www.puskomedia.id)."

A modified version may add its own attributions next to it.
```

Things to be aware of:
- **No open source license can stop a fork from adding ads.** The name and
  logo can: `TRADEMARK.md` states that modified versions may not use the
  "MeowServer" name or logo.
- Third-party binaries (PHP, MySQL and MariaDB under GPL-2.0, Caddy under
  Apache-2.0, and others) are not distributed by this project; users download
  them from official sources, so they do not affect this project's license.
- Code adapted from permissively licensed projects (MIT, BSD) is recorded in
  `NOTICE` with its original license text. Borrowing ideas needs no notice;
  translating code line by line into Rust does.

### 10.2 Contributions

- Contributors use the **DCO** (Developer Certificate of Origin), not a CLA:
  they commit with `git commit -s`, which adds
  `Signed-off-by: Name <email>` stating they have the right to contribute the
  code under the project license. A CI check rejects pull requests with
  unsigned commits.
- Contributors keep the copyright to their contributions, so relicensing later
  needs their consent. If a dual commercial license is ever wanted, a CLA must
  be decided **before** the first outside contribution.

### 10.3 Governance

- Lead maintainer: Akbar Bahaulloh. Maintainers are listed in `AUTHORS`.
- Merging requires a maintainer's approval and green CI on all three OSes.
- Behaviour: `CODE_OF_CONDUCT.md` (Contributor Covenant).
- Security: `SECURITY.md` (§8.3).
- Significant technical decisions are recorded as ADRs in `docs/adr/`.

## 11. Repository layout

A Cargo workspace with four crates. No further splitting until there is a real
reason.

```
meowserver/
├── Cargo.toml                 # workspace + shared dependency versions
├── crates/
│   ├── core/                  # library: all logic, no UI
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs      # meow.toml read/write/migrate
│   │       ├── layout.rs      # portable folder paths (bin/, data/, etc/, …)
│   │       ├── supervisor.rs  # spawn, watch, restart, stop children
│   │       ├── service/       # one file per service
│   │       │   ├── mod.rs     # Service trait
│   │       │   ├── caddy.rs
│   │       │   ├── nginx.rs
│   │       │   ├── apache.rs
│   │       │   ├── php.rs     # includes the Windows php-cgi pool
│   │       │   ├── mysql.rs
│   │       │   ├── mariadb.rs
│   │       │   ├── postgres.rs
│   │       │   ├── redis.rs
│   │       │   └── mailpit.rs
│   │       ├── provider/      # one file per download source (§9.5)
│   │       │   ├── mod.rs     # Provider trait: versions, URL, checksum
│   │       │   ├── github.rs  # Caddy, Mailpit, Redis
│   │       │   ├── php.rs
│   │       │   ├── mariadb.rs
│   │       │   ├── node.rs
│   │       │   ├── ...
│   │       │   └── system.rs  # Homebrew/apt detection
│   │       ├── download.rs    # download, verify, extract
│   │       ├── site/          # sites from www/, parked folders, added projects
│   │       │   ├── mod.rs
│   │       │   └── detect.rs  # framework and document root detection
│   │       ├── template.rs    # render configs from templates/
│   │       ├── hosts.rs       # hosts file diff (written via elevation)
│   │       ├── dns.rs         # local resolver (F17)
│   │       ├── tls.rs         # local CA and per-site certificates
│   │       ├── ipc.rs         # client ↔ meowd protocol
│   │       ├── engine.rs      # turns config + sites into running services
│   │       ├── logging.rs     # meowd log, rotation, crash logs
│   │       └── platform/      # the only place for OS-specific code
│   │           ├── mod.rs
│   │           ├── windows.rs
│   │           └── unix.rs    # macOS + Linux, with cfg for the differences
│   ├── cli/                   # `meow` binary
│   ├── daemon/                # `meowd` binary: supervisor + tray, no GUI toolkit
│   └── app/                   # `meowserver` binary: GUI window, IPC client
│       ├── src/
│       └── ui/                # .slint files
├── templates/                 # config templates per service
│   ├── caddy/  nginx/  apache/  php/  mysql/  mariadb/  postgres/  redis/
│   └── index/                 # localhost index page (F14)
├── data/
│   └── mysql-versions.toml    # the only hand-curated version list (§9.5)
├── assets/                    # icons, logo
├── locales/                   # en.ftl, id.ftl
├── packaging/
│   ├── windows/               # installer + portable .zip
│   ├── macos/                 # .app bundle, .dmg
│   └── linux/                 # AppImage, .deb
├── xtask/                     # build/release tasks in Rust (`cargo xtask dist`)
├── docs/
│   ├── PRD.md
│   ├── architecture.md
│   └── adr/                   # 0001-language-rust.md, …
├── .github/workflows/         # CI on 3 OSes, release, weekly upstream check
├── LICENSE                    # unmodified GPL-3.0 text
├── NOTICE                     # copyright, section 7(b) attribution term
├── AUTHORS                    # founder, maintainers, contributors
├── TRADEMARK.md
├── SECURITY.md
├── CODE_OF_CONDUCT.md
├── CONTRIBUTING.md            # includes DCO rules
├── README.md
└── README.id.md               # Indonesian README
```

Rules:
- `core` depends on nothing in `cli`, `daemon`, or `app`. Those three hold no
  business logic; they only call `core`.
- `daemon` never depends on a GUI library (Slint/iced), only `tray-icon`. This
  is what keeps the ≤ 15 MB target realistic.
- `#[cfg(target_os = …)]` code lives only in `core/src/platform/`.
- Config templates are plain files in `templates/`, not strings in code, so
  non-Rust contributors can fix them.
- A change in an upstream format may only affect one file in `provider/`.
- English everywhere in the repo: code, comments, commit messages, docs.
  Exceptions: `locales/id.ftl` and `README.id.md`.

## 12. On-disk layout

```
MeowServer/
├── meowd(.exe)                # supervisor + tray
├── meowserver(.exe)           # GUI window
├── meow(.exe)                 # CLI
├── meow.toml                  # app and per-site settings
├── bin/                       # service binaries, per version
│   ├── caddy/2.11.7/
│   ├── php/8.4.27/
│   ├── php/7.4.33/
│   ├── mysql/8.4.x/
│   └── ...
├── etc/
│   ├── generated/             # rewritten on every start; do not edit
│   └── custom/                # user overrides; never touched by the app
├── data/                      # database data, per service and major version
│   ├── mysql-8.4/
│   ├── mariadb-11.4/
│   └── postgres-17/
├── backups/                   # automatic backups before data upgrades (F25)
├── www/                       # projects: www/foo → foo.localhost
├── logs/
└── ssl/                       # local CA and site certificates
```

When installed as a package on macOS/Linux rather than as a portable folder,
locations follow OS conventions (`~/Library/Application Support/MeowServer`,
`~/.local/share/meowserver`), with `www/` defaulting to `~/Sites` or `~/www`.

## 13. CLI reference

The CLI is `meow`, English only.

```
meow up [--headless]          start meowd and autostart services
meow down                     stop all services
meow status
meow start|stop|restart <service>

meow use php 8.4              set the active version
meow use web apache           switch web server
meow install mysql 8.4        download from the official source
meow uninstall php 7.4
meow list php [--available]

meow sites                    list all sites
meow link <path> [--name n]   add a project from any folder
meow unlink <name>
meow park <dir> / unpark <dir>
meow site <name> --php 7.4    per-site settings
meow site <name> --proxy 3000
meow new laravel blog         quick create

meow ssl trust                trust the local CA
meow db backup mysql [db]
meow db restore mysql <file>
meow import laragon
meow share <site>

meow logs <service> [-f]
meow shell                    shell with PATH set to active versions
meow doctor
meow uninstall --system       undo all system changes
```

## 14. Quality and testing

- **Unit tests** in `core`: config parsing and migration, site detection,
  hosts diffing, IPC messages.
- **Snapshot tests** for every config template (all web servers × site types),
  so a template change shows up as a reviewable diff.
- **Integration tests** in CI on Windows, macOS, and Linux with real binaries
  (cached): start each web server, request a PHP site, a pinned-version site,
  and a proxy site; start each database and connect.
- **Upstream check:** a weekly job calls every provider endpoint (§9.5).
- **Release checklist** run by hand on clean VMs (Windows 11 Home, macOS,
  Ubuntu LTS): first run, Laragon import, HTTPS trust, undo system changes.
- Performance targets (§8.1) measured and recorded for every release.

## 15. Release and distribution

- **Versioning:** SemVer. Pre-1.0 builds are published as GitHub pre-releases
  (`v0.x`) and labelled clearly in the README as not ready for general use.
- **Channels:** GitHub Releases (Windows installer + portable zip, macOS
  `.dmg`, Linux AppImage + `.deb`); then winget, Homebrew cask, and AUR.
- **Signing:** Windows via SignPath (free for open source); macOS
  notarization requires an Apple Developer account (USD 99/year).
- **Updates:** opt-in notification only (F37).

## 16. Phases

No public v1.0 before Phase 4 is done. Every phase produces an internal build
the team uses daily.

| Phase | Content | Done when |
|---|---|---|
| **0 — Foundation** | Workspace; `meowd` with tray and supervisor (Job Object / process group); Windows `php-cgi` pool; Slint vs iced comparison; local DNS research (F17); CA name-constraint check; Xdebug source | RAM measured; GUI toolkit chosen; `php-cgi` pool survives a simple load test; F17 in or out; ADRs written |
| **1 — Core on Windows** | S1–S3; F1–F3, F5–F11, F14–F16, F23, F24, F27, F31, F32, F35 | The owner's Laragon setup (projects on `D:`, MySQL 8.4 data) runs on Caddy, Nginx, and Apache without dump/import |
| **2 — GUI** | Main window (§6.2), project list and Add project (F7, F8), tray (F30), terminal (F29), first run (§6.1) | Install → Start All → add a project from `D:` → open it, without touching a terminal |
| **3 — Completeness** | S4–S7; F12, F13, F17–F22, F25, F26, F28, F33, F34, F36, F37 | Every §5 feature works on Windows |
| **4 — macOS and Linux** | Full parity; F4; packaging; CI on 3 OSes; signing | §5 checklist passes on all 3 OSes → **v1.0** |

Windows comes first because that is where Laragon users are.

## 17. Success measures

There is no telemetry, so success is measured by what can be observed
directly:

- The owner and team use MeowServer instead of Laragon for 30 consecutive
  days before v1.0, with no fallback to Laragon.
- Every target in §8.1 is met in the v1.0 release.
- In a usability test with five developers new to MeowServer, each reaches a
  working site from a fresh install in under 5 minutes without help.
- After v1.0: public signals only (GitHub stars, release downloads, issues and
  their resolution time).

## 18. Risks

| Risk | Mitigation |
|---|---|
| No MVP: long road to a public release, no outside feedback | Internal builds used daily; public repo from day one with clear pre-release status |
| Many services lack official macOS/Linux binaries | System binaries (F4), static-php-cli for PHP |
| Upstream changes a page or API format | One provider per source; weekly CI check fails on format change |
| Apache Lounge and MySQL have no stable API | Apache: small parser with tests; MySQL: curated list |
| `*.localhost` not resolved by the OS on Windows | Hosts entries (F16); local resolver (F17) if viable |
| Windows antivirus flags unsigned binaries | Code signing via SignPath, reproducible builds |
| macOS Gatekeeper blocks the app | Notarization (paid account); document the workaround until then |
| Ports 80/443 in use (IIS, Skype, Laragon, system Apache) | Doctor names the owning process, offers alternatives |
| Linux blocks ports below 1024 without root | `setcap` at setup with user consent, or 8080/8443 |
| Windows `php-cgi` pool unstable (hangs, leaks, queued requests) | Recycling via `PHP_FCGI_MAX_REQUESTS`, health check per process, load test in Phase 0 |
| Data used by the wrong database or major version | Data directories per service and major version; F24 refuses mismatches |
| Data loss during upgrade or import | Automatic backup before any in-place change (F25); copy by default on import (F27) |
| Scope creep (becoming an IDE or cPanel) | §5.3 is the contract; new features go through an ADR |

## 19. Decisions log

| Date | Decision |
|---|---|
| 2026-10-25 | **Name "MeowServer" accepted.** A web search found only the inactive GitHub account `MeowServer`; no product of that name. Crate names `meowserver`, `meow-core`, `meowd` are free on crates.io. The project is non-commercial, so if a dispute ever arises the project is renamed rather than contested. |
| 2026-10-25 | **First-run language** follows the OS language when a translation exists, otherwise English. The first-run screen shows the choice either way (§6.1). |
| 2026-10-25 | Technical decisions are delegated to the implementer and recorded as ADRs in `docs/adr/`. |
| 2026-10-25 | Phase 0 results: Rust (ADR 0001), Slint software renderer (ADR 0002), Job Object per service (ADR 0003), `php-cgi` pool verified under load (ADR 0004), local DNS pending a manual admin test (ADR 0005), Xdebug from xdebug.org on Windows (ADR 0006). |
| 2026-10-25 | Owner request: MeowServer's own errors can be reported by the user as a pre-filled GitHub issue; developers' site errors stay local (F38). |

No open questions remain for v1.0 scope. New questions are added here as they
come up.
