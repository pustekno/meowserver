// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `meow`, the MeowServer command line (PRD §13). Every command calls into
//! `meow_core`; nothing here decides how services work.

mod daemon;
mod print;
mod sites;
mod system;
mod versions;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use meow_core::ipc::Request;
use meow_core::layout::Layout;

#[derive(Parser)]
#[command(name = "meow", version, about = "MeowServer: a lightweight local web development environment")]
struct Cli {
    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start MeowServer and the services in `general.autostart`.
    Up {
        /// Run without a tray icon.
        #[arg(long)]
        headless: bool,
    },
    /// Stop all services. MeowServer itself keeps running.
    Down,
    /// Stop all services, close the window, and exit MeowServer.
    Quit,
    /// Start one service: web, php, mysql, mariadb, postgres, redis, mailpit.
    Start { service: String },
    /// Stop one service.
    Stop { service: String },
    /// Restart one service.
    Restart { service: String },
    /// Re-read meow.toml and project folders, then apply the changes.
    Reload,
    /// Show services and sites.
    Status,

    /// List installed versions of a service, or downloadable ones.
    List {
        service: String,
        #[arg(long)]
        available: bool,
    },
    /// Download a service from its official source.
    Install {
        service: String,
        /// A version or prefix such as `8.4`. Defaults to the newest.
        version: Option<String>,
    },
    /// Remove an installed version.
    Uninstall { service: String, version: String },
    /// Set the active version (`meow use php 8.3`) or web server (`meow use web nginx`).
    Use { service: String, version: String },

    /// List all sites.
    Sites,
    /// Serve a folder from anywhere as a site.
    Link {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Stop serving a site added with `link` or `--proxy`. The folder is kept.
    Unlink { name: String },
    /// Serve every subfolder of a folder as a site.
    Park { dir: PathBuf },
    /// Stop serving a parked folder.
    Unpark { dir: PathBuf },
    /// Create a project in www/: blank, laravel, wordpress, or composer <package>.
    New {
        template: String,
        name: String,
        /// For `composer`: the package to create from, e.g. symfony/skeleton.
        #[arg(long)]
        package: Option<String>,
    },
    /// Change one site's settings.
    Site {
        name: String,
        /// PHP version for this site, or `default`.
        #[arg(long)]
        php: Option<String>,
        /// Forward to a port, a URL, or `auto` (picks a free port) instead of serving files.
        #[arg(long)]
        proxy: Option<String>,
        /// Document root relative to the project folder, or `auto`.
        #[arg(long)]
        root: Option<String>,
        /// Domain ending for this site: `localhost`, `test`, or `default`.
        #[arg(long)]
        domain: Option<String>,
    },

    /// Open a shell with PATH and php.ini set to the active versions.
    Shell,
    /// Run one command with MeowServer's PATH and php.ini, e.g. `meow run -- composer install`.
    Run {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        command: Vec<String>,
    },
    /// Show a service's log.
    Logs {
        service: String,
        /// Keep printing new lines.
        #[arg(short, long)]
        follow: bool,
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
    },
    /// Check for problems and suggest fixes.
    Doctor,
    /// Manage hosts file entries.
    Hosts {
        #[command(subcommand)]
        action: system::HostsAction,
    },
    /// Database backup, restore, upgrade, and GUI.
    Db {
        #[command(subcommand)]
        action: system::DbAction,
    },
    /// Share a site on the internet through cloudflared or ngrok (installed by you).
    Share { site: String },
    /// PHP extensions from PECL (imagick, redis, apcu, …) for Windows PHP builds.
    Ext {
        #[command(subcommand)]
        action: versions::ExtAction,
    },
    /// Start or stop the development server of a Node, Python, or Go project.
    Dev {
        #[command(subcommand)]
        action: sites::DevAction,
    },
    /// Turn Xdebug on (downloads it for each Windows PHP version) or off.
    Xdebug { state: system::OnOff },
    /// Local HTTPS: trust or untrust MeowServer's certificate authority.
    Ssl {
        #[command(subcommand)]
        action: system::SslAction,
    },
    /// Start MeowServer when you log in (`on`) or not (`off`).
    Login { state: system::OnOff },
    /// Undo every change MeowServer made outside its folder (hosts entries,
    /// trusted certificate, Defender exclusions, login item).
    UndoSystemChanges,
    /// Exclude MeowServer and project folders from Microsoft Defender scanning.
    Defender {
        #[command(subcommand)]
        action: system::DefenderAction,
    },
    /// Import projects and databases from Laragon, XAMPP, WampServer, or FlyEnv.
    Import(system::ImportArgs),
    /// Prepare a problem report for MeowServer's developers.
    Report {
        /// Open the pre-filled GitHub issue in the browser.
        #[arg(long)]
        open: bool,
    },
    /// Steps that need administrator rights; started by MeowServer itself.
    #[command(hide = true)]
    Privileged {
        #[command(subcommand)]
        action: system::Privileged,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let json = cli.json;
    // The elevated helper gets its folder explicitly: elevation does not
    // pass the environment along.
    if let Cmd::Privileged { action } = cli.command {
        return system::privileged(action);
    }
    let layout = Layout::discover()?;
    match cli.command {
        Cmd::Up { headless } => {
            let mut client = daemon::ensure(&layout, headless)?;
            let started = client.call(Request::StartAll);
            print::status(&client.call(Request::Status)?, json)?;
            started.map(drop)
        }
        Cmd::Down => daemon::call(&layout, Request::StopAll).map(drop),
        Cmd::Quit => daemon::quit(&layout),
        Cmd::Start { service } => daemon::call(&layout, Request::Start { unit: service }).map(drop),
        Cmd::Stop { service } => daemon::call(&layout, Request::Stop { unit: service }).map(drop),
        Cmd::Restart { service } => daemon::call(&layout, Request::Restart { unit: service }).map(drop),
        Cmd::Reload => daemon::call(&layout, Request::Reload).map(drop),
        Cmd::Status => daemon::status(&layout, json),

        Cmd::List { service, available } => versions::list(&layout, &service, available, json),
        Cmd::Install { service, version } => versions::install(&layout, &service, version.as_deref()),
        Cmd::Uninstall { service, version } => versions::uninstall(&layout, &service, &version),
        Cmd::Use { service, version } => versions::use_version(&layout, &service, &version),

        Cmd::Sites => sites::list(&layout, json),
        Cmd::Link { path, name } => sites::link(&layout, &path, name.as_deref()),
        Cmd::Unlink { name } => sites::unlink(&layout, &name),
        Cmd::Park { dir } => sites::park(&layout, &dir),
        Cmd::Unpark { dir } => sites::unpark(&layout, &dir),
        Cmd::New { template, name, package } => {
            sites::new_project(&layout, &template, &name, package.as_deref())
        }
        Cmd::Site { name, php, proxy, root, domain } => {
            sites::configure(&layout, &name, php, proxy, root, domain)
        }

        Cmd::Shell => system::shell(&layout),
        Cmd::Run { command } => system::run_command(&layout, &command),
        Cmd::Logs { service, follow, lines } => system::logs(&layout, &service, follow, lines),
        Cmd::Doctor => system::doctor(&layout, json),
        Cmd::Hosts { action } => system::hosts(&layout, action),
        Cmd::Defender { action } => system::defender(&layout, action),
        Cmd::Ssl { action } => system::ssl(&layout, action),
        Cmd::Db { action } => system::db(&layout, action),
        Cmd::Share { site } => system::share(&layout, &site),
        Cmd::Dev { action } => sites::dev(&layout, action),
        Cmd::Ext { action } => versions::ext(&layout, action),
        Cmd::Xdebug { state } => system::xdebug(&layout, state),
        Cmd::Login { state } => system::login(&layout, state),
        Cmd::UndoSystemChanges => system::undo(&layout),
        Cmd::Import(args) => system::import(&layout, args),
        Cmd::Report { open } => system::report(&layout, open),
        Cmd::Privileged { .. } => unreachable!(),
    }
}
