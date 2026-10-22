//! Hashing helpers. Project and page ids are derived from stable hashes so the
//! same folder always maps to the same project.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// FNV-1a, 64-bit. Not cryptographic — used for identity, not for integrity.
pub fn fnv1a_64(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn short_id(input: &str) -> String {
    format!("{:016x}", fnv1a_64(input))
}

/// An id that is unique per call: a short hash seeded with the clock and a
/// counter, so pages created in the same millisecond still differ.
pub fn unique_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{:016x}", fnv1a_64(&format!("{nanos}-{sequence}")))
}

/// Lowercase, dash-separated form of a title, safe to use in a file name.
pub fn slug(input: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;

    for character in input.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            out.push(character.to_ascii_lowercase());
            pending_dash = false;
        } else if !out.is_empty() {
            pending_dash = true;
        }
    }

    if out.is_empty() {
        "untitled".to_string()
    } else {
        out
    }
}
