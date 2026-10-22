use std::sync::atomic::{AtomicU64, Ordering};

use crate::util::hash;
use crate::util::time;

/// A fresh token per session. llama-server requires it, so another local
/// process cannot drive the model behind the app's back.
pub fn generate() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let seed = format!("{}-{}-{sequence}", time::now_iso(), std::process::id());

    format!(
        "{}{}",
        hash::short_id(&seed),
        hash::short_id(&format!("{seed}-second-half"))
    )
}
