use anyhow::{Context, Result};
use colored::*;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

mod alerts;
mod monitor;

use alerts::{AlertManager, AlertStatus, display_alerts, display_alert_detail};

#[derive(Debug, Clone)]
struct FstabEntry {
    device: String,
    mount_point: String,
    fs_type: String,
    options: String,
    dump: String,
    pass: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlockDevice {
    device: String,
    uuid: Option<String>,
    partuuid: Option<String>,
    label: Option<String>,
    fs_type: Option<String>,
    size: Option<String>,
    mount_point: Option<String>,
    is_removable: bool,
    is_ssd: bool,
}

#[derive(Debug, Clone)]
struct MountSuggestion {
    device: BlockDevice,
    suggested_device_id: String,
    suggested_mount_point: String,
    suggested_options: Vec<String>,
    suggested_fs_type: String,
    rationale: Vec<String>,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
        process::exit(1);
    }

    let command = &args[1];

    let result = match command.as_str() {
        "cat" => cat_fstab(),
        "dog" => dog_fstab(),
        "list" | "ls" => list_mounts(),
        "find" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog find <device|mount_point>".red());
                process::exit(1);
            }
            find_entry(&args[2])
        }
        "validate" => validate_fstab(),
        "discover" => discover_devices(),
        "suggest" => {
            let device_filter = if args.len() >= 3 {
                Some(args[2].as_str())
            } else {
                None
            };
            suggest_mounts(device_filter)
        }
        // Alerting commands
        "monitor" => {
            let interval = if args.len() >= 3 {
                args[2].parse::<u64>().unwrap_or(300)
            } else {
                300
            };
            start_monitoring(interval)
        }
        "check" => run_health_check(),
        "alerts" => {
            let status_filter = if args.len() >= 3 {
                match args[2].as_str() {
                    "firing" => Some(AlertStatus::Firing),
                    "acknowledged" => Some(AlertStatus::Acknowledged),
                    "resolved" => Some(AlertStatus::Resolved),
                    "silenced" => Some(AlertStatus::Silenced),
                    _ => None,
                }
            } else {
                None
            };
            list_alerts(status_filter)
        }
        "alert" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog alert <alert_id>".red());
                process::exit(1);
            }
            show_alert(&args[2])
        }
        "ack" | "acknowledge" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog ack <alert_id>".red());
                process::exit(1);
            }
            acknowledge_alert(&args[2])
        }
        "resolve" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog resolve <alert_id>".red());
                process::exit(1);
            }
            resolve_alert(&args[2])
        }
        "silence" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog silence <alert_id>".red());
                process::exit(1);
            }
            silence_alert(&args[2])
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        _ => {
            eprintln!("{} {}", "Unknown command:".red(), command);
            print_help();
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("{} {:#}", "Error:".red().bold(), e);
        process::exit(1);
    }
}

fn cat_fstab() -> Result<()> {
    let fstab_path = "/etc/fstab";
    let contents = fs::read_to_string(fstab_path)
        .with_context(|| format!("Failed to read {}", fstab_path))?;
    print!("{}", contents);
    Ok(())
}

fn dog_fstab() -> Result<()> {
    println!("{} Fetching and parsing /etc/fstab...\n", "🐕".bold());

    let entries = parse_fstab()?;

    if entries.is_empty() {
        println!("{}", "No entries found in /etc/fstab".yellow());
        return Ok(());
    }

    println!("{:<30} {:<20} {:<10} {:<30} {} {}",
             "DEVICE".cyan().bold(),
             "MOUNT POINT".cyan().bold(),
             "TYPE".cyan().bold(),
             "OPTIONS".cyan().bold(),
             "DUMP".cyan().bold(),
             "PASS".cyan().bold());
    println!("{}", "=".repeat(120).bright_black());

    for entry in &entries {
        let device = if entry.device.starts_with("UUID=") {
            entry.device.bright_yellow()
        } else if entry.device.starts_with("/dev/") {
            entry.device.bright_blue()
        } else {
            entry.device.normal()
        };

        let mount_color = match entry.mount_point.as_str() {
            "/" => entry.mount_point.bright_green().bold(),
            "none" | "swap" => entry.mount_point.bright_black(),
            _ => entry.mount_point.white(),
        };

        println!("{:<30} {:<20} {:<10} {:<30} {:<4} {}",
                 device.to_string(),
                 mount_color.to_string(),
                 entry.fs_type,
                 entry.options.truecolor(180, 180, 180).to_string(),
                 entry.dump,
                 entry.pass);
    }

    println!("\n{} Good dog! Retrieved {} entries",
             "🐕".bold(),
             entries.len().to_string().green().bold());
    Ok(())
}

fn parse_fstab() -> Result<Vec<FstabEntry>> {
    let fstab_path = "/etc/fstab";
    parse_fstab_from_path(fstab_path)
}

fn parse_fstab_from_path(path: &str) -> Result<Vec<FstabEntry>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path))?;

    let mut entries = Vec::new();

    for (line_num, line) in contents.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.len() < 6 {
            eprintln!("{} Line {}: Expected 6 fields, found {} - skipping",
                     "Warning:".yellow(),
                     line_num + 1,
                     parts.len());
            continue;
        }

        entries.push(FstabEntry {
            device: parts[0].to_string(),
            mount_point: parts[1].to_string(),
            fs_type: parts[2].to_string(),
            options: parts[3].to_string(),
            dump: parts[4].to_string(),
            pass: parts[5].to_string(),
        });
    }

    Ok(entries)
}

fn list_mounts() -> Result<()> {
    let entries = parse_fstab()?;

    if entries.is_empty() {
        println!("{}", "No mount points found".yellow());
        return Ok(());
    }

    println!("{}\n", "Mount points in /etc/fstab:".cyan().bold());
    for entry in entries {
        println!("  {} {} {} {}",
                 entry.device.bright_blue(),
                 "->".bright_black(),
                 entry.mount_point.bright_green(),
                 format!("({})", entry.fs_type).truecolor(180, 180, 180));
    }
    Ok(())
}

fn find_entry(search: &str) -> Result<()> {
    let entries = parse_fstab()?;
    let mut found = Vec::new();

    for entry in entries {
        if entry.device.contains(search) || entry.mount_point.contains(search) {
            found.push(entry);
        }
    }

    if found.is_empty() {
        println!("{} '{}'",
                 "No entries found matching".yellow(),
                 search.bright_white());
        return Ok(());
    }

    println!("{} {} matching entries:\n",
             "Found".green().bold(),
             found.len().to_string().bright_white().bold());
    println!("{:<30} {:<20} {:<10} {:<30} {} {}",
             "DEVICE".cyan().bold(),
             "MOUNT POINT".cyan().bold(),
             "TYPE".cyan().bold(),
             "OPTIONS".cyan().bold(),
             "DUMP".cyan().bold(),
             "PASS".cyan().bold());
    println!("{}", "=".repeat(120).bright_black());

    for entry in found {
        println!("{:<30} {:<20} {:<10} {:<30} {:<4} {}",
                 entry.device, entry.mount_point, entry.fs_type,
                 entry.options, entry.dump, entry.pass);
    }
    Ok(())
}

fn validate_fstab() -> Result<()> {
    println!("{} Validating /etc/fstab...\n", "🔍".bold());

    let entries = parse_fstab()?;
    let mut issues = 0;
    let mut warnings = 0;

    // Check if fstab is empty
    if entries.is_empty() {
        println!("{}", "⚠️  /etc/fstab is empty or contains no valid entries".yellow());
        return Ok(());
    }

    // Check for duplicate mount points
    let mut mount_points = std::collections::HashSet::new();
    for (i, entry) in entries.iter().enumerate() {
        if entry.mount_point != "none" && entry.mount_point != "swap" {
            if !mount_points.insert(&entry.mount_point) {
                println!("{} Entry {}: Duplicate mount point '{}'",
                         "⚠️ ".yellow(),
                         i + 1,
                         entry.mount_point.bright_white());
                issues += 1;
            }
        }
    }

    // Check each entry for common issues
    for (i, entry) in entries.iter().enumerate() {
        // Check root filesystem pass value
        if entry.mount_point == "/" && entry.pass != "1" {
            println!("{} Entry {}: Root filesystem should have pass=1, found pass={}",
                     "⚠️ ".yellow(),
                     i + 1,
                     entry.pass.bright_white());
            issues += 1;
        }

        // Check mount point format
        if entry.mount_point != "none" && entry.mount_point != "swap" {
            if !entry.mount_point.starts_with('/') {
                println!("{} Entry {}: Mount point '{}' doesn't start with /",
                         "❌".red(),
                         i + 1,
                         entry.mount_point.bright_white());
                issues += 1;
            }
        }

        // Check swap partition configuration
        if entry.fs_type == "swap" && entry.mount_point != "none" && entry.mount_point != "swap" {
            println!("{} Entry {}: Swap partition should have mount point 'none' or 'swap'",
                     "⚠️ ".yellow(),
                     i + 1);
            issues += 1;
        }

        // Check for potentially dangerous options
        if entry.options.contains("noauto") && entry.mount_point == "/" {
            println!("{} Entry {}: Root filesystem with 'noauto' option will not mount at boot!",
                     "❌".red(),
                     i + 1);
            issues += 1;
        }

        // Check pass value validity
        if let Err(_) = entry.pass.parse::<u32>() {
            println!("{} Entry {}: Invalid pass value '{}' (should be 0, 1, or 2)",
                     "❌".red(),
                     i + 1,
                     entry.pass.bright_white());
            issues += 1;
        }

        // Check dump value validity
        if let Err(_) = entry.dump.parse::<u32>() {
            println!("{} Entry {}: Invalid dump value '{}' (should be 0 or 1)",
                     "⚠️ ".yellow(),
                     i + 1,
                     entry.dump.bright_white());
            warnings += 1;
        }

        // Warn about missing mount points
        if entry.mount_point != "none" && entry.mount_point != "swap" {
            if !Path::new(&entry.mount_point).exists() {
                println!("{} Entry {}: Mount point directory '{}' does not exist",
                         "ℹ️ ".blue(),
                         i + 1,
                         entry.mount_point.bright_white());
                warnings += 1;
            }
        }
    }

    // Summary
    println!();
    if issues == 0 && warnings == 0 {
        println!("{} No issues found! /etc/fstab looks good.", "✅".green());
    } else {
        if issues > 0 {
            println!("{} Found {} critical issue(s)",
                     "❌".red(),
                     issues.to_string().red().bold());
        }
        if warnings > 0 {
            println!("{} Found {} warning(s)",
                     "⚠️ ".yellow(),
                     warnings.to_string().yellow().bold());
        }
    }
    Ok(())
}

fn discover_block_devices() -> Result<Vec<BlockDevice>> {
    let os = env::consts::OS;

    match os {
        "macos" => discover_macos_devices(),
        "linux" => discover_linux_devices(),
        _ => {
            eprintln!("{} Device discovery not supported on {}",
                     "Warning:".yellow(), os);
            Ok(Vec::new())
        }
    }
}

fn discover_macos_devices() -> Result<Vec<BlockDevice>> {
    let output = Command::new("diskutil")
        .arg("list")
        .arg("-plist")
        .output()
        .context("Failed to run diskutil list")?;

    if !output.status.success() {
        anyhow::bail!("diskutil command failed");
    }

    // Parse the output and get disk info
    let list_output = Command::new("diskutil")
        .arg("list")
        .output()
        .context("Failed to run diskutil list")?;

    let list_str = String::from_utf8_lossy(&list_output.stdout);
    let mut devices = Vec::new();

    // Parse disk identifiers from the output
    for line in list_str.lines() {
        if line.contains("disk") && !line.starts_with("/") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(disk_id) = parts.last() {
                if disk_id.starts_with("disk") {
                    if let Ok(device) = get_macos_device_info(disk_id) {
                        // Only add devices with a filesystem
                        if device.fs_type.is_some() {
                            devices.push(device);
                        }
                    }
                }
            }
        }
    }

    Ok(devices)
}

fn get_macos_device_info(disk_id: &str) -> Result<BlockDevice> {
    let output = Command::new("diskutil")
        .arg("info")
        .arg(disk_id)
        .output()
        .context("Failed to run diskutil info")?;

    let info_str = String::from_utf8_lossy(&output.stdout);
    let mut uuid = None;
    let mut label = None;
    let mut fs_type = None;
    let mut size = None;
    let mut mount_point = None;
    let mut is_removable = false;
    let is_ssd = false; // Would need additional detection

    for line in info_str.lines() {
        let line = line.trim();
        if line.starts_with("Volume UUID:") {
            uuid = line.split(':').nth(1).map(|s| s.trim().to_string());
        } else if line.starts_with("Volume Name:") {
            let vol_name = line.split(':').nth(1).map(|s| s.trim().to_string());
            // Filter out "Not applicable"
            if let Some(ref name) = vol_name {
                if !name.starts_with("Not applicable") && !name.is_empty() {
                    label = vol_name;
                }
            }
        } else if line.starts_with("Type (Bundle):") || line.starts_with("File System Personality:") {
            let fs = line.split(':').nth(1).map(|s| s.trim().to_string());
            if let Some(ref f) = fs {
                if !f.is_empty() && fs_type.is_none() {
                    fs_type = fs;
                }
            }
        } else if line.starts_with("Disk Size:") || line.starts_with("Total Size:") {
            size = line.split(':').nth(1).map(|s| s.trim().to_string());
        } else if line.starts_with("Mount Point:") {
            let mp = line.split(':').nth(1).map(|s| s.trim().to_string());
            if let Some(ref m) = mp {
                if !m.starts_with("Not applicable") && !m.is_empty() {
                    mount_point = mp;
                }
            }
        } else if line.starts_with("Removable Media:") {
            is_removable = line.contains("Removable");
        }
    }

    Ok(BlockDevice {
        device: format!("/dev/{}", disk_id),
        uuid,
        partuuid: None,
        label,
        fs_type,
        size,
        mount_point,
        is_removable,
        is_ssd,
    })
}

fn discover_linux_devices() -> Result<Vec<BlockDevice>> {
    // Use lsblk to get block device information
    let output = Command::new("lsblk")
        .args(&["-J", "-o", "NAME,UUID,PARTUUID,LABEL,FSTYPE,SIZE,MOUNTPOINT,RM,ROTA"])
        .output()
        .context("Failed to run lsblk. Make sure lsblk is installed.")?;

    if !output.status.success() {
        anyhow::bail!("lsblk command failed");
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&json_str)
        .context("Failed to parse lsblk JSON output")?;

    let mut devices = Vec::new();

    if let Some(blockdevices) = parsed["blockdevices"].as_array() {
        for device in blockdevices {
            parse_linux_device(device, &mut devices);
        }
    }

    Ok(devices)
}

fn parse_linux_device(device: &serde_json::Value, devices: &mut Vec<BlockDevice>) {
    let name = device["name"].as_str().unwrap_or("");
    let device_path = if name.starts_with("/dev/") {
        name.to_string()
    } else {
        format!("/dev/{}", name)
    };

    let block_device = BlockDevice {
        device: device_path,
        uuid: device["uuid"].as_str().map(String::from),
        partuuid: device["partuuid"].as_str().map(String::from),
        label: device["label"].as_str().map(String::from),
        fs_type: device["fstype"].as_str().map(String::from),
        size: device["size"].as_str().map(String::from),
        mount_point: device["mountpoint"].as_str().map(String::from),
        is_removable: device["rm"].as_str() == Some("1"),
        is_ssd: device["rota"].as_str() == Some("0"), // Non-rotating = SSD
    };

    // Only add if it has a filesystem
    if block_device.fs_type.is_some() {
        devices.push(block_device);
    }

    // Recursively parse children (partitions)
    if let Some(children) = device["children"].as_array() {
        for child in children {
            parse_linux_device(child, devices);
        }
    }
}

fn discover_devices() -> Result<()> {
    println!("{} Discovering block devices...\n", "🔍".bold());

    let devices = discover_block_devices()?;

    if devices.is_empty() {
        println!("{}", "No block devices found".yellow());
        return Ok(());
    }

    println!("{:<20} {:<38} {:<20} {:<10} {:<10} {:<20}",
             "DEVICE".cyan().bold(),
             "UUID".cyan().bold(),
             "LABEL".cyan().bold(),
             "TYPE".cyan().bold(),
             "SIZE".cyan().bold(),
             "MOUNT POINT".cyan().bold());
    println!("{}", "=".repeat(140).bright_black());

    for device in &devices {
        let uuid_display = device.uuid.as_deref().unwrap_or("-");
        let label_display = device.label.as_deref().unwrap_or("-");
        let fs_display = device.fs_type.as_deref().unwrap_or("-");
        let size_display = device.size.as_deref().unwrap_or("-");
        let mount_display = device.mount_point.as_deref().unwrap_or("-");

        let device_color = if device.is_removable {
            device.device.bright_magenta()
        } else if device.is_ssd {
            device.device.bright_cyan()
        } else {
            device.device.bright_blue()
        };

        let mut tags = Vec::new();
        if device.is_ssd {
            tags.push("SSD".green());
        }
        if device.is_removable {
            tags.push("REMOVABLE".magenta());
        }

        print!("{:<20} {:<38} {:<20} {:<10} {:<10} {:<20}",
               device_color.to_string(),
               uuid_display.truecolor(150, 150, 150).to_string(),
               label_display.bright_white().to_string(),
               fs_display.yellow().to_string(),
               size_display,
               mount_display.green().to_string());

        if !tags.is_empty() {
            print!(" [");
            for (i, tag) in tags.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{}", tag);
            }
            print!("]");
        }
        println!();
    }

    println!("\n{} Found {} block device(s)",
             "✓".green().bold(),
             devices.len().to_string().bright_white().bold());
    Ok(())
}

fn suggest_mount_options(device: &BlockDevice) -> MountSuggestion {
    let fs_type = device.fs_type.as_deref().unwrap_or("unknown");
    let mut options = Vec::new();
    let mut rationale = Vec::new();

    // Base options
    options.push("defaults".to_string());

    // SSD optimizations
    if device.is_ssd {
        match fs_type {
            "ext4" => {
                options.push("noatime".to_string());
                options.push("discard".to_string());
                rationale.push("noatime: Reduces SSD wear by not updating access times".to_string());
                rationale.push("discard: Enables TRIM support for SSDs".to_string());
            }
            "btrfs" => {
                options.push("noatime".to_string());
                options.push("ssd".to_string());
                options.push("discard=async".to_string());
                rationale.push("SSD-optimized mount options for btrfs".to_string());
                rationale.push("async discard improves performance".to_string());
            }
            "xfs" => {
                options.push("noatime".to_string());
                options.push("discard".to_string());
                rationale.push("XFS with SSD optimizations".to_string());
            }
            _ => {}
        }
    } else {
        // HDD optimizations
        options.push("relatime".to_string());
        rationale.push("relatime: Balances access time updates for HDDs".to_string());
    }

    // Filesystem-specific options
    match fs_type {
        "ntfs" | "ntfs3" => {
            options.clear();
            options.push("defaults".to_string());
            options.push("uid=1000".to_string());
            options.push("gid=1000".to_string());
            options.push("umask=0022".to_string());
            rationale.push("NTFS with user permissions set".to_string());
        }
        "vfat" | "exfat" => {
            options.clear();
            options.push("defaults".to_string());
            options.push("uid=1000".to_string());
            options.push("gid=1000".to_string());
            options.push("umask=0022".to_string());
            options.push("utf8".to_string());
            rationale.push("FAT filesystem with UTF-8 and user permissions".to_string());
        }
        _ => {}
    }

    // Removable device options
    if device.is_removable {
        options.push("nofail".to_string());
        rationale.push("nofail: System can boot even if device is not present".to_string());
    }

    // Determine device identifier preference
    let suggested_device_id = if let Some(uuid) = &device.uuid {
        format!("UUID={}", uuid)
    } else if let Some(label) = &device.label {
        format!("LABEL={}", label)
    } else {
        device.device.clone()
    };

    if device.uuid.is_some() {
        rationale.push("Using UUID for stable device identification".to_string());
    }

    // Suggest mount point
    let suggested_mount_point = if let Some(label) = &device.label {
        format!("/mnt/{}", label.to_lowercase().replace(" ", "_"))
    } else if let Some(uuid) = &device.uuid {
        format!("/mnt/disk_{}", &uuid[..8])
    } else {
        let device_name = device.device.trim_start_matches("/dev/");
        format!("/mnt/{}", device_name)
    };

    MountSuggestion {
        device: device.clone(),
        suggested_device_id,
        suggested_mount_point,
        suggested_options: options,
        suggested_fs_type: fs_type.to_string(),
        rationale,
    }
}

fn suggest_mounts(device_filter: Option<&str>) -> Result<()> {
    println!("{} Generating mount suggestions...\n", "💡".bold());

    let devices = discover_block_devices()?;

    // Filter out already mounted devices and apply user filter
    let unmounted: Vec<_> = devices.into_iter()
        .filter(|d| {
            let not_system_mounted = d.mount_point.is_none() ||
                matches!(d.mount_point.as_deref(), Some("/") | Some("/boot") | Some("/home"));

            let matches_filter = if let Some(filter) = device_filter {
                d.device.contains(filter) ||
                d.label.as_ref().map_or(false, |l| l.contains(filter)) ||
                d.uuid.as_ref().map_or(false, |u| u.contains(filter))
            } else {
                true
            };

            not_system_mounted && matches_filter && d.fs_type.is_some()
        })
        .collect();

    if unmounted.is_empty() {
        println!("{}", "No devices available for mounting suggestions".yellow());
        return Ok(());
    }

    for device in unmounted {
        let suggestion = suggest_mount_options(&device);

        println!("{}", "─".repeat(100).bright_black());
        println!("{} {}", "Device:".cyan().bold(), device.device.bright_white());

        if let Some(uuid) = &device.uuid {
            println!("  {} {}", "UUID:".truecolor(150, 150, 150), uuid.truecolor(150, 150, 150));
        }
        if let Some(label) = &device.label {
            println!("  {} {}", "Label:".cyan(), label.bright_white());
        }
        println!("  {} {}", "Type:".cyan(), suggestion.suggested_fs_type.yellow());
        if let Some(size) = &device.size {
            println!("  {} {}", "Size:".cyan(), size);
        }

        println!("\n{}", "Suggested fstab entry:".green().bold());
        println!("  {} {} {} {} {} {}",
                 suggestion.suggested_device_id.bright_yellow(),
                 suggestion.suggested_mount_point.bright_green(),
                 suggestion.suggested_fs_type.yellow(),
                 suggestion.suggested_options.join(",").truecolor(180, 180, 180),
                 "0".truecolor(150, 150, 150),
                 "2".truecolor(150, 150, 150));

        if !suggestion.rationale.is_empty() {
            println!("\n{}", "Rationale:".blue().bold());
            for reason in &suggestion.rationale {
                println!("  {} {}", "•".blue(), reason.truecolor(200, 200, 200));
            }
        }

        println!();
    }

    println!("{}", "=".repeat(100).bright_black());
    println!("{} Remember to create the mount point directory before mounting:", "Note:".yellow().bold());
    println!("  {}", "sudo mkdir -p <mount_point>".bright_white());
    println!("  {}", "sudo mount -a  # Test the configuration".bright_white());

    Ok(())
}

fn get_storage_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".catdog").join("alerts.json")
}

fn start_monitoring(interval: u64) -> Result<()> {
    let storage_path = get_storage_path();
    monitor::start_monitoring(&storage_path, interval)
}

fn run_health_check() -> Result<()> {
    let storage_path = get_storage_path();
    monitor::check_once(&storage_path)
}

fn list_alerts(status_filter: Option<AlertStatus>) -> Result<()> {
    let storage_path = get_storage_path();
    let manager = AlertManager::new(storage_path)?;

    let alerts = manager.get_alerts(status_filter);
    display_alerts(&alerts);

    Ok(())
}

fn show_alert(alert_id: &str) -> Result<()> {
    let storage_path = get_storage_path();
    let manager = AlertManager::new(storage_path)?;

    match manager.get_alert(alert_id) {
        Some(alert) => {
            display_alert_detail(alert);
            Ok(())
        }
        None => {
            eprintln!("{} Alert not found: {}", "Error:".red(), alert_id);
            process::exit(1);
        }
    }
}

fn acknowledge_alert(alert_id: &str) -> Result<()> {
    let storage_path = get_storage_path();
    let mut manager = AlertManager::new(storage_path)?;

    manager.acknowledge_alert(alert_id)?;
    println!("{} Alert {} acknowledged", "✓".green().bold(), alert_id);

    Ok(())
}

fn resolve_alert(alert_id: &str) -> Result<()> {
    let storage_path = get_storage_path();
    let mut manager = AlertManager::new(storage_path)?;

    manager.resolve_alert(alert_id)?;
    println!("{} Alert {} resolved", "✓".green().bold(), alert_id);

    Ok(())
}

fn silence_alert(alert_id: &str) -> Result<()> {
    let storage_path = get_storage_path();
    let mut manager = AlertManager::new(storage_path)?;

    manager.silence_alert(alert_id)?;
    println!("{} Alert {} silenced", "✓".green().bold(), alert_id);

    Ok(())
}

fn print_help() {
    println!("{} {} An fstab utility with PagerDuty-like alerting!",
        "catdog".bright_green().bold(),
        "-".bright_black());
    println!("\n{}", "USAGE:".cyan().bold());
    println!("    catdog <COMMAND>\n");

    println!("{} {}", "FILESYSTEM".cyan().bold(), "COMMANDS:".cyan().bold());
    println!("    {}          Display raw /etc/fstab file (like cat)", "cat".bright_yellow());
    println!("    {}          Fetch and parse /etc/fstab in a nice table (like a dog fetching!)", "dog".bright_yellow());
    println!("    {}     List all mount points", "list, ls".bright_yellow());
    println!("    {}  Find entries matching device or mount point", "find <term>".bright_yellow());
    println!("    {}     Check /etc/fstab for common issues", "validate".bright_yellow());
    println!("    {}    Discover available block devices", "discover".bright_yellow());
    println!("    {}       Generate smart mount suggestions for devices", "suggest [device]".bright_yellow());

    println!("\n{} {}", "ALERTING".cyan().bold(), "COMMANDS:".cyan().bold());
    println!("    {}       Run filesystem health checks once", "check".bright_yellow());
    println!("    {}       Start continuous monitoring (default: 300s interval)", "monitor [interval]".bright_yellow());
    println!("    {}        List all alerts (optionally filter: firing/acknowledged/resolved/silenced)", "alerts [status]".bright_yellow());
    println!("    {}       Show detailed information about an alert", "alert <id>".bright_yellow());
    println!("    {}         Acknowledge an alert", "ack <id>".bright_yellow());
    println!("    {}        Resolve an alert", "resolve <id>".bright_yellow());
    println!("    {}       Silence an alert", "silence <id>".bright_yellow());

    println!("\n    {}         Show this help message", "help".bright_yellow());

    println!("\n{}", "EXAMPLES:".cyan().bold());
    println!("    catdog cat                 {} Show raw fstab file", "#".bright_black());
    println!("    catdog dog                 {} Parse and display fstab nicely", "#".bright_black());
    println!("    catdog find /dev           {} Find all entries with /dev", "#".bright_black());
    println!("    catdog validate            {} Check for common issues", "#".bright_black());
    println!("    catdog discover            {} List all block devices with details", "#".bright_black());
    println!("    catdog suggest             {} Generate fstab entries with smart defaults", "#".bright_black());
    println!("    catdog check               {} Run health checks once", "#".bright_black());
    println!("    catdog monitor 60          {} Start monitoring with 60s interval", "#".bright_black());
    println!("    catdog alerts              {} List all alerts", "#".bright_black());
    println!("    catdog alerts firing       {} List only firing alerts", "#".bright_black());
    println!("    catdog alert <id>          {} Show alert details", "#".bright_black());
    println!("    catdog ack <id>            {} Acknowledge an alert", "#".bright_black());
    println!("    catdog resolve <id>        {} Resolve an alert", "#".bright_black());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_fstab(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_parse_valid_fstab() {
        let content = r#"
# Comment line
UUID=abc-123 / ext4 defaults 0 1
/dev/sda2 /home ext4 defaults 0 2
tmpfs /tmp tmpfs defaults 0 0
"#;
        let file = create_test_fstab(content);
        let entries = parse_fstab_from_path(file.path().to_str().unwrap()).unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].device, "UUID=abc-123");
        assert_eq!(entries[0].mount_point, "/");
        assert_eq!(entries[1].device, "/dev/sda2");
        assert_eq!(entries[2].fs_type, "tmpfs");
    }

    #[test]
    fn test_parse_empty_fstab() {
        let content = r#"
# Only comments

"#;
        let file = create_test_fstab(content);
        let entries = parse_fstab_from_path(file.path().to_str().unwrap()).unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_parse_fstab_with_invalid_lines() {
        let content = r#"
UUID=abc-123 / ext4 defaults 0 1
invalid only four fields
/dev/sda2 /home ext4 defaults 0 2
"#;
        let file = create_test_fstab(content);
        let entries = parse_fstab_from_path(file.path().to_str().unwrap()).unwrap();

        // Should skip invalid line
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_fstab_entry_fields() {
        let content = "UUID=test /mnt/data btrfs rw,noatime 0 2\n";
        let file = create_test_fstab(content);
        let entries = parse_fstab_from_path(file.path().to_str().unwrap()).unwrap();

        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.device, "UUID=test");
        assert_eq!(entry.mount_point, "/mnt/data");
        assert_eq!(entry.fs_type, "btrfs");
        assert_eq!(entry.options, "rw,noatime");
        assert_eq!(entry.dump, "0");
        assert_eq!(entry.pass, "2");
    }
}
