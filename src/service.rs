use anyhow::{Context, Result};
use colored::*;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServiceManager {
    Systemd, // Most Linux distros
    Launchd, // macOS
    InitD,   // Old Linux systems
    OpenRC,  // Alpine, Gentoo
    Unknown,
}

impl ServiceManager {
    pub fn name(&self) -> &str {
        match self {
            ServiceManager::Systemd => "systemd",
            ServiceManager::Launchd => "launchd",
            ServiceManager::InitD => "init.d",
            ServiceManager::OpenRC => "OpenRC",
            ServiceManager::Unknown => "unknown",
        }
    }

    pub fn requires_sudo(&self) -> bool {
        match self {
            ServiceManager::Unknown => false,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub status: ServiceStatus,
    pub enabled: Option<bool>,
    pub pid: Option<u32>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}

impl ServiceStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ServiceStatus::Running => "running",
            ServiceStatus::Stopped => "stopped",
            ServiceStatus::Failed => "failed",
            ServiceStatus::Unknown => "unknown",
        }
    }
}

/// Detect the system's service manager
pub fn detect_service_manager() -> Result<ServiceManager> {
    debug!("Detecting service manager...");

    // Check for systemd (most common on modern Linux)
    if is_command_available("systemctl") {
        let output = Command::new("systemctl").arg("--version").output();
        if output.is_ok() && output.unwrap().status.success() {
            info!("Detected service manager: systemd");
            return Ok(ServiceManager::Systemd);
        }
    }

    // Check for launchd (macOS)
    if is_command_available("launchctl") {
        info!("Detected service manager: launchd");
        return Ok(ServiceManager::Launchd);
    }

    // Check for OpenRC
    if is_command_available("rc-service") {
        info!("Detected service manager: OpenRC");
        return Ok(ServiceManager::OpenRC);
    }

    // Check for init.d
    if std::path::Path::new("/etc/init.d").exists() {
        info!("Detected service manager: init.d");
        return Ok(ServiceManager::InitD);
    }

    Ok(ServiceManager::Unknown)
}

/// Check if a command is available in PATH
fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Start a service
pub fn start_service(
    service: &str,
    sm: &ServiceManager,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let mut cmd_parts = Vec::new();

    match sm {
        ServiceManager::Systemd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("systemctl");
            cmd_parts.push("start");
            cmd_parts.push(service);
        }
        ServiceManager::Launchd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("launchctl");
            cmd_parts.push("start");
            cmd_parts.push(service);
        }
        ServiceManager::OpenRC => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("rc-service");
            cmd_parts.push(service);
            cmd_parts.push("start");
        }
        ServiceManager::InitD => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("/etc/init.d");
            cmd_parts.push(service);
            cmd_parts.push("start");
        }
        ServiceManager::Unknown => {
            anyhow::bail!("Unknown service manager - cannot start service");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Stop a service
pub fn stop_service(
    service: &str,
    sm: &ServiceManager,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let mut cmd_parts = Vec::new();

    match sm {
        ServiceManager::Systemd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("systemctl");
            cmd_parts.push("stop");
            cmd_parts.push(service);
        }
        ServiceManager::Launchd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("launchctl");
            cmd_parts.push("stop");
            cmd_parts.push(service);
        }
        ServiceManager::OpenRC => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("rc-service");
            cmd_parts.push(service);
            cmd_parts.push("stop");
        }
        ServiceManager::InitD => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("/etc/init.d");
            cmd_parts.push(service);
            cmd_parts.push("stop");
        }
        ServiceManager::Unknown => {
            anyhow::bail!("Unknown service manager - cannot stop service");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Restart a service
pub fn restart_service(
    service: &str,
    sm: &ServiceManager,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let mut cmd_parts = Vec::new();

    match sm {
        ServiceManager::Systemd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("systemctl");
            cmd_parts.push("restart");
            cmd_parts.push(service);
        }
        ServiceManager::Launchd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("launchctl");
            cmd_parts.push("kickstart");
            cmd_parts.push("-k");
            cmd_parts.push(service);
        }
        ServiceManager::OpenRC => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("rc-service");
            cmd_parts.push(service);
            cmd_parts.push("restart");
        }
        ServiceManager::InitD => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("/etc/init.d");
            cmd_parts.push(service);
            cmd_parts.push("restart");
        }
        ServiceManager::Unknown => {
            anyhow::bail!("Unknown service manager - cannot restart service");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Enable a service to start on boot
pub fn enable_service(
    service: &str,
    sm: &ServiceManager,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let mut cmd_parts = Vec::new();

    match sm {
        ServiceManager::Systemd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("systemctl");
            cmd_parts.push("enable");
            cmd_parts.push(service);
        }
        ServiceManager::Launchd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("launchctl");
            cmd_parts.push("enable");
            cmd_parts.push(service);
        }
        ServiceManager::OpenRC => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("rc-update");
            cmd_parts.push("add");
            cmd_parts.push(service);
            cmd_parts.push("default");
        }
        ServiceManager::InitD => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("update-rc.d");
            cmd_parts.push(service);
            cmd_parts.push("enable");
        }
        ServiceManager::Unknown => {
            anyhow::bail!("Unknown service manager - cannot enable service");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Disable a service from starting on boot
pub fn disable_service(
    service: &str,
    sm: &ServiceManager,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let mut cmd_parts = Vec::new();

    match sm {
        ServiceManager::Systemd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("systemctl");
            cmd_parts.push("disable");
            cmd_parts.push(service);
        }
        ServiceManager::Launchd => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("launchctl");
            cmd_parts.push("disable");
            cmd_parts.push(service);
        }
        ServiceManager::OpenRC => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("rc-update");
            cmd_parts.push("del");
            cmd_parts.push(service);
        }
        ServiceManager::InitD => {
            if sm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("update-rc.d");
            cmd_parts.push(service);
            cmd_parts.push("disable");
        }
        ServiceManager::Unknown => {
            anyhow::bail!("Unknown service manager - cannot disable service");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Get service status
pub fn get_service_status(service: &str, sm: &ServiceManager) -> Result<ServiceInfo> {
    match sm {
        ServiceManager::Systemd => get_systemd_status(service),
        ServiceManager::Launchd => get_launchd_status(service),
        ServiceManager::OpenRC => get_openrc_status(service),
        ServiceManager::InitD => get_initd_status(service),
        ServiceManager::Unknown => Ok(ServiceInfo {
            name: service.to_string(),
            status: ServiceStatus::Unknown,
            enabled: None,
            pid: None,
            description: None,
        }),
    }
}

fn get_systemd_status(service: &str) -> Result<ServiceInfo> {
    let output = Command::new("systemctl")
        .arg("status")
        .arg(service)
        .output()
        .context("Failed to get service status")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse status
    let status = if stdout.contains("Active: active (running)") {
        ServiceStatus::Running
    } else if stdout.contains("Active: inactive") || stdout.contains("Active: dead") {
        ServiceStatus::Stopped
    } else if stdout.contains("Active: failed") {
        ServiceStatus::Failed
    } else {
        ServiceStatus::Unknown
    };

    // Parse PID
    let pid = stdout
        .lines()
        .find(|line| line.contains("Main PID:"))
        .and_then(|line| {
            line.split_whitespace()
                .nth(2)
                .and_then(|s| s.parse::<u32>().ok())
        });

    // Check if enabled
    let enabled_output = Command::new("systemctl")
        .arg("is-enabled")
        .arg(service)
        .output()
        .ok();

    let enabled = enabled_output.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled");

    Ok(ServiceInfo {
        name: service.to_string(),
        status,
        enabled,
        pid,
        description: None,
    })
}

fn get_launchd_status(service: &str) -> Result<ServiceInfo> {
    let output = Command::new("launchctl")
        .arg("list")
        .output()
        .context("Failed to get service status")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check if service is in the list
    let status = if stdout.contains(service) {
        ServiceStatus::Running
    } else {
        ServiceStatus::Stopped
    };

    Ok(ServiceInfo {
        name: service.to_string(),
        status,
        enabled: None,
        pid: None,
        description: None,
    })
}

fn get_openrc_status(service: &str) -> Result<ServiceInfo> {
    let output = Command::new("rc-service")
        .arg(service)
        .arg("status")
        .output()
        .context("Failed to get service status")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let status = if stdout.contains("started") {
        ServiceStatus::Running
    } else if stdout.contains("stopped") {
        ServiceStatus::Stopped
    } else {
        ServiceStatus::Unknown
    };

    Ok(ServiceInfo {
        name: service.to_string(),
        status,
        enabled: None,
        pid: None,
        description: None,
    })
}

fn get_initd_status(service: &str) -> Result<ServiceInfo> {
    let script_path = format!("/etc/init.d/{}", service);
    let output = Command::new(&script_path)
        .arg("status")
        .output()
        .context("Failed to get service status")?;

    let status = if output.status.success() {
        ServiceStatus::Running
    } else {
        ServiceStatus::Stopped
    };

    Ok(ServiceInfo {
        name: service.to_string(),
        status,
        enabled: None,
        pid: None,
        description: None,
    })
}

/// List all services
pub fn list_services(sm: &ServiceManager) -> Result<Vec<ServiceInfo>> {
    match sm {
        ServiceManager::Systemd => list_systemd_services(),
        ServiceManager::Launchd => list_launchd_services(),
        ServiceManager::OpenRC => list_openrc_services(),
        ServiceManager::InitD => list_initd_services(),
        ServiceManager::Unknown => Ok(Vec::new()),
    }
}

fn list_systemd_services() -> Result<Vec<ServiceInfo>> {
    let output = Command::new("systemctl")
        .arg("list-units")
        .arg("--type=service")
        .arg("--all")
        .arg("--no-pager")
        .output()
        .context("Failed to list services")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut services = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let name = parts[0].trim_end_matches(".service").to_string();
            let status = if parts[2] == "running" {
                ServiceStatus::Running
            } else if parts[2] == "failed" {
                ServiceStatus::Failed
            } else {
                ServiceStatus::Stopped
            };

            services.push(ServiceInfo {
                name,
                status,
                enabled: None,
                pid: None,
                description: None,
            });
        }
    }

    Ok(services)
}

fn list_launchd_services() -> Result<Vec<ServiceInfo>> {
    let output = Command::new("launchctl")
        .arg("list")
        .output()
        .context("Failed to list services")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut services = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            services.push(ServiceInfo {
                name: parts[2].to_string(),
                status: ServiceStatus::Running,
                enabled: None,
                pid: parts[0].parse().ok(),
                description: None,
            });
        }
    }

    Ok(services)
}

fn list_openrc_services() -> Result<Vec<ServiceInfo>> {
    let output = Command::new("rc-status")
        .arg("--servicelist")
        .output()
        .context("Failed to list services")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut services = Vec::new();

    for line in stdout.lines() {
        let name = line.trim().to_string();
        if !name.is_empty() {
            services.push(ServiceInfo {
                name,
                status: ServiceStatus::Unknown,
                enabled: None,
                pid: None,
                description: None,
            });
        }
    }

    Ok(services)
}

fn list_initd_services() -> Result<Vec<ServiceInfo>> {
    let mut services = Vec::new();
    let entries = std::fs::read_dir("/etc/init.d").context("Failed to read /etc/init.d")?;

    for entry in entries {
        if let Ok(entry) = entry {
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip common non-service files
            if name != "README" && name != "." && name != ".." {
                services.push(ServiceInfo {
                    name,
                    status: ServiceStatus::Unknown,
                    enabled: None,
                    pid: None,
                    description: None,
                });
            }
        }
    }

    Ok(services)
}

/// Execute a command with proper output handling
fn execute_command(cmd_parts: &[&str], dry_run: bool, verbose: bool) -> Result<()> {
    if cmd_parts.is_empty() {
        anyhow::bail!("No command to execute");
    }

    let cmd_str = cmd_parts.join(" ");

    if dry_run {
        println!(
            "{} Would execute: {}",
            "[DRY-RUN]".yellow().bold(),
            cmd_str.bright_white()
        );
        return Ok(());
    }

    if verbose {
        println!("{} {}", "Executing:".cyan(), cmd_str.bright_white());
    }

    let mut command = Command::new(cmd_parts[0]);
    for arg in &cmd_parts[1..] {
        command.arg(arg);
    }

    let output = command
        .output()
        .context(format!("Failed to execute: {}", cmd_str))?;

    if verbose || !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        anyhow::bail!("Command failed with exit code: {:?}", output.status.code());
    }

    Ok(())
}
