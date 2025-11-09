use anyhow::{Context, Result};
use colored::*;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PackageManager {
    Apt,    // Debian/Ubuntu
    Dnf,    // Fedora/RHEL 8+
    Yum,    // CentOS/RHEL 7
    Pacman, // Arch Linux
    Zypper, // openSUSE
    Brew,   // macOS
    Apk,    // Alpine Linux
    Unknown,
}

impl PackageManager {
    pub fn name(&self) -> &str {
        match self {
            PackageManager::Apt => "apt",
            PackageManager::Dnf => "dnf",
            PackageManager::Yum => "yum",
            PackageManager::Pacman => "pacman",
            PackageManager::Zypper => "zypper",
            PackageManager::Brew => "brew",
            PackageManager::Apk => "apk",
            PackageManager::Unknown => "unknown",
        }
    }

    pub fn requires_sudo(&self) -> bool {
        match self {
            PackageManager::Brew => false,
            PackageManager::Unknown => false,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub installed: bool,
}

/// Detect the system's package manager
pub fn detect_package_manager() -> Result<PackageManager> {
    debug!("Detecting package manager...");

    // Check for various package managers in order of specificity
    let managers = vec![
        ("brew", PackageManager::Brew),
        ("apt-get", PackageManager::Apt),
        ("dnf", PackageManager::Dnf),
        ("yum", PackageManager::Yum),
        ("pacman", PackageManager::Pacman),
        ("zypper", PackageManager::Zypper),
        ("apk", PackageManager::Apk),
    ];

    for (cmd, pm) in managers {
        if is_command_available(cmd) {
            info!("Detected package manager: {}", pm.name());
            return Ok(pm);
        }
    }

    Ok(PackageManager::Unknown)
}

/// Check if a command is available in PATH
fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Install one or more packages
pub fn install_packages(
    packages: &[String],
    pm: &PackageManager,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    if packages.is_empty() {
        anyhow::bail!("No packages specified");
    }

    let mut cmd_parts = Vec::new();

    // Build command based on package manager
    match pm {
        PackageManager::Apt => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("apt-get");
            cmd_parts.push("install");
            cmd_parts.push("-y");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Dnf | PackageManager::Yum => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push(pm.name());
            cmd_parts.push("install");
            cmd_parts.push("-y");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Pacman => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("pacman");
            cmd_parts.push("-S");
            cmd_parts.push("--noconfirm");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Zypper => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("zypper");
            cmd_parts.push("install");
            cmd_parts.push("-y");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Brew => {
            cmd_parts.push("brew");
            cmd_parts.push("install");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Apk => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("apk");
            cmd_parts.push("add");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Unknown => {
            anyhow::bail!("Unknown package manager - cannot install packages");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Remove one or more packages
pub fn remove_packages(
    packages: &[String],
    pm: &PackageManager,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    if packages.is_empty() {
        anyhow::bail!("No packages specified");
    }

    let mut cmd_parts = Vec::new();

    match pm {
        PackageManager::Apt => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("apt-get");
            cmd_parts.push("remove");
            cmd_parts.push("-y");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Dnf | PackageManager::Yum => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push(pm.name());
            cmd_parts.push("remove");
            cmd_parts.push("-y");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Pacman => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("pacman");
            cmd_parts.push("-R");
            cmd_parts.push("--noconfirm");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Zypper => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("zypper");
            cmd_parts.push("remove");
            cmd_parts.push("-y");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Brew => {
            cmd_parts.push("brew");
            cmd_parts.push("uninstall");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Apk => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("apk");
            cmd_parts.push("del");
            cmd_parts.extend(packages.iter().map(|s| s.as_str()));
        }
        PackageManager::Unknown => {
            anyhow::bail!("Unknown package manager - cannot remove packages");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Update package cache/repositories
pub fn update_cache(pm: &PackageManager, dry_run: bool, verbose: bool) -> Result<()> {
    let mut cmd_parts = Vec::new();

    match pm {
        PackageManager::Apt => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("apt-get");
            cmd_parts.push("update");
        }
        PackageManager::Dnf | PackageManager::Yum => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push(pm.name());
            cmd_parts.push("check-update");
        }
        PackageManager::Pacman => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("pacman");
            cmd_parts.push("-Sy");
        }
        PackageManager::Zypper => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("zypper");
            cmd_parts.push("refresh");
        }
        PackageManager::Brew => {
            cmd_parts.push("brew");
            cmd_parts.push("update");
        }
        PackageManager::Apk => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("apk");
            cmd_parts.push("update");
        }
        PackageManager::Unknown => {
            anyhow::bail!("Unknown package manager - cannot update cache");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Upgrade all packages
pub fn upgrade_packages(pm: &PackageManager, dry_run: bool, verbose: bool) -> Result<()> {
    let mut cmd_parts = Vec::new();

    match pm {
        PackageManager::Apt => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("apt-get");
            cmd_parts.push("upgrade");
            cmd_parts.push("-y");
        }
        PackageManager::Dnf | PackageManager::Yum => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push(pm.name());
            cmd_parts.push("upgrade");
            cmd_parts.push("-y");
        }
        PackageManager::Pacman => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("pacman");
            cmd_parts.push("-Syu");
            cmd_parts.push("--noconfirm");
        }
        PackageManager::Zypper => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("zypper");
            cmd_parts.push("update");
            cmd_parts.push("-y");
        }
        PackageManager::Brew => {
            cmd_parts.push("brew");
            cmd_parts.push("upgrade");
        }
        PackageManager::Apk => {
            if pm.requires_sudo() {
                cmd_parts.push("sudo");
            }
            cmd_parts.push("apk");
            cmd_parts.push("upgrade");
        }
        PackageManager::Unknown => {
            anyhow::bail!("Unknown package manager - cannot upgrade packages");
        }
    }

    execute_command(&cmd_parts, dry_run, verbose)
}

/// Search for packages
pub fn search_packages(query: &str, pm: &PackageManager) -> Result<Vec<PackageInfo>> {
    let output = match pm {
        PackageManager::Apt => {
            let cmd = Command::new("apt-cache")
                .arg("search")
                .arg(query)
                .output()
                .context("Failed to search packages with apt-cache")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Dnf | PackageManager::Yum => {
            let cmd = Command::new(pm.name())
                .arg("search")
                .arg(query)
                .output()
                .context(format!("Failed to search packages with {}", pm.name()))?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Pacman => {
            let cmd = Command::new("pacman")
                .arg("-Ss")
                .arg(query)
                .output()
                .context("Failed to search packages with pacman")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Zypper => {
            let cmd = Command::new("zypper")
                .arg("search")
                .arg(query)
                .output()
                .context("Failed to search packages with zypper")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Brew => {
            let cmd = Command::new("brew")
                .arg("search")
                .arg(query)
                .output()
                .context("Failed to search packages with brew")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Apk => {
            let cmd = Command::new("apk")
                .arg("search")
                .arg(query)
                .output()
                .context("Failed to search packages with apk")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Unknown => {
            anyhow::bail!("Unknown package manager - cannot search packages");
        }
    };

    parse_search_results(&output, pm)
}

/// List installed packages
pub fn list_installed(pm: &PackageManager) -> Result<Vec<PackageInfo>> {
    let output = match pm {
        PackageManager::Apt => {
            let cmd = Command::new("dpkg")
                .arg("-l")
                .output()
                .context("Failed to list packages with dpkg")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Dnf | PackageManager::Yum => {
            let cmd = Command::new(pm.name())
                .arg("list")
                .arg("installed")
                .output()
                .context(format!("Failed to list packages with {}", pm.name()))?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Pacman => {
            let cmd = Command::new("pacman")
                .arg("-Q")
                .output()
                .context("Failed to list packages with pacman")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Zypper => {
            let cmd = Command::new("zypper")
                .arg("search")
                .arg("--installed-only")
                .output()
                .context("Failed to list packages with zypper")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Brew => {
            let cmd = Command::new("brew")
                .arg("list")
                .arg("--versions")
                .output()
                .context("Failed to list packages with brew")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Apk => {
            let cmd = Command::new("apk")
                .arg("info")
                .output()
                .context("Failed to list packages with apk")?;
            String::from_utf8_lossy(&cmd.stdout).to_string()
        }
        PackageManager::Unknown => {
            anyhow::bail!("Unknown package manager - cannot list packages");
        }
    };

    parse_installed_packages(&output, pm)
}

/// Check if a package is installed
pub fn is_package_installed(package: &str, pm: &PackageManager) -> Result<bool> {
    let result = match pm {
        PackageManager::Apt => Command::new("dpkg")
            .arg("-s")
            .arg(package)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        PackageManager::Dnf | PackageManager::Yum => Command::new(pm.name())
            .arg("list")
            .arg("installed")
            .arg(package)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        PackageManager::Pacman => Command::new("pacman")
            .arg("-Q")
            .arg(package)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        PackageManager::Zypper => Command::new("zypper")
            .arg("search")
            .arg("--installed-only")
            .arg(package)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        PackageManager::Brew => Command::new("brew")
            .arg("list")
            .arg(package)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        PackageManager::Apk => Command::new("apk")
            .arg("info")
            .arg("-e")
            .arg(package)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        PackageManager::Unknown => false,
    };

    Ok(result)
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

/// Parse search results into PackageInfo structs
fn parse_search_results(output: &str, pm: &PackageManager) -> Result<Vec<PackageInfo>> {
    let mut packages = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse based on package manager format
        match pm {
            PackageManager::Apt => {
                // Format: "package-name - description"
                if let Some(pos) = line.find(" - ") {
                    let name = line[..pos].trim().to_string();
                    let description = line[pos + 3..].trim().to_string();
                    packages.push(PackageInfo {
                        name,
                        version: None,
                        description: Some(description),
                        installed: false,
                    });
                }
            }
            PackageManager::Brew => {
                // Format: just package names
                packages.push(PackageInfo {
                    name: line.to_string(),
                    version: None,
                    description: None,
                    installed: false,
                });
            }
            PackageManager::Pacman => {
                // Format: "repo/package version"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[0].split('/').last().unwrap_or(parts[0]).to_string();
                    let version = parts[1].to_string();
                    packages.push(PackageInfo {
                        name,
                        version: Some(version),
                        description: None,
                        installed: false,
                    });
                }
            }
            _ => {
                // Generic parsing - just take the first word as package name
                if let Some(name) = line.split_whitespace().next() {
                    packages.push(PackageInfo {
                        name: name.to_string(),
                        version: None,
                        description: None,
                        installed: false,
                    });
                }
            }
        }
    }

    Ok(packages)
}

/// Parse installed packages into PackageInfo structs
fn parse_installed_packages(output: &str, pm: &PackageManager) -> Result<Vec<PackageInfo>> {
    let mut packages = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Listing") || line.starts_with("Desired") {
            continue;
        }

        match pm {
            PackageManager::Apt => {
                // Format: "ii  package-name  version  description"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && parts[0] == "ii" {
                    packages.push(PackageInfo {
                        name: parts[1].to_string(),
                        version: Some(parts[2].to_string()),
                        description: parts.get(3..).map(|d| d.join(" ")),
                        installed: true,
                    });
                }
            }
            PackageManager::Brew => {
                // Format: "package version"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if !parts.is_empty() {
                    packages.push(PackageInfo {
                        name: parts[0].to_string(),
                        version: parts.get(1).map(|v| v.to_string()),
                        description: None,
                        installed: true,
                    });
                }
            }
            PackageManager::Pacman => {
                // Format: "package version"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    packages.push(PackageInfo {
                        name: parts[0].to_string(),
                        version: Some(parts[1].to_string()),
                        description: None,
                        installed: true,
                    });
                }
            }
            _ => {
                // Generic parsing
                let parts: Vec<&str> = line.split_whitespace().collect();
                if !parts.is_empty() {
                    packages.push(PackageInfo {
                        name: parts[0].to_string(),
                        version: parts.get(1).map(|v| v.to_string()),
                        description: None,
                        installed: true,
                    });
                }
            }
        }
    }

    Ok(packages)
}
