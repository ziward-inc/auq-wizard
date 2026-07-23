mod broker;
mod cli;
mod database;
mod install;
mod preferences;
mod protocol;

use std::{sync::Arc, time::Duration};

use broker::{show_main_window, AppState, Broker};
use database::Database;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, RunEvent, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_window_state::StateFlags;

pub fn main_entry() {
    if cli::is_cli_invocation() {
        std::process::exit(cli::run());
    }
    run();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::POSITION | StateFlags::SIZE)
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(1_000_000)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            broker::get_active_request,
            broker::get_queue_summary,
            broker::submit_answer,
            broker::cancel_request,
            install::get_integration_status,
            install::install_integrations,
            install::trust_codex_hooks,
            preferences::get_auq_enabled,
            preferences::set_auq_enabled,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let database_path = app.path().app_data_dir()?.join("queue.sqlite3");
            let database = Database::open(&database_path)?;
            let _ = database.cleanup_completed();
            let broker = Arc::new(Broker::new(database));
            app.manage(AppState {
                broker: Arc::clone(&broker),
            });
            setup_tray(app, Arc::clone(&broker))?;

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = broker::run_socket_server(app_handle, broker).await {
                    log::error!("AUQ broker stopped: {error:#}");
                }
            });

            let app_handle = app.handle().clone();
            let cleanup_broker = Arc::clone(&app.state::<AppState>().broker);
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60 * 60)).await;
                    if let Err(error) = cleanup_broker.database.cleanup_completed() {
                        log::warn!("failed to clean completed requests: {error:#}");
                    }
                    cleanup_broker.changed(&app_handle);
                }
            });

            if !std::env::args().any(|argument| argument == "--background") {
                show_main_window(app.handle());
            }
            Ok(())
        });

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building AUQ Wizard");
    app.run(|app, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            let state = app.state::<AppState>();
            if !state.broker.is_shutting_down() {
                api.prevent_exit();
                state.broker.request_shutdown();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    app.exit(0);
                });
            }
        }
        RunEvent::Exit => {
            let _ = std::fs::remove_file(broker::socket_path());
        }
        _ => {}
    });
}

fn setup_tray(app: &mut tauri::App, broker: Arc<Broker>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open AUQ Wizard", true, None::<&str>)?;
    let pending_count = broker
        .database
        .summary()
        .map(|summary| summary.pending)
        .unwrap_or(0);
    let pending = MenuItem::with_id(
        app,
        "pending",
        format!("Pending: {pending_count}"),
        false,
        None::<&str>,
    )?;
    broker.set_pending_item(pending.clone());
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pending, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip(if preferences::is_enabled().unwrap_or(false) {
            format!("AUQ Wizard · {pending_count} pending")
        } else {
            format!("AUQ Wizard · paused · {pending_count} pending")
        })
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => {
                broker.request_shutdown();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    app.exit(0);
                });
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
