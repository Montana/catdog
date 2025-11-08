use anyhow::{Context, Result};
use colored::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::alerts::{Alert, AlertManager, AlertSeverity};

pub struct FsMonitor {
    alert_manager: AlertManager,
}

impl FsMonitor {
    pub fn new(alert_manager: AlertManager) -> Self {
        FsMonitor { alert_manager }
    }

    pub fn run_checks(&mut self) -> Result<()> {
        println!("{} Running filesystem checks...", "🔍".bold());

        self.check_disk_usage()?;
        self.check_fstab_validity()?;
        self.check_mount_failures()?;

        println!("{} Checks complete", "✓".green().bold());
        Ok(())
    }

    pub fn monitor_loop(&mut self, interval_seconds: u64) -> Result<()> {
        println!("{} Starting filesystem monitoring (interval: {}s)",
            "🚀".bold(), interval_seconds);
        println!("Press Ctrl+C to stop\n");

        loop {
            if let Err(e) = self.run_checks() {
                eprintln!("{} Check failed: {}", "Error:".red(), e);
            }

            thread::sleep(Duration::from_secs(interval_seconds));
        }
    }

    fn check_disk_usage(&mut self) -> Result<()> {
        let mounts = self.get_mounted_filesystems()?;

        for (mount_point, usage) in mounts {
            if usage >= 90 {
                let mut alert = Alert::new(
                    format!("Critical disk usage on {}", mount_point),
                    format!("Disk usage is at {}% on {}", usage, mount_point),
                    AlertSeverity::Critical,
                    "disk_usage_monitor".to_string(),
                );
                alert.add_metadata("mount_point".to_string(), mount_point.clone());
                alert.add_metadata("usage_percent".to_string(), usage.to_string());

                self.alert_manager.create_alert(alert)?;
            } else if usage >= 80 {
                let mut alert = Alert::new(
                    format!("High disk usage on {}", mount_point),
                    format!("Disk usage is at {}% on {}", usage, mount_point),
                    AlertSeverity::Warning,
                    "disk_usage_monitor".to_string(),
                );
                alert.add_metadata("mount_point".to_string(), mount_point.clone());
                alert.add_metadata("usage_percent".to_string(), usage.to_string());

                self.alert_manager.create_alert(alert)?;
            }
        }

        Ok(())
    }

    fn get_mounted_filesystems(&self) -> Result<HashMap<String, u8>> {
        let os = std::env::consts::OS;
        match os {
            "macos" => self.get_macos_disk_usage(),
            "linux" => self.get_linux_disk_usage(),
            _ => Ok(HashMap::new()),
        }
    }

    fn get_macos_disk_usage(&self) -> Result<HashMap<String, u8>> {
        let output = Command::new("df")
            .args(&["-H"])
            .output()
            .context("Failed to run df command")?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut usage_map = HashMap::new();

        for line in output_str.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                let mount_point = parts[5];
                let capacity_str = parts[4].trim_end_matches('%');

                if let Ok(capacity) = capacity_str.parse::<u8>() {
                    usage_map.insert(mount_point.to_string(), capacity);
                }
            }
        }

        Ok(usage_map)
    }

    fn get_linux_disk_usage(&self) -> Result<HashMap<String, u8>> {
        let output = Command::new("df")
            .args(&["-h"])
            .output()
            .context("Failed to run df command")?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut usage_map = HashMap::new();

        for line in output_str.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                let mount_point = parts[5];
                let capacity_str = parts[4].trim_end_matches('%');

                if let Ok(capacity) = capacity_str.parse::<u8>() {
                    usage_map.insert(mount_point.to_string(), capacity);
                }
            }
        }

        Ok(usage_map)
    }

    fn check_fstab_validity(&mut self) -> Result<()> {
        let fstab_path = "/etc/fstab";

        if !Path::new(fstab_path).exists() {
            let alert = Alert::new(
                "fstab file not found".to_string(),
                format!("{} does not exist", fstab_path),
                AlertSeverity::Warning,
                "fstab_monitor".to_string(),
            );
            self.alert_manager.create_alert(alert)?;
            return Ok(());
        }

        let contents = match fs::read_to_string(fstab_path) {
            Ok(c) => c,
            Err(e) => {
                let mut alert = Alert::new(
                    "Cannot read fstab file".to_string(),
                    format!("Failed to read {}: {}", fstab_path, e),
                    AlertSeverity::Critical,
                    "fstab_monitor".to_string(),
                );
                alert.add_metadata("error".to_string(), e.to_string());
                self.alert_manager.create_alert(alert)?;
                return Ok(());
            }
        };

        let mut line_num = 0;
        for line in contents.lines() {
            line_num += 1;
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 6 {
                let mut alert = Alert::new(
                    format!("Malformed fstab entry at line {}", line_num),
                    format!("Line {} has {} fields, expected 6", line_num, parts.len()),
                    AlertSeverity::Warning,
                    "fstab_monitor".to_string(),
                );
                alert.add_metadata("line_number".to_string(), line_num.to_string());
                alert.add_metadata("line_content".to_string(), trimmed.to_string());
                self.alert_manager.create_alert(alert)?;
            }
        }

        Ok(())
    }

    fn check_mount_failures(&mut self) -> Result<()> {
        // Check if any mounts from fstab have failed
        let fstab_path = "/etc/fstab";

        if !Path::new(fstab_path).exists() {
            return Ok(());
        }

        let contents = fs::read_to_string(fstab_path)?;
        let mut expected_mounts = Vec::new();

        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 6 {
                let mount_point = parts[1];
                if mount_point != "none" && mount_point != "swap" {
                    expected_mounts.push((parts[0].to_string(), mount_point.to_string()));
                }
            }
        }

        // Check which mount points don't exist or aren't mounted
        for (device, mount_point) in expected_mounts {
            let path = Path::new(&mount_point);

            if !path.exists() {
                let mut alert = Alert::new(
                    format!("Mount point {} does not exist", mount_point),
                    format!("The mount point directory {} for device {} does not exist",
                        mount_point, device),
                    AlertSeverity::Warning,
                    "mount_monitor".to_string(),
                );
                alert.add_metadata("device".to_string(), device);
                alert.add_metadata("mount_point".to_string(), mount_point.clone());
                self.alert_manager.create_alert(alert)?;
            }
        }

        Ok(())
    }
}

pub fn check_once(storage_path: &Path) -> Result<()> {
    let alert_manager = AlertManager::new(storage_path.to_path_buf())?;
    let mut monitor = FsMonitor::new(alert_manager);
    monitor.run_checks()
}

pub fn start_monitoring(storage_path: &Path, interval_seconds: u64) -> Result<()> {
    let alert_manager = AlertManager::new(storage_path.to_path_buf())?;
    let mut monitor = FsMonitor::new(alert_manager);
    monitor.monitor_loop(interval_seconds)
}