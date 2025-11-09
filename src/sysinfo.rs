use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: OsInfo,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disks: Vec<DiskInfo>,
    pub network: NetworkInfo,
    pub hostname: String,
    pub uptime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub name: String,
    pub version: String,
    pub kernel: String,
    pub architecture: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub model: String,
    pub cores: usize,
    pub threads: Option<usize>,
    pub frequency: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total: String,
    pub available: String,
    pub used: String,
    pub percent_used: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub device: String,
    pub mount_point: String,
    pub filesystem: String,
    pub total: String,
    pub used: String,
    pub available: String,
    pub percent_used: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub interfaces: Vec<NetworkInterface>,
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ip_address: Option<String>,
    pub mac_address: Option<String>,
}

/// Gather comprehensive system information
pub fn gather_system_info() -> Result<SystemInfo> {
    Ok(SystemInfo {
        os: get_os_info()?,
        cpu: get_cpu_info()?,
        memory: get_memory_info()?,
        disks: get_disk_info()?,
        network: get_network_info()?,
        hostname: get_hostname()?,
        uptime: get_uptime().ok(),
    })
}

/// Get OS information
fn get_os_info() -> Result<OsInfo> {
    let platform = std::env::consts::OS;

    match platform {
        "macos" => get_macos_info(),
        "linux" => get_linux_info(),
        _ => Ok(OsInfo {
            name: platform.to_string(),
            version: "Unknown".to_string(),
            kernel: get_kernel_version().unwrap_or_else(|_| "Unknown".to_string()),
            architecture: std::env::consts::ARCH.to_string(),
            platform: platform.to_string(),
        }),
    }
}

fn get_macos_info() -> Result<OsInfo> {
    let version_output = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .context("Failed to get macOS version")?;

    let version = String::from_utf8_lossy(&version_output.stdout)
        .trim()
        .to_string();

    let name_output = Command::new("sw_vers")
        .arg("-productName")
        .output()
        .context("Failed to get macOS name")?;

    let name = String::from_utf8_lossy(&name_output.stdout)
        .trim()
        .to_string();

    Ok(OsInfo {
        name,
        version,
        kernel: get_kernel_version()?,
        architecture: std::env::consts::ARCH.to_string(),
        platform: "macos".to_string(),
    })
}

fn get_linux_info() -> Result<OsInfo> {
    // Try to read /etc/os-release
    let os_release = fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();

    let mut name = "Linux".to_string();
    let mut version = "Unknown".to_string();

    for line in os_release.lines() {
        if let Some(value) = line.strip_prefix("NAME=") {
            name = value.trim_matches('"').to_string();
        } else if let Some(value) = line.strip_prefix("VERSION=") {
            version = value.trim_matches('"').to_string();
        } else if let Some(value) = line.strip_prefix("VERSION_ID=") {
            if version == "Unknown" {
                version = value.trim_matches('"').to_string();
            }
        }
    }

    Ok(OsInfo {
        name,
        version,
        kernel: get_kernel_version()?,
        architecture: std::env::consts::ARCH.to_string(),
        platform: "linux".to_string(),
    })
}

fn get_kernel_version() -> Result<String> {
    let output = Command::new("uname")
        .arg("-r")
        .output()
        .context("Failed to get kernel version")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get CPU information
fn get_cpu_info() -> Result<CpuInfo> {
    let platform = std::env::consts::OS;

    match platform {
        "macos" => get_macos_cpu_info(),
        "linux" => get_linux_cpu_info(),
        _ => Ok(CpuInfo {
            model: "Unknown".to_string(),
            cores: num_cpus::get_physical(),
            threads: Some(num_cpus::get()),
            frequency: None,
        }),
    }
}

fn get_macos_cpu_info() -> Result<CpuInfo> {
    let model_output = Command::new("sysctl")
        .arg("-n")
        .arg("machdep.cpu.brand_string")
        .output()
        .context("Failed to get CPU model")?;

    let model = String::from_utf8_lossy(&model_output.stdout)
        .trim()
        .to_string();

    let cores_output = Command::new("sysctl")
        .arg("-n")
        .arg("hw.physicalcpu")
        .output()
        .context("Failed to get CPU cores")?;

    let cores = String::from_utf8_lossy(&cores_output.stdout)
        .trim()
        .parse()
        .unwrap_or(1);

    let threads_output = Command::new("sysctl")
        .arg("-n")
        .arg("hw.logicalcpu")
        .output()
        .ok();

    let threads =
        threads_output.and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());

    let freq_output = Command::new("sysctl")
        .arg("-n")
        .arg("hw.cpufrequency")
        .output()
        .ok();

    let frequency = freq_output.and_then(|o| {
        let freq_hz: u64 = String::from_utf8_lossy(&o.stdout).trim().parse().ok()?;
        Some(format!("{:.2} GHz", freq_hz as f64 / 1_000_000_000.0))
    });

    Ok(CpuInfo {
        model,
        cores,
        threads,
        frequency,
    })
}

fn get_linux_cpu_info() -> Result<CpuInfo> {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").context("Failed to read /proc/cpuinfo")?;

    let mut model = "Unknown".to_string();
    let mut frequency = None;

    for line in cpuinfo.lines() {
        if line.starts_with("model name") {
            if let Some(value) = line.split(':').nth(1) {
                model = value.trim().to_string();
            }
        } else if line.starts_with("cpu MHz") {
            if let Some(value) = line.split(':').nth(1) {
                if let Ok(mhz) = value.trim().parse::<f64>() {
                    frequency = Some(format!("{:.2} GHz", mhz / 1000.0));
                }
            }
        }
    }

    Ok(CpuInfo {
        model,
        cores: num_cpus::get_physical(),
        threads: Some(num_cpus::get()),
        frequency,
    })
}

/// Get memory information
fn get_memory_info() -> Result<MemoryInfo> {
    let platform = std::env::consts::OS;

    match platform {
        "macos" => get_macos_memory_info(),
        "linux" => get_linux_memory_info(),
        _ => Ok(MemoryInfo {
            total: "Unknown".to_string(),
            available: "Unknown".to_string(),
            used: "Unknown".to_string(),
            percent_used: 0.0,
        }),
    }
}

fn get_macos_memory_info() -> Result<MemoryInfo> {
    let total_output = Command::new("sysctl")
        .arg("-n")
        .arg("hw.memsize")
        .output()
        .context("Failed to get total memory")?;

    let total_bytes: u64 = String::from_utf8_lossy(&total_output.stdout)
        .trim()
        .parse()
        .unwrap_or(0);

    let vm_stat_output = Command::new("vm_stat")
        .output()
        .context("Failed to get memory stats")?;

    let vm_stat = String::from_utf8_lossy(&vm_stat_output.stdout);

    // Parse vm_stat output
    let mut free_pages = 0u64;
    let mut inactive_pages = 0u64;
    let page_size = 4096u64; // macOS page size

    for line in vm_stat.lines() {
        if line.starts_with("Pages free:") {
            if let Some(value) = line.split(':').nth(1) {
                free_pages = value.trim().trim_end_matches('.').parse().unwrap_or(0);
            }
        } else if line.starts_with("Pages inactive:") {
            if let Some(value) = line.split(':').nth(1) {
                inactive_pages = value.trim().trim_end_matches('.').parse().unwrap_or(0);
            }
        }
    }

    let available_bytes = (free_pages + inactive_pages) * page_size;
    let used_bytes = total_bytes.saturating_sub(available_bytes);
    let percent_used = if total_bytes > 0 {
        (used_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(MemoryInfo {
        total: format_bytes(total_bytes),
        available: format_bytes(available_bytes),
        used: format_bytes(used_bytes),
        percent_used,
    })
}

fn get_linux_memory_info() -> Result<MemoryInfo> {
    let meminfo = fs::read_to_string("/proc/meminfo").context("Failed to read /proc/meminfo")?;

    let mut total = 0u64;
    let mut available = 0u64;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            if let Some(value) = line.split_whitespace().nth(1) {
                total = value.parse().unwrap_or(0);
            }
        } else if line.starts_with("MemAvailable:") {
            if let Some(value) = line.split_whitespace().nth(1) {
                available = value.parse().unwrap_or(0);
            }
        }
    }

    // Convert from KB to bytes
    let total_bytes = total * 1024;
    let available_bytes = available * 1024;
    let used_bytes = total_bytes.saturating_sub(available_bytes);
    let percent_used = if total_bytes > 0 {
        (used_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(MemoryInfo {
        total: format_bytes(total_bytes),
        available: format_bytes(available_bytes),
        used: format_bytes(used_bytes),
        percent_used,
    })
}

/// Get disk information
fn get_disk_info() -> Result<Vec<DiskInfo>> {
    let platform = std::env::consts::OS;

    match platform {
        "macos" | "linux" => get_df_disk_info(),
        _ => Ok(Vec::new()),
    }
}

fn get_df_disk_info() -> Result<Vec<DiskInfo>> {
    let output = Command::new("df")
        .arg("-h")
        .output()
        .context("Failed to get disk info")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut disks = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 {
            // Skip tmpfs, devfs, and other virtual filesystems
            let device = parts[0];
            if device.starts_with("/dev/") || device.starts_with("map") {
                let filesystem = if parts.len() >= 9 { parts[8] } else { parts[0] };
                let mount_point = parts[parts.len() - 1];

                // Parse percentage
                let percent_str = parts[4].trim_end_matches('%');
                let percent = percent_str.parse::<f64>().unwrap_or(0.0);

                disks.push(DiskInfo {
                    device: device.to_string(),
                    mount_point: mount_point.to_string(),
                    filesystem: filesystem.to_string(),
                    total: parts[1].to_string(),
                    used: parts[2].to_string(),
                    available: parts[3].to_string(),
                    percent_used: percent,
                });
            }
        }
    }

    Ok(disks)
}

/// Get network information
fn get_network_info() -> Result<NetworkInfo> {
    let hostname = get_hostname()?;
    let interfaces = get_network_interfaces()?;

    Ok(NetworkInfo {
        interfaces,
        hostname,
    })
}

fn get_network_interfaces() -> Result<Vec<NetworkInterface>> {
    let platform = std::env::consts::OS;

    match platform {
        "macos" | "linux" => get_ifconfig_interfaces(),
        _ => Ok(Vec::new()),
    }
}

fn get_ifconfig_interfaces() -> Result<Vec<NetworkInterface>> {
    let output = Command::new("ifconfig")
        .output()
        .or_else(|_| Command::new("ip").arg("addr").output())
        .context("Failed to get network interfaces")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut interfaces = Vec::new();
    let mut current_name = String::new();
    let mut current_ip = None;
    let mut current_mac = None;

    for line in stdout.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') && line.contains(':') {
            // New interface - save previous one
            if !current_name.is_empty() {
                interfaces.push(NetworkInterface {
                    name: current_name.clone(),
                    ip_address: current_ip.clone(),
                    mac_address: current_mac.clone(),
                });
            }

            // Parse interface name
            current_name = line.split(':').next().unwrap_or("").trim().to_string();
            current_ip = None;
            current_mac = None;
        } else if line.contains("inet ") && !line.contains("inet6") {
            // Parse IP address
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(pos) = parts.iter().position(|&s| s == "inet") {
                if parts.len() > pos + 1 {
                    current_ip = Some(parts[pos + 1].to_string());
                }
            }
        } else if line.contains("ether ") {
            // Parse MAC address
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(pos) = parts.iter().position(|&s| s == "ether") {
                if parts.len() > pos + 1 {
                    current_mac = Some(parts[pos + 1].to_string());
                }
            }
        }
    }

    // Add last interface
    if !current_name.is_empty() {
        interfaces.push(NetworkInterface {
            name: current_name,
            ip_address: current_ip,
            mac_address: current_mac,
        });
    }

    Ok(interfaces)
}

fn get_hostname() -> Result<String> {
    let output = Command::new("hostname")
        .output()
        .context("Failed to get hostname")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_uptime() -> Result<String> {
    let output = Command::new("uptime")
        .output()
        .context("Failed to get uptime")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Format bytes into human-readable format
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}
