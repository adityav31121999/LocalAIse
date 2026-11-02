//! Updater module: background check scheduler, state persistence, and Tauri commands.

pub mod checker;
pub mod commands;
pub mod state;

use crate::updater::checker::spawn_checker;
use crate::updater::state::{UpdaterState, UpdaterStateInner};
use tauri::Manager;

/// Initialize the updater: load state, register commands, spawn background checker.
pub fn init(app: &tauri::AppHandle) {
    let state = UpdaterStateInner::load(app);
    app.manage(UpdaterState::new(state));
    spawn_checker(app.clone());
}