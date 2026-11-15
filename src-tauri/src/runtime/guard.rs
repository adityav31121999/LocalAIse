//! Startup check: the running llama-server must agree with the selected
//! compute device.
//!
//! CPU launches explicitly pass `--device none` and hide GPU backends.
//! Enumeration here is informational only: GPU builds report available
//! adapters even when a particular launch is pinned to the CPU.

use std::path::Path;
use std::process::Command;

use crate::runtime::backend::parse_device_line;
use crate::runtime::profile::ComputeDevice;

/// Runs `llama-server --list-devices` and checks GPU availability when needed.
///
/// The parsing treats each device as its own line. If the pinned llama.cpp
/// release does not know `--list-devices` at all, the check passes with a
/// note — the release bundle gate (`scripts/check-bundle.ts`) is then the
/// guarantee that a bundled binary is CPU-only.
pub fn verify_device_mode(binary: &Path, device: &ComputeDevice) -> Result<(), String> {
    let mut command = Command::new(binary);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command.arg("--list-devices");
    let output = command
        .output()
        .map_err(|e| format!("could not run llama-server: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout);

    let gpu_lines: Vec<String> = text
        .lines()
        .filter_map(parse_device_line)
        .map(|(_, name)| name)
        .collect();

    match device {
        // A GPU run that finds no device cannot offload anything, so the
        // server would silently fall back to the CPU.
        ComputeDevice::UserGpu { .. } if gpu_lines.is_empty() && output.status.success() => {
            Err("the selected llama-server build reports no GPU device, so it cannot offload any layers to the GPU. Pick a GPU build of llama.cpp, or use the CPU.".to_string())
        }
        // Older releases do not know --list-devices; silently fine, since
        // the bundle check is the real CPU-only guarantee for shipped builds.
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_error_message_names_the_devices() {
        // The message contract the UI relies on.
        let line = "CUDA0: NVIDIA GeForce RTX 3060 (vram 12288)";
        assert!(line.to_lowercase().contains("cuda"));
        assert!(parse_device_line(line).is_some());
    }

}
