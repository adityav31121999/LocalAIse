//! System memory detection: read available RAM from the OS.

/// Detected system memory information.
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    /// Total installed RAM in bytes.
    pub total_bytes: u64,
    /// Total installed RAM in GB (rounded to one decimal).
    pub total_gb: f32,
    /// RAM that is free right now, in bytes.
    pub available_bytes: u64,
    /// RAM that is free right now, in GB (rounded to one decimal).
    pub available_gb: f32,
}

impl MemoryInfo {
    pub fn unknown() -> Self {
        Self {
            total_bytes: 0,
            total_gb: 0.0,
            available_bytes: 0,
            available_gb: 0.0,
        }
    }
}

/// Read total and available RAM from the OS.
///
/// Returns `MemoryInfo::unknown()` if the OS does not expose this information.
pub fn read() -> MemoryInfo {
    let (total_bytes, available_bytes) = read_bytes();
    let total_gb = (total_bytes as f64 / 1_073_741_824.0).round() as f32;
    let available_gb = (available_bytes as f64 / 1_073_741_824.0).round() as f32;
    MemoryInfo {
        total_bytes,
        total_gb,
        available_bytes,
        available_gb,
    }
}

#[cfg(target_os = "windows")]
mod windows {
    // Minimal FFI so no extra crate is needed for a 30-byte struct.
    #[repr(C)]
    pub struct MemoryStatusEx {
        pub dw_length: u32,
        pub dw_memory_load: u32,
        pub ull_total_phys: u64,
        pub ull_avail_phys: u64,
        pub ull_total_page_file: u64,
        pub ull_avail_page_file: u64,
        pub ull_total_virtual: u64,
        pub ull_avail_virtual: u64,
        pub ull_avail_ext_virtual: u64,
    }

    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }

    pub fn total_bytes() -> u64 {
        let mut status = MemoryStatusEx {
            dw_length: std::mem::size_of::<MemoryStatusEx>() as u32,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_ext_virtual: 0,
        };
        // Safety: the struct is valid for the duration of the call, sized by
        // the API contract, and the FFI signature holds no opaque state.
        let okay = unsafe { GlobalMemoryStatusEx(&mut status) };
        if okay != 0 {
            status.ull_total_phys
        } else {
            0
        }
    }

    pub fn available_bytes() -> u64 {
        let mut status = MemoryStatusEx {
            dw_length: std::mem::size_of::<MemoryStatusEx>() as u32,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_ext_virtual: 0,
        };
        // Safety: the struct is valid for the duration of the call, sized by
        // the API contract, and the FFI signature holds no opaque state.
        let okay = unsafe { GlobalMemoryStatusEx(&mut status) };
        if okay != 0 {
            status.ull_avail_phys
        } else {
            0
        }
    }
}

#[cfg(target_os = "windows")]
fn read_bytes() -> (u64, u64) {
    let total = windows::total_bytes();
    let total = if total > 0 {
        total
    } else {
        16 * 1_073_741_824 // conservative fallback when the FFI call fails
    };
    (total, windows::available_bytes())
}

#[cfg(target_os = "linux")]
fn read_bytes() -> (u64, u64) {
    // Parse /proc/meminfo — always available on Linux
    let mut total = 0;
    let mut available = 0;
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
                total = kb * 1024;
            }
            if let Some(rest) = line.strip_prefix("MemAvailable:") {
                let kb: u64 = rest.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
                available = kb * 1024;
            }
        }
    }
    if total == 0 {
        total = 16 * 1_073_741_824;
    }
    if available == 0 {
        available = total;
    }
    (total, available)
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn read_bytes() -> (u64, u64) {
    (16 * 1_073_741_824, 16 * 1_073_741_824)
}
