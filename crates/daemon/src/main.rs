// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `meowd`: the supervisor and tray (PRD §9.3). The only long-lived
//! MeowServer process and the only owner of service processes. It must not
//! depend on a GUI toolkit.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icon;
mod tray;

use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use anyhow::Result;
use meow_core::config::Config;
use meow_core::engine::Engine;
use meow_core::ipc::{self, Request, Response};
use meow_core::layout::Layout;
use meow_core::{hosts, logging, platform, report, supervisor, terminal, watch};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::MenuEvent;

use tray::{Action, Tray};

enum UserEvent {
    Menu(MenuEvent),
    Changed,
    Message(String),
    Shutdown,
}

fn main() {
    if let Err(e) = run() {
        log::error!("{e:#}");
        eprintln!("meowd: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let headless = std::env::args().any(|a| a == "--headless");
    let layout = Layout::discover()?;
    layout.ensure()?;
    logging::init(&layout.logs().join("meowd.log"), cfg!(debug_assertions) || headless)?;
    report::install_panic_hook(&layout);
    log::info!("meowd {} starting in {}", meow_core::VERSION, layout.root().display());

    let (events_tx, events_rx) = mpsc::channel::<supervisor::Event>();
    let engine = Arc::new(Engine::new(layout.clone(), Some(events_tx)));

    if headless {
        return run_headless(&layout, engine, events_rx);
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    serve_ipc(&layout, engine.clone(), {
        let proxy = proxy.clone();
        move || {
            let _ = proxy.send_event(UserEvent::Shutdown);
        }
    })?;

    let forward = proxy.clone();
    std::thread::spawn(move || {
        for _ in events_rx {
            if forward.send_event(UserEvent::Changed).is_err() {
                break;
            }
        }
    });

    let on_change = {
        let (engine, proxy) = (engine.clone(), proxy.clone());
        move || after_change(&engine, &proxy, |engine| engine.reload())
    };
    watch::spawn(layout.clone(), on_change);

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = menu_proxy.send_event(UserEvent::Menu(e));
    }));

    let crashed = report::unreported_crash(&layout).is_some();
    let mut tray: Option<Tray> = None;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => match Tray::new(&engine, crashed) {
                Ok(t) => {
                    tray = Some(t);
                    after_change(&engine, &proxy, |engine| engine.start_all());
                }
                Err(e) => {
                    log::error!("cannot create the tray icon: {e:#}");
                    *control_flow = ControlFlow::Exit;
                }
            },
            Event::UserEvent(UserEvent::Menu(e)) => {
                let Some(action) = Action::parse(&e.id) else { return };
                if let Some(t) = &mut tray {
                    t.clear_message();
                }
                if matches!(action, Action::Report) {
                    if let Some(t) = &mut tray {
                        t.crash_notice = false;
                    }
                }
                if handle(action, &engine, &proxy) {
                    shut_down(&engine);
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::Changed) => {
                if let Some(t) = &tray {
                    t.refresh(&engine);
                }
            }
            Event::UserEvent(UserEvent::Message(text)) => {
                if let Some(t) = &mut tray {
                    t.show_message(&engine, &text);
                }
            }
            Event::UserEvent(UserEvent::Shutdown) => {
                shut_down(&engine);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

/// Runs a tray action. Returns true when MeowServer should quit.
fn handle(action: Action, engine: &Arc<Engine>, proxy: &EventLoopProxy<UserEvent>) -> bool {
    let layout = engine.layout();
    let config = || Config::load(layout).unwrap_or_default();
    let report_err = |r: Result<()>| {
        if let Err(e) = r {
            log::error!("{e:#}");
            let _ = proxy.send_event(UserEvent::Message(format!("{e:#}")));
        }
    };
    match action {
        Action::StartAll => after_change(engine, proxy, |e| e.start_all()),
        Action::StopAll => after_change(engine, proxy, |e| {
            e.stop_all();
            Ok(())
        }),
        Action::OpenApp => report_err(Command::new(tray::gui_exe()).spawn().map(drop).map_err(Into::into)),
        Action::OpenUrl(url) => report_err(platform::open(url.as_ref()).map_err(Into::into)),
        Action::UseWeb(server) => {
            let mut c = config();
            c.general.web_server = server;
            report_err(c.save(layout));
            after_change(engine, proxy, |e| e.reload());
        }
        Action::UsePhp(line) => {
            let mut c = config();
            c.versions.insert("php".into(), line);
            report_err(c.save(layout));
            after_change(engine, proxy, |e| e.reload());
        }
        Action::Toggle(unit) => {
            if engine.running_units().contains(&unit) {
                after_change(engine, proxy, move |e| {
                    e.stop(unit);
                    Ok(())
                });
            } else {
                after_change(engine, proxy, move |e| e.start(&[unit]));
            }
        }
        Action::OpenWww => {
            let www = layout.resolve(&config().general.www);
            let _ = std::fs::create_dir_all(&www);
            report_err(platform::open(www.as_os_str()).map_err(Into::into));
        }
        Action::Terminal => report_err(terminal::open(layout, &config(), None)),
        Action::OpenLogs => report_err(platform::open(layout.logs().as_os_str()).map_err(Into::into)),
        Action::Report => {
            let url = report::issue_url("Problem report", &report::build(layout));
            report_err(platform::open(url.as_ref()).map_err(Into::into));
            report::acknowledge(layout);
        }
        Action::Quit => return true,
    }
    false
}

fn run_headless(
    layout: &Layout,
    engine: Arc<Engine>,
    events_rx: mpsc::Receiver<supervisor::Event>,
) -> Result<()> {
    let (quit_tx, quit_rx) = mpsc::channel::<()>();
    serve_ipc(layout, engine.clone(), move || {
        let _ = quit_tx.send(());
    })?;
    std::thread::spawn(move || for _ in events_rx {});
    let watched = engine.clone();
    watch::spawn(layout.clone(), move || {
        if let Err(e) = watched.reload() {
            log::error!("reload: {e:#}");
        }
    });
    if let Err(e) = engine.start_all() {
        log::error!("start: {e:#}");
    }
    let _ = quit_rx.recv();
    shut_down(&engine);
    Ok(())
}

/// `on_shutdown` runs when a client asks MeowServer to quit; the stop
/// itself happens on the main thread.
fn serve_ipc(
    layout: &Layout,
    engine: Arc<Engine>,
    on_shutdown: impl Fn() + Send + Sync + 'static,
) -> Result<()> {
    let proxy_engine = engine.clone();
    ipc::serve(
        layout,
        Arc::new(move |request| match request {
            Request::Shutdown => {
                on_shutdown();
                Response::ok(())
            }
            other => {
                let response = proxy_engine.handle(other);
                let engine = proxy_engine.clone();
                std::thread::spawn(move || sync_hosts(&engine));
                response
            }
        }),
    )
}

/// Runs a slow engine call off the UI thread, reports failures in the tray,
/// then brings the hosts file and the menu up to date.
fn after_change(
    engine: &Arc<Engine>,
    proxy: &EventLoopProxy<UserEvent>,
    f: impl FnOnce(&Engine) -> Result<()> + Send + 'static,
) {
    let engine = engine.clone();
    let proxy = proxy.clone();
    std::thread::spawn(move || {
        if let Err(e) = f(&engine) {
            log::error!("{e:#}");
            let _ = proxy.send_event(UserEvent::Message(format!("{e:#}")));
        }
        let _ = proxy.send_event(UserEvent::Changed);
        sync_hosts(&engine);
    });
}

/// Keeps hosts entries in step with the sites (PRD F16). At most one
/// elevation prompt at a time; declining is fine.
fn sync_hosts(engine: &Engine) {
    static BUSY: AtomicBool = AtomicBool::new(false);
    let layout = engine.layout();
    let config = Config::load(layout).unwrap_or_default();
    if !config.general.manage_hosts || hosts::is_current(layout, &config) {
        return;
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let meow = std::env::current_exe()
        .unwrap_or_default()
        .with_file_name(format!("meow{}", std::env::consts::EXE_SUFFIX));
    match hosts::sync(layout, &config, &meow) {
        Ok(hosts::Sync::Declined) => log::info!("hosts update declined by the user"),
        Ok(_) => {}
        Err(e) => log::warn!("hosts update failed: {e:#}"),
    }
    BUSY.store(false, Ordering::SeqCst);
}

fn shut_down(engine: &Engine) {
    log::info!("stopping all services");
    engine.stop_all();
    log::info!("meowd exiting");
}
