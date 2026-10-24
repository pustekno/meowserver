// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `meowd`: the supervisor and tray (PRD §9.3). The only long-lived
//! MeowServer process and the only owner of service processes. It must not
//! depend on a GUI toolkit.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icon;

use std::sync::Arc;
use std::sync::mpsc;

use anyhow::Result;
use meow_core::config::Config;
use meow_core::engine::Engine;
use meow_core::ipc::{self, Request, Response};
use meow_core::layout::Layout;
use meow_core::{logging, platform, supervisor};
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

enum UserEvent {
    Menu(MenuEvent),
    ServiceChanged,
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

    // Service state changes repaint the tray.
    let forward = proxy.clone();
    std::thread::spawn(move || {
        for _ in events_rx {
            if forward.send_event(UserEvent::ServiceChanged).is_err() {
                break;
            }
        }
    });

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = menu_proxy.send_event(UserEvent::Menu(e));
    }));

    let mut tray: Option<Tray> = None;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(tao::event::StartCause::Init) => match Tray::new() {
                Ok(t) => {
                    tray = Some(t);
                    autostart(&engine, &proxy);
                }
                Err(e) => {
                    log::error!("cannot create the tray icon: {e:#}");
                    *control_flow = ControlFlow::Exit;
                }
            },
            Event::UserEvent(UserEvent::Menu(e)) => {
                let Some(t) = &tray else { return };
                if e.id == t.start_all.id() {
                    in_background(&engine, &proxy, |engine| engine.start_all());
                } else if e.id == t.stop_all.id() {
                    in_background(&engine, &proxy, |engine| {
                        engine.stop_all();
                        Ok(())
                    });
                } else if e.id == t.open_www.id() {
                    let config = Config::load(engine.layout()).unwrap_or_default();
                    let www = engine.layout().resolve(&config.general.www);
                    let _ = std::fs::create_dir_all(&www);
                    let _ = platform::open(www.as_os_str());
                } else if e.id == t.open_logs.id() {
                    let _ = platform::open(engine.layout().logs().as_os_str());
                } else if e.id == t.quit.id() {
                    shut_down(&engine);
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::ServiceChanged) => {
                if let Some(t) = &tray {
                    t.refresh(&engine);
                }
            }
            Event::UserEvent(UserEvent::Message(text)) => {
                if let Some(t) = &tray {
                    t.show_message(&text);
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
    if let Err(e) = engine.start_all() {
        log::error!("start: {e:#}");
    }
    let _ = quit_rx.recv();
    shut_down(&engine);
    Ok(())
}

/// `on_shutdown` runs after the reply to a `Shutdown` request is decided;
/// the actual stop happens on the main thread.
fn serve_ipc(
    layout: &Layout,
    engine: Arc<Engine>,
    on_shutdown: impl Fn() + Send + Sync + 'static,
) -> Result<()> {
    ipc::serve(
        layout,
        Arc::new(move |request| match request {
            Request::Shutdown => {
                on_shutdown();
                Response::ok(())
            }
            other => engine.handle(other),
        }),
    )
}

fn autostart(engine: &Arc<Engine>, proxy: &EventLoopProxy<UserEvent>) {
    in_background(engine, proxy, |engine| engine.start_all());
}

/// Runs a slow engine call off the UI thread and reports failures in the tray.
fn in_background(
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
        let _ = proxy.send_event(UserEvent::ServiceChanged);
    });
}

fn shut_down(engine: &Engine) {
    log::info!("stopping all services");
    engine.stop_all();
    log::info!("meowd exiting");
}

struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    start_all: MenuItem,
    stop_all: MenuItem,
    open_www: MenuItem,
    open_logs: MenuItem,
    quit: MenuItem,
}

impl Tray {
    fn new() -> Result<Self> {
        let status = MenuItem::new("MeowServer — stopped", false, None);
        let start_all = MenuItem::new("Start All", true, None);
        let stop_all = MenuItem::new("Stop All", true, None);
        let open_www = MenuItem::new("Open www folder", true, None);
        let open_logs = MenuItem::new("Open logs folder", true, None);
        let quit = MenuItem::new("Quit (stops all services)", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &status,
            &PredefinedMenuItem::separator(),
            &start_all,
            &stop_all,
            &PredefinedMenuItem::separator(),
            &open_www,
            &open_logs,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("MeowServer")
            .with_icon(icon::stopped())
            .build()?;
        Ok(Self { icon, status, start_all, stop_all, open_www, open_logs, quit })
    }

    fn refresh(&self, engine: &Engine) {
        let processes = engine.status().processes;
        let running: Vec<String> = processes
            .iter()
            .filter(|p| matches!(p.state, supervisor::State::Running { .. }))
            .map(|p| p.id.split('#').next().unwrap_or(&p.id).to_owned())
            .fold(Vec::new(), |mut acc, id| {
                if !acc.contains(&id) {
                    acc.push(id);
                }
                acc
            });
        let failed = processes.iter().any(|p| matches!(p.state, supervisor::State::Failed { .. }));
        let text = match (running.is_empty(), failed) {
            (_, true) => "MeowServer — a service failed (see logs)".to_owned(),
            (true, false) => "MeowServer — stopped".to_owned(),
            (false, false) => format!("MeowServer — running ({})", running.join(", ")),
        };
        self.status.set_text(&text);
        let _ = self.icon.set_tooltip(Some(&text));
        let _ = self.icon.set_icon(Some(if running.is_empty() { icon::stopped() } else { icon::running() }));
    }

    fn show_message(&self, text: &str) {
        let short: String = text.chars().take(120).collect();
        self.status.set_text(format!("Error: {short}"));
        let _ = self.icon.set_tooltip(Some(format!("MeowServer — {short}")));
    }
}
