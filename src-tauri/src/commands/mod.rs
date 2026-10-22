//! Thin Tauri entry points.
//!
//! Each command validates its arguments, takes the app's state, and calls into
//! the module that owns the work. Nothing here holds business logic, so the
//! same functions stay testable without a window.

pub mod activity;
pub mod chat;
pub mod files;
pub mod gpu;
pub mod models;
pub mod personal;
pub mod projects;
pub mod research;
pub mod runtime;
