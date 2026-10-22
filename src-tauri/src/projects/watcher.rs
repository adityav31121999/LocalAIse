//! Background watcher: keeps every project's index in step with its folder.
//!
//! A single thread scans the registered projects every so often. When a file
//! is new, modified or has never been indexed, it is re-parsed, re-chunked and
//! written back to the project's index, and the UI is told to refresh its
//! sources panel. Deletions are noticed the same way: files that sit in the
//! index but are gone from the list have their chunks removed.

use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

/// How often the folder scan runs. Cheap: per project it walks names and
/// stamps only, and only parses what actually changed.
const SCAN_INTERVAL: Duration = Duration::from_secs(15);

pub fn start(app: &AppHandle) {
    let handle = app.clone();
    let _ = std::thread::Builder::new()
        .name("LocalAIse-watch".into())
        .spawn(move || loop {
            std::thread::sleep(SCAN_INTERVAL);
            watch_once(&handle);
        });
}

/// One scan across all registered projects. Quietly skipped when the app is
/// shutting down or the state is briefly locked behind a command.
fn watch_once(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let Ok(registry) = state.registry() else {
        return;
    };
    let projects = registry.projects.clone();
    drop(registry);

    for project in projects {
        if reindex_changed(&project.id, &project.path) {
            app.emit(
                "sources://updated",
                json!({ "projectId": project.id, "path": project.path }),
            )
            .ok();
        }
    }
}

/// Re-indexes every source whose modification stamp differs from the index.
/// Returns true when anything changed, so the UI can be nudged.
fn reindex_changed(project_id: &str, root: &std::path::Path) -> bool {
    let index_db = crate::index::db::IndexDb::new(root);
    let index = index_db.load();

    let sources = match crate::projects::manager::scan_sources(project_id, root) {
        Ok(sources) => sources,
        Err(_) => return false,
    };

    let mut changed = false;

    // New or modified files.
    for source in &sources {
        let stale = index
            .indexed_files
            .get(&source.name)
            .map(|stamp| stamp != &source.modified_at)
            .unwrap_or(true);
        if !stale {
            continue;
        }
        let rel = std::path::Path::new(&source.name);
        if let Ok(chunks) = crate::ingest::pipeline::ingest_file(root, rel) {
            let _ = index_db.insert_chunks(&source.name, &source.modified_at, chunks);
            changed = true;
        }
    }

    // Files that vanished.
    for name in index.indexed_files.keys() {
        if !sources.iter().any(|source| source.name == *name) {
            let _ = index_db.remove_file(name);
            changed = true;
        }
    }

    changed
}
