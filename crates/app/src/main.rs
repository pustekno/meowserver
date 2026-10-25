// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! `meowserver`: the window (PRD §6). A client of `meowd`; it exits fully
//! when closed so the GUI toolkit is never loaded while only the tray runs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod snapshot;
mod tr;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use meow_core::config::{Config, Suffix, WebServer};
use meow_core::ipc::Request;
use meow_core::layout::Layout;
use meow_core::service::{self, Kind, php};
use meow_core::site::{self, manage};
use meow_core::{branding, doctor, import, platform, report, terminal};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use tr::t;

slint::include_modules!();

const KINDS: [Kind; 11] = [
    Kind::Php,
    Kind::Caddy,
    Kind::Nginx,
    Kind::Apache,
    Kind::Mysql,
    Kind::Mariadb,
    Kind::Postgres,
    Kind::Redis,
    Kind::Mailpit,
    Kind::Node,
    Kind::Composer,
];

const INI_FIELDS: [(&str, &str); 6] = [
    ("memory_limit", "Memory limit"),
    ("upload_max_filesize", "Largest upload"),
    ("post_max_size", "Largest form post"),
    ("max_execution_time", "Time limit per request (seconds)"),
    ("display_errors", "Show errors (On/Off)"),
    ("date.timezone", "Time zone"),
];

/// A handle the worker threads use to reach the window.
#[derive(Clone)]
pub struct Ui {
    weak: slint::Weak<AppWindow>,
    layout: Layout,
    refreshing: Arc<AtomicBool>,
}

impl Ui {
    fn on_ui(&self, f: impl FnOnce(&AppWindow) + Send + 'static) {
        let weak = self.weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                f(&ui);
            }
        });
    }

    pub fn progress(&self, text: String) {
        self.on_ui(move |ui| {
            let b = ui.global::<Backend>();
            b.set_task(text.clone().into());
            b.set_setup_progress(text.clone().into());
            b.set_install_progress(text.into());
        });
    }

    pub fn message(&self, text: String, error: bool) {
        self.on_ui(move |ui| show(ui, &text, error));
    }

    /// Re-reads everything in the background, then updates the window.
    pub fn refresh(&self) {
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return;
        }
        let ui = self.clone();
        std::thread::spawn(move || {
            let snap = snapshot::take(&ui.layout);
            let flag = ui.refreshing.clone();
            ui.on_ui(move |w| {
                LOCAL.with(|l| apply(w, &snap, &mut l.borrow_mut()));
                flag.store(false, Ordering::SeqCst);
            });
        });
    }

    /// Runs slow work off the UI thread with the window marked busy; shows
    /// the outcome and refreshes afterwards.
    pub fn run(&self, job: impl FnOnce(&Layout, &Ui) -> Result<String> + Send + 'static) {
        let ui = self.clone();
        self.on_ui(|w| w.global::<Backend>().set_busy(true));
        std::thread::spawn(move || {
            let outcome = job(&ui.layout, &ui);
            ui.on_ui(move |w| {
                let b = w.global::<Backend>();
                b.set_busy(false);
                b.set_task(SharedString::new());
                b.set_setup_running(false);
                match outcome {
                    Ok(text) if text.is_empty() => {}
                    Ok(text) => show(w, &text, false),
                    Err(e) => show(w, &format!("{e:#}"), true),
                }
            });
            ui.refresh();
            run_doctor(&ui);
        });
    }
}

/// State that lives on the UI thread only.
#[derive(Default)]
struct Local {
    sites: Vec<site::Site>,
    /// Every extension the selected PHP build ships, and the ones ticked.
    php_available: Vec<String>,
    php_ext: BTreeSet<String>,
    php_ini: BTreeMap<String, String>,
    add_path: Option<PathBuf>,
}

thread_local! {
    static LOCAL: RefCell<Local> = RefCell::default();
}

/// Started by a setup program, this process inherited Windows'
/// RedirectionGuard: it, `meowd`, and every service would refuse folder
/// links made without admin rights (most of a Laragon user's www). Starts
/// a fresh copy through Explorer instead and returns true. A stamp file
/// keeps this from looping should Explorer itself be guarded.
fn relaunch_unguarded(layout: &Layout) -> bool {
    if !platform::redirection_guarded(std::process::id()) {
        return false;
    }
    let stamp = layout.tmp().join("relaunched");
    let recent = std::fs::metadata(&stamp)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t.elapsed().is_ok_and(|age| age < Duration::from_secs(30)));
    if recent {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let _ = std::fs::write(&stamp, b"");
    std::process::Command::new("explorer.exe").arg(exe).spawn().is_ok()
}

fn main() -> Result<()> {
    let layout = Layout::discover()?;
    layout.ensure()?;
    meow_core::report::install_panic_hook(&layout);
    if relaunch_unguarded(&layout) {
        return Ok(());
    }
    // The window is a client; make sure the tray and services are up.
    let daemon_error = meow_core::ipc::ensure_running(&layout, false).err();

    let window = AppWindow::new()?;
    let pixels = branding::cat(64, branding::ORANGE);
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&pixels, 64, 64);
    window.set_window_icon(slint::Image::from_rgba8(buffer));

    let config = Config::load(&layout).unwrap_or_default();
    let _ = slint::select_bundled_translation(&config.general.language);
    tr::set_language(&config.general.language);

    let ui = Ui { weak: window.as_weak(), layout: layout.clone(), refreshing: Arc::default() };
    let b = window.global::<Backend>();
    b.set_version(meow_core::VERSION.into());
    b.set_attribution(branding::ATTRIBUTION.into());
    b.set_root_dir(layout.root().display().to_string().into());
    b.set_defender_available(platform::os() == platform::Os::Windows);
    b.set_kinds(strings(KINDS.iter().map(|k| snapshot::title(*k).to_owned())));
    if let Some(e) = daemon_error {
        show(&window, &format!("{e:#}"), true);
    }

    wire(&window, &ui);
    load_versions(&window, &layout);
    load_php(&window, &layout);
    ui.refresh();
    run_doctor(&ui);
    detect_importers(&ui);

    // Keep the view current while someone looks at it; do nothing while
    // the window is minimized (PRD §8.1: no busy work when idle).
    let timer = slint::Timer::default();
    {
        let ui = ui.clone();
        let weak = window.as_weak();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(3), move || {
            if weak.upgrade().is_some_and(|w| !w.window().is_minimized()) {
                ui.refresh();
            }
        });
    }
    window.run()?;
    Ok(())
}

fn wire(window: &AppWindow, ui: &Ui) {
    let b = window.global::<Backend>();
    let layout = ui.layout.clone();

    b.on_start_all({
        let ui = ui.clone();
        move || ui.run(|l, _| actions::call(l, Request::StartAll).map(|()| String::new()))
    });
    b.on_quit_all({
        let ui = ui.clone();
        move || {
            ui.on_ui(|w| {
                let b = w.global::<Backend>();
                b.set_busy(true);
                b.set_task(t("Stopping all services…").into());
            });
            let layout = ui.layout.clone();
            std::thread::spawn(move || {
                actions::quit(&layout);
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
            });
        }
    });
    b.on_stop_all({
        let ui = ui.clone();
        move || ui.run(|l, _| actions::call(l, Request::StopAll).map(|()| String::new()))
    });
    b.on_toggle_unit({
        let ui = ui.clone();
        move |unit| {
            let unit = unit.to_string();
            ui.run(move |l, _| {
                let snap = snapshot::take(l);
                let on = snap.services.iter().any(|s| s.unit.id() == unit && s.running)
                    || snap.databases.iter().any(|d| d.unit.id() == unit && d.running);
                let request = if on { Request::Stop { unit } } else { Request::Start { unit } };
                actions::call(l, request).map(|()| String::new())
            })
        }
    });
    b.on_open_url({
        let ui = ui.clone();
        move |url| {
            if let Err(e) = platform::open(url.as_str().as_ref()) {
                ui.message(format!("{e}"), true);
            }
        }
    });
    b.on_open_path({
        let ui = ui.clone();
        move |path| {
            if let Err(e) = platform::open(path.as_str().as_ref()) {
                ui.message(format!("{e}"), true);
            }
        }
    });
    b.on_open_terminal({
        let ui = ui.clone();
        move |path| {
            let config = Config::load(&ui.layout).unwrap_or_default();
            let cwd = (!path.is_empty()).then(|| PathBuf::from(path.as_str()));
            if let Err(e) = terminal::open(&ui.layout, &config, cwd.as_deref()) {
                ui.message(format!("{e:#}"), true);
            }
        }
    });
    b.on_open_logs({
        let layout = layout.clone();
        move || {
            let _ = platform::open(layout.logs().as_os_str());
        }
    });
    b.on_dismiss_message({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                w.global::<Backend>().set_message(SharedString::new());
            }
        }
    });

    b.on_quick_setup({
        let ui = ui.clone();
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                w.global::<Backend>().set_setup_running(true);
            }
            ui.run(|l, ui| {
                actions::quick_setup(l, ui)?;
                Ok(t("Ready. Put a project in www/ or use Sites → Add project."))
            })
        }
    });
    // Import (PRD F27): look first, show the plan, copy only after Import.
    let pending: Arc<Mutex<Option<(import::Tool, PathBuf)>>> = Arc::default();
    b.on_preview_import({
        let ui = ui.clone();
        let pending = pending.clone();
        move |id| {
            let Some(tool) = import::Tool::parse(&id) else { return };
            let pending = pending.clone();
            ui.run(move |layout, ui| {
                ui.progress(t("Looking at what can be imported…"));
                let source = import::detect(tool, None).ok_or_else(|| {
                    anyhow::anyhow!("{}", t("{} was not found.").replacen("{}", tool.name(), 1))
                })?;
                let plan = actions::import_plan(layout, &source);
                let blocked = import::in_use(&source);
                ui.progress(String::new());
                *pending.lock().unwrap() = Some((tool, source.root));
                ui.on_ui(move |w| {
                    let b = w.global::<Backend>();
                    b.set_import_plan(plan.into());
                    b.set_import_blocked(blocked);
                });
                Ok(String::new())
            })
        }
    });
    b.on_stop_import_blocker({
        let ui = ui.clone();
        let pending = pending.clone();
        move || {
            let Some((tool, root)) = pending.lock().unwrap().clone() else { return };
            ui.run(move |layout, ui| {
                let (plan, blocked) = actions::stop_import_blocker(layout, ui, tool, &root)?;
                ui.on_ui(move |w| {
                    let b = w.global::<Backend>();
                    b.set_import_plan(plan.into());
                    b.set_import_blocked(blocked);
                });
                Ok(String::new())
            })
        }
    });
    b.on_db_version({
        let ui = ui.clone();
        move |unit, line, enable| {
            let (unit, line) = (unit.to_string(), line.to_string());
            ui.run(move |layout, _| actions::db_version(layout, &unit, &line, enable))
        }
    });
    b.on_confirm_import({
        let ui = ui.clone();
        let weak = window.as_weak();
        let pending = pending.clone();
        move || {
            let Some((tool, root)) = pending.lock().unwrap().take() else { return };
            if let Some(w) = weak.upgrade() {
                w.global::<Backend>().set_import_plan(SharedString::new());
            }
            ui.run(move |layout, ui| actions::import_from(layout, ui, tool, &root))
        }
    });
    b.on_cancel_import({
        let weak = window.as_weak();
        move || {
            pending.lock().unwrap().take();
            if let Some(w) = weak.upgrade() {
                w.global::<Backend>().set_import_plan(SharedString::new());
            }
        }
    });

    // Sites.
    b.on_search({
        let weak = window.as_weak();
        move |_| {
            if let Some(w) = weak.upgrade() {
                LOCAL.with(|l| show_sites(&w, &l.borrow()));
            }
        }
    });
    b.on_pick_folder({
        let weak = window.as_weak();
        let layout = layout.clone();
        move || {
            let Some(w) = weak.upgrade() else { return };
            let Some(folder) = rfd::FileDialog::new().set_title("Choose a project folder").pick_folder()
            else {
                return;
            };
            let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let b = w.global::<Backend>();
            b.set_add_path(folder.display().to_string().into());
            b.set_add_name(site::slugify(&name).unwrap_or_default().into());
            LOCAL.with(|l| l.borrow_mut().add_path = Some(folder));
            validate_add(&w, &layout);
            b.set_adding(true);
        }
    });
    b.on_add_name_edited({
        let weak = window.as_weak();
        let layout = layout.clone();
        move |_| {
            if let Some(w) = weak.upgrade() {
                validate_add(&w, &layout);
            }
        }
    });
    b.on_cancel_add({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                w.global::<Backend>().set_adding(false);
            }
        }
    });
    b.on_confirm_add({
        let weak = window.as_weak();
        let ui = ui.clone();
        move || {
            let Some(w) = weak.upgrade() else { return };
            let b = w.global::<Backend>();
            let Some(path) = LOCAL.with(|l| l.borrow().add_path.clone()) else { return };
            let name = b.get_add_name().to_string();
            match edit_config(&ui.layout, |c| manage::link(&ui.layout, c, &path, Some(&name)).map(drop)) {
                Ok(()) => {
                    b.set_adding(false);
                    let config = Config::load(&ui.layout).unwrap_or_default();
                    show(&w, &format!("{} {name}.{}", t("Added"), config.general.suffix.as_str()), false);
                    reload_in_background(&ui);
                }
                Err(e) => b.set_add_error(format!("{e:#}").into()),
            }
        }
    });
    b.on_remove_site({
        let ui = ui.clone();
        move |name| {
            let name = name.to_string();
            match edit_config(&ui.layout, |c| manage::unlink(c, &name)) {
                Ok(()) => {
                    ui.message(format!("{name} {}", t("is no longer served. No files were touched.")), false);
                    reload_in_background(&ui);
                }
                Err(e) => ui.message(format!("{e:#}"), true),
            }
        }
    });
    b.on_set_site_php({
        let ui = ui.clone();
        move |name, version| {
            let (name, version) = (name.to_string(), version.to_string());
            let result = edit_config(&ui.layout, |c| {
                manage::settings(c, &name).php = (version != "default").then(|| version.clone());
                Ok(())
            });
            match result {
                Ok(()) => reload_in_background(&ui),
                Err(e) => ui.message(format!("{e:#}"), true),
            }
        }
    });

    b.on_set_site_domain({
        let ui = ui.clone();
        move |name, index| {
            let name = name.to_string();
            let suffix = if index == 1 { Suffix::Test } else { Suffix::Localhost };
            let result = edit_config(&ui.layout, |c| {
                // The default ending needs no setting of its own.
                manage::settings(c, &name).suffix = (suffix != c.general.suffix).then_some(suffix);
                Ok(())
            });
            match result {
                Ok(()) => reload_in_background(&ui),
                Err(e) => ui.message(format!("{e:#}"), true),
            }
        }
    });

    // Versions.
    b.on_select_kind({
        let weak = window.as_weak();
        let layout = layout.clone();
        move |_| {
            if let Some(w) = weak.upgrade() {
                load_versions(&w, &layout);
                w.global::<Backend>().invoke_load_available();
            }
        }
    });
    b.on_load_available({
        let weak = window.as_weak();
        let ui = ui.clone();
        move || {
            let Some(w) = weak.upgrade() else { return };
            let kind = selected_kind(&w);
            w.global::<Backend>().set_loading_available(true);
            let ui = ui.clone();
            std::thread::spawn(move || {
                let result = provider_releases(&ui.layout, kind);
                ui.on_ui(move |w| {
                    let b = w.global::<Backend>();
                    b.set_loading_available(false);
                    match result {
                        Ok(rows) => b.set_available_versions(model(rows)),
                        Err(e) => show(w, &format!("{e:#}"), true),
                    }
                });
            });
        }
    });
    b.on_install_version({
        let weak = window.as_weak();
        let ui = ui.clone();
        move |version| {
            let Some(w) = weak.upgrade() else { return };
            let kind = selected_kind(&w);
            let version = version.to_string();
            let weak = weak.clone();
            ui.run(move |l, ui| {
                let installed = actions::install(l, ui, kind, Some(&version))?;
                let weak = weak.clone();
                let layout = l.clone();
                ui.on_ui(move |_| {
                    if let Some(w) = weak.upgrade() {
                        load_versions(&w, &layout);
                        load_php(&w, &layout);
                    }
                });
                Ok(format!("{} {installed} {}", snapshot::title(kind), t("installed.")))
            });
        }
    });
    b.on_use_version({
        let weak = window.as_weak();
        let ui = ui.clone();
        move |version| {
            let Some(w) = weak.upgrade() else { return };
            let kind = selected_kind(&w);
            let line = version.split('.').take(2).collect::<Vec<_>>().join(".");
            let result = edit_config(&ui.layout, |c| {
                c.versions.insert(kind.id().into(), line.clone());
                Ok(())
            });
            if let Err(e) = result {
                show(&w, &format!("{e:#}"), true);
                return;
            }
            load_versions(&w, &ui.layout);
            ui.run(move |l, _| {
                match kind {
                    Kind::Php | Kind::Caddy | Kind::Nginx | Kind::Apache => actions::reload(l),
                    Kind::Mysql | Kind::Mariadb | Kind::Postgres | Kind::Redis | Kind::Mailpit => {
                        let snap = snapshot::take(l);
                        if snap.services.iter().any(|s| s.running && s.unit.id() == kind.id()) {
                            actions::call(l, Request::Restart { unit: kind.id().into() })?;
                        }
                    }
                    Kind::Node | Kind::Composer => {}
                }
                Ok(format!("{} {line} {}", snapshot::title(kind), t("is now active.")))
            });
        }
    });
    b.on_uninstall_version({
        let weak = window.as_weak();
        let ui = ui.clone();
        move |version| {
            let Some(w) = weak.upgrade() else { return };
            let kind = selected_kind(&w);
            match actions::uninstall(&ui.layout, kind, &version) {
                Ok(()) => {
                    load_versions(&w, &ui.layout);
                    show(&w, &format!("{} {} {version}.", t("Removed"), snapshot::title(kind)), false);
                }
                Err(e) => show(&w, &format!("{e:#}"), true),
            }
        }
    });

    // PHP.
    b.on_select_php({
        let weak = window.as_weak();
        let layout = layout.clone();
        move |_| {
            if let Some(w) = weak.upgrade() {
                load_php(&w, &layout);
            }
        }
    });
    b.on_toggle_extension({
        let weak = window.as_weak();
        move |name, on| {
            LOCAL.with(|l| {
                let mut l = l.borrow_mut();
                if on {
                    l.php_ext.insert(name.to_string());
                } else {
                    l.php_ext.remove(name.as_str());
                }
            });
            if let Some(w) = weak.upgrade() {
                let count = LOCAL.with(|l| l.borrow().php_ext.len());
                w.global::<Backend>().set_extensions_enabled(count as i32);
            }
        }
    });
    b.on_search_extensions({
        let weak = window.as_weak();
        move |_| {
            if let Some(w) = weak.upgrade() {
                show_extensions(&w);
            }
        }
    });
    b.on_set_ini(|key, value| {
        LOCAL.with(|l| {
            l.borrow_mut().php_ini.insert(key.to_string(), value.to_string());
        });
    });
    b.on_save_php({
        let weak = window.as_weak();
        let ui = ui.clone();
        move || {
            let Some(w) = weak.upgrade() else { return };
            let Some(installed) = selected_php(&w, &ui.layout) else { return };
            let available: BTreeSet<String> = php::available_extensions(&installed).into_iter().collect();
            let (chosen, ini) = LOCAL.with(|l| (l.borrow().php_ext.clone(), l.borrow().php_ini.clone()));
            let result = edit_config(&ui.layout, |c| {
                // Keep choices for extensions this build does not have.
                let mut list: BTreeSet<String> =
                    c.php.extensions.iter().filter(|e| !available.contains(*e)).cloned().collect();
                list.extend(chosen.iter().cloned());
                c.php.extensions = list.into_iter().collect();
                for (key, value) in &ini {
                    if php::default_setting(key) == Some(value.as_str()) || value.trim().is_empty() {
                        c.php.ini.remove(key);
                    } else {
                        c.php.ini.insert(key.clone(), value.trim().to_owned());
                    }
                }
                Ok(())
            });
            if let Err(e) = result {
                show(&w, &format!("{e:#}"), true);
                return;
            }
            ui.run(|l, _| {
                let running = snapshot::take(l).services.iter().any(|s| s.unit.id() == "php" && s.running);
                if running {
                    actions::call(l, Request::Restart { unit: "php".into() })?;
                }
                Ok(t("PHP settings saved and applied."))
            });
        }
    });

    // Settings.
    b.on_set_language({
        let ui = ui.clone();
        move |index| {
            let language = if index == 1 { "id" } else { "en" };
            let _ = slint::select_bundled_translation(language);
            tr::set_language(language);
            ui.refresh();
            if let Err(e) = edit_config(&ui.layout, |c| {
                c.general.language = language.into();
                Ok(())
            }) {
                ui.message(format!("{e:#}"), true);
            }
        }
    });
    b.on_set_web({
        let ui = ui.clone();
        move |index| {
            let server = [WebServer::Caddy, WebServer::Nginx, WebServer::Apache][index.clamp(0, 2) as usize];
            let kind = [Kind::Caddy, Kind::Nginx, Kind::Apache][index.clamp(0, 2) as usize];
            if service::installed(&ui.layout, kind).is_empty() {
                ui.message(
                    format!(
                        "{} is not installed yet; install it on the Versions page.",
                        snapshot::title(kind)
                    ),
                    true,
                );
                ui.refresh();
                return;
            }
            if edit_config(&ui.layout, |c| {
                c.general.web_server = server;
                Ok(())
            })
            .is_ok()
            {
                reload_in_background(&ui);
            }
        }
    });
    b.on_set_suffix({
        let ui = ui.clone();
        move |index| {
            let suffix = if index == 1 { Suffix::Test } else { Suffix::Localhost };
            if edit_config(&ui.layout, |c| {
                c.general.suffix = suffix;
                Ok(())
            })
            .is_ok()
            {
                reload_in_background(&ui);
            }
        }
    });
    b.on_set_lan({
        let ui = ui.clone();
        move |on| {
            if edit_config(&ui.layout, |c| {
                c.general.lan_access = on;
                Ok(())
            })
            .is_ok()
            {
                reload_in_background(&ui);
            }
        }
    });
    b.on_set_manage_hosts({
        let ui = ui.clone();
        move |on| {
            if edit_config(&ui.layout, |c| {
                c.general.manage_hosts = on;
                Ok(())
            })
            .is_ok()
            {
                reload_in_background(&ui);
            }
        }
    });
    b.on_defender_exclude({
        let ui = ui.clone();
        move || {
            ui.run(|l, _| {
                let meow = std::env::current_exe()?.with_file_name(format!("meow{}", platform::EXE_SUFFIX));
                let args = vec![
                    "privileged".into(),
                    "defender-exclude".into(),
                    "--home".into(),
                    l.root().display().to_string(),
                ];
                match platform::run_elevated(&meow, &args) {
                    Ok(0) => Ok(t("Folders excluded from Defender scanning.")),
                    Ok(code) => anyhow::bail!("excluding failed (exit code {code})"),
                    Err(_) => Ok(t("Not changed: administrator rights were declined.")),
                }
            })
        }
    });
    b.on_set_start_on_login({
        let ui = ui.clone();
        move |on| {
            let result = meow_core::system::set_login_item(&ui.layout, on).and_then(|()| {
                edit_config(&ui.layout, |c| {
                    c.general.start_on_login = on;
                    Ok(())
                })
            });
            if let Err(e) = result {
                ui.message(format!("{e:#}"), true);
            }
            ui.refresh();
        }
    });
    b.on_set_check_updates({
        let ui = ui.clone();
        move |on| {
            if let Err(e) = edit_config(&ui.layout, |c| {
                c.general.check_updates = on;
                Ok(())
            }) {
                ui.message(format!("{e:#}"), true);
            }
        }
    });
    b.on_set_xdebug({
        let ui = ui.clone();
        move |on| {
            ui.run(move |l, _| {
                use meow_core::service::php;
                let mut notes = Vec::new();
                if on {
                    let http = meow_core::http::Http::new();
                    let missing: Vec<_> = service::installed(l, Kind::Php)
                        .into_iter()
                        .filter(|i| !php::has_xdebug(i) && i.dir.join("ext").is_dir())
                        .collect();
                    if !missing.is_empty() {
                        let index = meow_core::provider::xdebug::index(&http)?;
                        for installed in missing {
                            if let Err(e) = php::install_xdebug(&http, &index, &installed) {
                                notes.push(format!("PHP {}: {e:#}", installed.version));
                            }
                        }
                    }
                }
                edit_config(l, |c| {
                    c.php.xdebug = on;
                    Ok(())
                })?;
                if snapshot::take(l).services.iter().any(|s| s.unit.id() == "php" && s.running) {
                    actions::call(l, Request::Restart { unit: "php".into() })?;
                }
                Ok(notes.join("\n"))
            })
        }
    });
    b.on_db_backup({
        let ui = ui.clone();
        move |unit| {
            let unit = unit.to_string();
            ui.run(move |l, _| {
                let kind = Kind::from_id(&unit).ok_or_else(|| anyhow::anyhow!("unknown service {unit}"))?;
                let file = meow_core::database::backup(l, kind, None)?;
                Ok(format!("{} {}", t("Saved"), file.display()))
            })
        }
    });
    b.on_install_service({
        let ui = ui.clone();
        move |id| {
            // `postgres`, or with a version: `php 7.4` (from a site's needs).
            let (id, selector) = match id.split_once(' ') {
                Some((id, version)) => (id.to_owned(), Some(version.to_owned())),
                None => (id.to_string(), None),
            };
            let Some(kind) = Kind::from_id(&id) else { return };
            // MySQL: the LTS line, which is what Laragon users have.
            let selector = selector.or_else(|| (kind == Kind::Mysql).then(|| "8.4".to_owned()));
            ui.run(move |l, ui| {
                let version = actions::install(l, ui, kind, selector.as_deref())?;
                Ok(format!("{} {version} {}", snapshot::title(kind), t("installed.")))
            });
        }
    });
    b.on_db_open({
        let ui = ui.clone();
        move |unit| {
            let Some(kind) = Kind::from_id(&unit) else { return };
            if let Err(e) = meow_core::database::open_tool(&ui.layout, kind) {
                ui.message(format!("{e:#}"), true);
            }
        }
    });
    b.on_trust_ca({
        let ui = ui.clone();
        move || {
            ui.run(|l, _| {
                meow_core::tls::trust(l)?;
                Ok(t("Trusted. Restart the browser if it was open."))
            })
        }
    });
    b.on_undo_system({
        let ui = ui.clone();
        move || {
            ui.run(|l, _| {
                let meow = std::env::current_exe()?.with_file_name(format!("meow{}", platform::EXE_SUFFIX));
                Ok(meow_core::system::undo(l, &meow).join("\n"))
            })
        }
    });
    b.on_show_create({
        let weak = window.as_weak();
        move |on| {
            if let Some(w) = weak.upgrade() {
                let b = w.global::<Backend>();
                b.set_creating(on);
                b.set_setup_progress(SharedString::new());
            }
        }
    });
    b.on_create_project({
        let weak = window.as_weak();
        let ui = ui.clone();
        move || {
            let Some(w) = weak.upgrade() else { return };
            let b = w.global::<Backend>();
            let name = b.get_new_name().to_string();
            let package = b.get_new_package().to_string();
            let template = match b.get_template_index() {
                1 => meow_core::create::Template::Laravel,
                2 => meow_core::create::Template::WordPress,
                3 => meow_core::create::Template::Composer(package),
                _ => meow_core::create::Template::Blank,
            };
            let weak = weak.clone();
            ui.run(move |l, ui| {
                let dir =
                    meow_core::create::create(l, &template, &name, &mut |text| ui.progress(text.to_owned()))?;
                actions::reload(l);
                ui.progress(String::new());
                let weak = weak.clone();
                ui.on_ui(move |_| {
                    if let Some(w) = weak.upgrade() {
                        w.global::<Backend>().set_creating(false);
                    }
                });
                Ok(format!("{} {}", t("Created"), dir.display()))
            });
        }
    });
    b.on_run_doctor({
        let ui = ui.clone();
        move || run_doctor(&ui)
    });
    b.on_report_problem({
        let ui = ui.clone();
        move || {
            let url = report::issue_url("Problem report", &report::build(&ui.layout));
            let _ = platform::open(url.as_ref());
            report::acknowledge(&ui.layout);
        }
    });
}

fn apply(w: &AppWindow, snap: &snapshot::Snapshot, local: &mut Local) {
    let b = w.global::<Backend>();
    b.set_headline(snap.headline.clone().into());
    b.set_any_running(snap.any_running);
    b.set_needs_setup(snap.needs_setup);
    b.set_localhost_url(snap.localhost_url.clone().into());
    b.set_services(model(
        snap.services
            .iter()
            .map(|s| ServiceRow {
                unit: s.unit.id().into(),
                name: s.name.clone().into(),
                version: s.version.clone().unwrap_or_default().into(),
                port: s.port.clone().into(),
                state: s.state.clone().into(),
                running: s.running,
                failed: s.failed,
                installed: s.version.is_some(),
            })
            .collect(),
    ));
    let mut choices = vec!["default".to_owned()];
    choices.extend(snap.php_lines.iter().cloned());
    b.set_php_choices(strings(choices));
    let installed_ids: Vec<&str> = snap.databases.iter().map(|d| d.unit.id()).collect();
    b.set_missing_databases(model(
        [("mysql", "MySQL"), ("mariadb", "MariaDB"), ("postgres", "PostgreSQL")]
            .into_iter()
            .filter(|(id, _)| !installed_ids.contains(id))
            .filter(|(id, _)| {
                Kind::from_id(id).is_some_and(|k| meow_core::provider::for_service(k).is_some())
            })
            .map(|(id, name)| Choice { id: id.into(), name: name.into() })
            .collect(),
    ));
    b.set_databases(model(
        snap.databases
            .iter()
            .map(|d| DbRow {
                unit: d.unit.id().into(),
                name: d.name.into(),
                version: d.version.clone().into(),
                port: d.port.to_string().into(),
                running: d.running,
                data_dir: d.data_dir.clone().into(),
                user: d.user.into(),
                extras: model(
                    d.extras
                        .iter()
                        .map(|e| DbExtra {
                            line: e.line.clone().into(),
                            enabled: e.enabled,
                            running: e.running,
                            port: e.port.map(|p| p.to_string()).unwrap_or_default().into(),
                        })
                        .collect(),
                ),
            })
            .collect(),
    ));
    let c = &snap.config;
    b.set_language_index(if c.general.language == "id" { 1 } else { 0 });
    b.set_suffix_index(snapshot::suffix_index(c.general.suffix));
    b.set_web_index(snapshot::web_index(c.general.web_server));
    b.set_lan_access(c.general.lan_access);
    b.set_manage_hosts(c.general.manage_hosts);
    b.set_start_on_login(c.general.start_on_login);
    b.set_check_updates(c.general.check_updates);
    b.set_xdebug(c.php.xdebug);
    b.set_ca_trusted(meow_core::tls::is_trusted(&w_layout(w)));

    local.sites = snap.sites.clone();
    show_sites(w, local);
}

fn show_sites(w: &AppWindow, local: &Local) {
    let b = w.global::<Backend>();
    let filter = b.get_site_filter().to_lowercase();
    let choices: Vec<String> = b.get_php_choices().iter().map(|s| s.to_string()).collect();
    let layout = w_layout(w);
    let config = Config::load(&layout).unwrap_or_default();
    let rows: Vec<SiteRow> = local
        .sites
        .iter()
        .filter(|s| {
            filter.is_empty()
                || s.name.contains(&filter)
                || s.path.to_string_lossy().to_lowercase().contains(&filter)
        })
        .map(|s| {
            let need = site::needs::problems(&layout, &config, s).into_iter().next();
            let (need, install, label) = need.map(|p| snapshot::need(&p)).unwrap_or_default();
            (s, need, install, label)
        })
        .map(|(s, need, need_install, need_install_label)| SiteRow {
            need: need.into(),
            need_install: need_install.into(),
            need_install_label: need_install_label.into(),
            name: s.name.clone().into(),
            host: s.host.clone().into(),
            url: snapshot::site_url(s, &config).into(),
            folder: s.proxy.clone().unwrap_or_else(|| s.path.display().to_string()).into(),
            php: s.php.clone().unwrap_or_else(|| "default".into()).into(),
            domain_index: if s.host.ends_with(".test") { 1 } else { 0 },
            php_index: s
                .php
                .as_deref()
                .and_then(|p| choices.iter().position(|c| c == p || p.starts_with(&format!("{c}."))))
                .unwrap_or(0) as i32,
            source: snapshot::source_label(&s.source).into(),
            framework: snapshot::framework(s.framework).into(),
            removable: s.source == site::Source::Added,
            proxy: s.proxy.is_some(),
        })
        .collect();
    // Avoid resetting the list (and its scroll position) when nothing changed.
    let current = b.get_sites();
    let same = current.row_count() == rows.len()
        && rows.iter().enumerate().all(|(i, r)| current.row_data(i).is_some_and(|c| c == *r));
    if !same {
        b.set_sites(model(rows));
    }
}

/// The layout, recovered from the window's About text (set once at start).
fn w_layout(w: &AppWindow) -> Layout {
    Layout::at(PathBuf::from(w.global::<Backend>().get_root_dir().as_str()))
}

fn validate_add(w: &AppWindow, layout: &Layout) {
    let b = w.global::<Backend>();
    let name = b.get_add_name().to_string();
    let config = Config::load(layout).unwrap_or_default();
    let error = match site::slugify(&name) {
        None => "Use letters, digits, and hyphens.".to_owned(),
        Some(slug) if slug != name => format!("Will be saved as {slug}."),
        Some(slug) => match site::discover(layout, &config).sites.iter().find(|s| s.name == slug) {
            Some(s) => format!("{slug} is already used by {}.", s.path.display()),
            None => String::new(),
        },
    };
    let preview = site::slugify(&name)
        .map(|slug| {
            let port = config.port("web", 80);
            let port = if port == 80 { String::new() } else { format!(":{port}") };
            format!("http://{slug}.{}{port}/", config.general.suffix.as_str())
        })
        .unwrap_or_default();
    // A suggestion about the final name is not an error.
    let blocking = error.starts_with("Use") || error.contains("already used");
    b.set_add_error(if blocking { error.into() } else { SharedString::new() });
    b.set_add_preview(if blocking { SharedString::new() } else { preview.into() });
}

fn selected_kind(w: &AppWindow) -> Kind {
    KINDS[w.global::<Backend>().get_kind_index().clamp(0, KINDS.len() as i32 - 1) as usize]
}

fn load_versions(w: &AppWindow, layout: &Layout) {
    let b = w.global::<Backend>();
    let kind = selected_kind(w);
    let config = Config::load(layout).unwrap_or_default();
    let active = service::select(layout, kind, config.version_selector(kind.id())).map(|i| i.version);
    let rows = service::installed(layout, kind)
        .into_iter()
        .map(|i| VersionRow {
            version: i.version.to_string().into(),
            active: Some(&i.version) == active.as_ref(),
            installed: true,
            verified: true,
        })
        .collect();
    b.set_installed_versions(model(rows));
    b.set_available_versions(model(Vec::new()));
    b.set_install_progress(SharedString::new());
}

fn provider_releases(layout: &Layout, kind: Kind) -> Result<Vec<VersionRow>> {
    let provider = meow_core::provider::for_service(kind)
        .ok_or_else(|| anyhow::anyhow!("this service cannot be downloaded on this system"))?;
    let installed = service::installed(layout, kind);
    Ok(provider
        .releases(&meow_core::http::Http::new())?
        .into_iter()
        .map(|r| VersionRow {
            version: r.version.to_string().into(),
            active: false,
            installed: installed.iter().any(|i| i.version == r.version),
            verified: r.assets.iter().all(|a| a.has_checksum()),
        })
        .collect())
}

fn selected_php(w: &AppWindow, layout: &Layout) -> Option<service::Installed> {
    let all = service::installed(layout, Kind::Php);
    let index = w.global::<Backend>().get_php_index().max(0) as usize;
    all.into_iter().nth(index)
}

fn load_php(w: &AppWindow, layout: &Layout) {
    let b = w.global::<Backend>();
    let all = service::installed(layout, Kind::Php);
    b.set_php_versions(strings(all.iter().map(|i| i.version.to_string())));
    if b.get_php_index() as usize >= all.len() {
        b.set_php_index(0);
    }
    let config = Config::load(layout).unwrap_or_default();
    let extensions = selected_php(w, layout).map(|i| php::available_extensions(&i)).unwrap_or_default();
    let enabled: BTreeSet<String> =
        extensions.iter().filter(|e| config.php.extensions.contains(e)).cloned().collect();
    let settings: Vec<SettingRow> = INI_FIELDS
        .iter()
        .map(|(key, label)| {
            let value =
                config.php.ini.get(*key).cloned().or_else(|| php::default_setting(key).map(str::to_owned));
            SettingRow { key: (*key).into(), label: t(label).into(), value: value.unwrap_or_default().into() }
        })
        .collect();
    LOCAL.with(|l| {
        let mut l = l.borrow_mut();
        l.php_ext = enabled;
        l.php_available = extensions;
        l.php_ini = settings.iter().map(|s| (s.key.to_string(), s.value.to_string())).collect();
    });
    b.set_ini_settings(model(settings));
    show_extensions(w);
}

/// The extension list, narrowed by the search box. Ticks survive searching.
fn show_extensions(w: &AppWindow) {
    let b = w.global::<Backend>();
    let filter = b.get_extension_filter().trim().to_lowercase();
    LOCAL.with(|l| {
        let l = l.borrow();
        let rows: Vec<ExtRow> = l
            .php_available
            .iter()
            .filter(|e| filter.is_empty() || e.contains(&filter))
            .map(|e| ExtRow { name: e.clone().into(), enabled: l.php_ext.contains(e) })
            .collect();
        b.set_extensions(model(rows));
        b.set_extensions_total(l.php_available.len() as i32);
        b.set_extensions_enabled(l.php_ext.len() as i32);
    });
}

fn run_doctor(ui: &Ui) {
    let ui = ui.clone();
    std::thread::spawn(move || {
        let problems: Vec<(String, String, String)> = doctor::run(&ui.layout)
            .into_iter()
            .filter(|c| !c.ok)
            .map(|c| (c.name, c.detail, c.fix.unwrap_or_default()))
            .collect();
        ui.on_ui(move |w| {
            let rows = problems
                .into_iter()
                .map(|(name, detail, fix)| CheckRow {
                    name: name.into(),
                    ok: false,
                    detail: detail.into(),
                    fix: fix.into(),
                })
                .collect();
            w.global::<Backend>().set_problems(model(rows));
        });
    });
}

/// Looks for Laragon, XAMPP, WampServer, and FlyEnv off the UI thread
/// (it measures their data folders) and offers an import for each found.
fn detect_importers(ui: &Ui) {
    let ui = ui.clone();
    std::thread::spawn(move || {
        let found: Vec<(&str, &str)> =
            import::detect_all().iter().map(|s| (s.tool.id(), s.tool.name())).collect();
        ui.on_ui(move |w| {
            let rows =
                found.into_iter().map(|(id, name)| Choice { id: id.into(), name: name.into() }).collect();
            w.global::<Backend>().set_importers(model(rows));
        });
    });
}

fn reload_in_background(ui: &Ui) {
    let ui2 = ui.clone();
    std::thread::spawn(move || {
        actions::reload(&ui2.layout);
        ui2.refresh();
    });
}

fn edit_config(layout: &Layout, change: impl FnOnce(&mut Config) -> Result<()>) -> Result<()> {
    let mut config = Config::load(layout)?;
    change(&mut config)?;
    manage::tidy(&mut config);
    config.save(layout)
}

fn show(w: &AppWindow, text: &str, error: bool) {
    let b = w.global::<Backend>();
    b.set_message(text.into());
    b.set_message_is_error(error);
}

fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(rows))
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    model(items.into_iter().map(SharedString::from).collect())
}
