//! Cooperative cancellation for a running chat turn.
//!
//! A chat command runs the model loop, so the UI cannot stop it by dropping
//! its event listener alone. `chat_stop` flips a flag on the current run and
//! every long step (each streamed model call and each tool call) checks it.
//!
//! Each run gets its own flag. If a stopped user sends the next message right
//! away, the new run installs a fresh flag and the old loop keeps its own, so
//! a late cancel never touches a newer turn.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

fn runs() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static RUNS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Marks one chat turn as running and removes it when the turn ends.
pub struct RunGuard {
    project_id: String,
    flag: Arc<AtomicBool>,
}

impl RunGuard {
    /// Registers a new run for the project, replacing any finished one.
    pub fn begin(project_id: &str) -> Self {
        let flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut map) = runs().lock() {
            map.insert(project_id.to_string(), flag.clone());
        }
        Self {
            project_id: project_id.to_string(),
            flag,
        }
    }

    /// True once the user asked this run to stop.
    pub fn cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = runs().lock() {
            // Only clear the slot when this run still owns it; a newer run for
            // the same project must survive this one ending.
            let owns_slot = map
                .get(&self.project_id)
                .is_some_and(|flag| Arc::ptr_eq(flag, &self.flag));
            if owns_slot {
                map.remove(&self.project_id);
            }
        }
    }
}

/// Asks the project's current run to stop. A no-op when nothing is running.
pub fn cancel(project_id: &str) {
    if let Ok(map) = runs().lock() {
        if let Some(flag) = map.get(project_id) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_only_reaches_the_run_it_was_aimed_at() {
        let first = RunGuard::begin("cancel-test-project");
        assert!(!first.cancelled());

        cancel("cancel-test-project");
        assert!(first.cancelled());

        // A newer turn installs its own flag; the old turn's stop must not
        // leak into it, and the old turn stays stopped.
        let second = RunGuard::begin("cancel-test-project");
        assert!(!second.cancelled());
        assert!(first.cancelled());

        // Dropping the old run must not unregister the new one.
        drop(first);
        cancel("cancel-test-project");
        assert!(second.cancelled());
    }
}
