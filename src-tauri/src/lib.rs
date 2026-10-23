//! localAIse core.
//!
//! The Rust side owns everything that touches the disk, the model files and the
//! llama-server process. The frontend only asks for results over Tauri commands,
//! so a folder never has to be exposed to the web view.

pub mod activity;
pub mod agent;
pub mod commands;
pub mod config;
pub mod error;
pub mod hardware;
pub mod index;
pub mod ingest;
pub mod models;
pub mod network;
pub mod personal;
pub mod projects;
pub mod retrieval;
pub mod runtime;
pub mod sandbox;
pub mod tools;
pub mod util;

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use tauri::{AppHandle, Manager};

use crate::config::app_config::Registry;
use crate::config::paths::AppDirs;
use crate::error::{AppError, AppResult};
use crate::runtime::backend::CapabilityCache;
use crate::runtime::supervisor::Supervisor;

/// Everything a command needs: where the app keeps its own files, which folders
/// the user has registered, and the single llama-server this session runs.
pub struct AppState {
    pub dirs: AppDirs,
    pub registry: Mutex<Registry>,
    pub supervisor: Mutex<Supervisor>,
    /// What the llama-server build in use can do. Probing a binary is slow
    /// enough to be worth remembering.
    pub capabilities: Mutex<CapabilityCache>,
}

impl AppState {
    pub fn registry(&self) -> AppResult<MutexGuard<'_, Registry>> {
        self.registry
            .lock()
            .map_err(|_| AppError::msg("the project list is busy; try again"))
    }

    pub fn supervisor(&self) -> AppResult<MutexGuard<'_, Supervisor>> {
        self.supervisor
            .lock()
            .map_err(|_| AppError::msg("the local runtime is busy; try again"))
    }

    pub fn capabilities(&self) -> AppResult<MutexGuard<'_, CapabilityCache>> {
        self.capabilities
            .lock()
            .map_err(|_| AppError::msg("the local runtime is busy; try again"))
    }

    /// The personal space folder, falling back to the default location.
    pub fn personal_space(&self) -> AppResult<PathBuf> {
        Ok(self.registry()?.personal_space_or_default(&self.dirs))
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Ensure writable runtime exists and base CPU files are staged before supervisor launches
            if let Err(e) = crate::runtime::gpu_build::ensure_runtime(app.handle()) {
                eprintln!("[runtime] Warning: ensure_runtime: {e}");
            }

            let dirs = AppDirs::resolve(app.handle())?;
            let registry = Registry::load(&dirs.state_file);

            app.manage(AppState {
                dirs,
                registry: Mutex::new(registry),
                supervisor: Mutex::new(Supervisor::new()),
                capabilities: Mutex::new(CapabilityCache::default()),
            });

            // Keep every project's index in step with its folder's files.
            crate::projects::watcher::start(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::gpu::detect_gpu,
            commands::gpu::list_backends,
            commands::gpu::install_backend,
            commands::gpu::remove_backend,
            commands::gpu::cancel_install,
            commands::projects::list_projects,
            commands::projects::open_project,
            commands::projects::list_sources,
            commands::projects::reindex_sources,
            commands::projects::set_project_internet,
            commands::projects::set_project_system_prompt,
            commands::projects::set_project_diary,
            commands::projects::create_project_folder,
            commands::files::list_project_directory,
            commands::files::create_project_entry,
            commands::files::rename_project_entry,
            commands::files::copy_project_entry,
            commands::files::delete_project_entry,
            commands::projects::list_project_chats,
            commands::projects::read_project_chat,
            commands::projects::delete_project_chat,
            commands::projects::add_project_folder,
            commands::projects::create_project,
            commands::research::analyze_document,
            commands::research::create_research_plan,
            commands::research::execute_research_plan,
            commands::chat::chat_send,
            commands::models::select_model,
            commands::models::delete_model,
            commands::models::list_available_models,
            commands::models::get_selected_model,
            commands::models::list_model_folders,
            commands::models::add_model_folder,
            commands::models::remove_model_folder,
            commands::models::pick_model_folder,
            commands::models::get_hardware_report,
            commands::models::runtime_status,
            commands::models::get_runtime_profile,
            commands::models::get_runtime_capabilities,
            commands::models::validate_runtime_profile,
            commands::models::apply_runtime_profile,
            commands::models::reset_runtime_profile,
            commands::models::get_profile_estimate,
            commands::models::get_server_binary,
            commands::models::pick_server_binary,
            commands::gpu::list_gpu_builds,
            commands::gpu::install_gpu_build,
            commands::personal::list_diary_pages,
            commands::personal::create_diary_page,
            commands::personal::save_diary_page,
            commands::personal::delete_diary_page,
            commands::personal::list_calendar_events,
            commands::personal::create_calendar_event,
            commands::personal::update_calendar_event,
            commands::personal::delete_calendar_event,
            commands::personal::export_calendar_ics,
            commands::personal::import_calendar_ics,
            commands::personal::search_personal,
            commands::activity::list_activity,
        ])
        .build(tauri::generate_context!())
        .expect("could not start localAIse")
        .run(on_event);
}

/// Stops llama-server on the way out, so closing the window never leaves a
/// model loaded in the background.
fn on_event(app: &AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::Exit = event {
        if let Some(state) = app.try_state::<AppState>() {
            if let Ok(mut supervisor) = state.supervisor.lock() {
                supervisor.stop();
            }
        }
    }
}
