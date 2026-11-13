//! Thin Tauri shell: window, events, commands. All logic lives in melfield-core.

mod commands;

use std::path::PathBuf;
use std::sync::Arc;

use melfield_core::{Config, CoreEvent, EventSink, Melfield};
use tauri::{AppHandle, Emitter, Manager};

pub struct AppState {
    pub mf: Arc<Melfield>,
    pub api: tokio::sync::Mutex<Option<melfield_api::ApiServer>>,
}

struct TauriSink(AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, event: CoreEvent) {
        let _ = self.0.emit("melfield://event", event);
    }
}

fn uv_path() -> PathBuf {
    // Tauri places externalBin sidecars next to the main executable (without the target triple).
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    exe_dir.join(if cfg!(windows) { "uv.exe" } else { "uv" })
}

fn init_logging(dir: &std::path::Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let appender = tracing_appender::rolling::daily(dir, "melfield.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = std::env::var("MELFIELD_LOG").unwrap_or_else(|_| "info,wgpu=warn".into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(writer)
        .try_init()
        .ok()?;
    Some(guard)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let resources = app
                .path()
                .resource_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            // In dev, resources come straight from the repo.
            let resources_dir = if resources.join("workers").exists() {
                resources
            } else {
                PathBuf::from("../..")
            };
            let config = Config {
                root: None,
                resources_dir,
                uv_path: uv_path(),
                app_version: app.package_info().version.to_string(),
                headless: false,
            };
            let sink = Arc::new(TauriSink(app.handle().clone()));
            let mf = tauri::async_runtime::block_on(async { Melfield::new(config, sink) })
                .map_err(|e| format!("melfield failed to start: {}", e.detail))?;
            let guard = init_logging(&mf.paths().logs());
            app.manage(guard);
            tracing::info!(version = %app.package_info().version, "melfield started");
            let state = AppState {
                mf: mf.clone(),
                api: tokio::sync::Mutex::new(None),
            };
            app.manage(state);
            // Start the local API if it was left on.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if mf.settings().map(|s| s.api_enabled).unwrap_or(false) {
                    let _ = commands::start_api(&handle).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_state,
            commands::voices,
            commands::get_voice,
            commands::install_recipe,
            commands::cancel_pull,
            commands::remove_voice,
            commands::voices_sharing_files,
            commands::accept_license,
            commands::start_setup,
            commands::speak,
            commands::cancel_job,
            commands::preview_voice,
            commands::play_generation,
            commands::player_control,
            commands::history,
            commands::delete_generation,
            commands::set_favourite,
            commands::export_generation,
            commands::clone_plan,
            commands::start_recording,
            commands::stop_recording,
            commands::import_audio,
            commands::play_recording,
            commands::create_clone,
            commands::update_settings,
            commands::disk_usage,
            commands::move_storage,
            commands::export_logs,
            commands::engines,
            commands::repair_engine,
            commands::api_status,
            commands::set_api_enabled,
            commands::hf_oauth_available,
            commands::hf_oauth_begin,
            commands::hf_sign_in_with_token,
            commands::hf_sign_out,
            commands::hf_check_access,
            commands::open_url,
            commands::open_data_dir,
            commands::reveal_path,
            commands::open_notices,
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // Last-resort: we cannot show UI here; log to stderr and exit calmly.
            eprintln!("melfield could not start: {e}");
            std::process::exit(1);
        })
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.mf.shutdown();
                }
            }
        });
}
