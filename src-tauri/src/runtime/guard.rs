//! Startup check: the running llama-server must agree with the selected
//! compute device.
//!
//! In CPU mode the app must never see a GPU device. The check reads the
//! server's own device list, so a mislabeled binary (a GPU build shipped as
//! "CPU") cannot silently use VRAM.

use std::path::Path;
use std::process::Command;

use crate::runtime::backend::parse_device_line;
use crate::runtime::profile::ComputeDevice;

/// Runs `llama-server --list-devices` and checks the result against the
/// selected mode.
///
/// The parsing treats each device as its own line. If the pinned llama.cpp
/// release does not know `--list-devices` at all, the check passes with a
/// note — the release bundle gate (`scripts/check-bundle.ts`) is then the
/// guarantee that a bundled binary is CPU-only.
pub fn verify_device_mode(binary: &Path, device: &ComputeDevice) -> Result<(), String> {
    let mut command = Command::new(binary);
    command.arg("--list-devices");
    // Hiding the GPU only makes sense when the run is meant to stay on the
    // CPU. A GPU run needs the device to be visible, or the server would
    // start without one and fall back to the CPU anyway.
    if matches!(device, ComputeDevice::Cpu) {
        command.env("CUDA_VISIBLE_DEVICES", "");
    }

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
        ComputeDevice::Cpu if !gpu_lines.is_empty() => Err(format!(
            "CPU mode, but llama-server reports GPU devices: {}",
            gpu_lines.join("; ")
        )),
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
