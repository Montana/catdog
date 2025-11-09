use anyhow::{Context, Result};
use colored::*;
use log::info;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

mod alerts;
mod backup;
mod config;
mod corpus;
mod diff;
mod error;
mod monitor;
mod package;
mod service;
mod sysinfo;

use alerts::{display_alert_detail, display_alerts, AlertManager, AlertStatus};
use config::Config;
use error::{to_user_error, UserError};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

#[derive(Debug, Clone)]
struct CliConfig {
    json_output: bool,
    no_color: bool,
    verbose: bool,
    dry_run: bool,
    app_config: Config,
}

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
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // Run main logic and handle errors nicely
    if let Err(e) = run() {
        let user_error = to_user_error(e);
        user_error.display();
        process::exit(user_error.exit_code());
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    // Handle version early
    if args.len() >= 2 && (args[1] == "--version" || args[1] == "-V" || args[1] == "version") {
        print_version();
        return Ok(());
    }

    if args.len() < 2 {
        print_help();
        process::exit(1);
    }

    // Load application config
    let app_config = Config::load().context("Failed to load configuration")?;

    // Parse global flags
    let config = CliConfig {
        json_output: args.contains(&"--json".to_string()),
        no_color: args.contains(&"--no-color".to_string()) || env::var("NO_COLOR").is_ok(),
        verbose: args.contains(&"-v".to_string()) || args.contains(&"--verbose".to_string()),
        dry_run: args.contains(&"--dry-run".to_string()),
        app_config,
    };

    // Disable colors if requested
    if config.no_color {
        colored::control::set_override(false);
    }

    // Show dry-run notice
    if config.dry_run {
        println!(
            "{} Dry-run mode enabled - no changes will be made\n",
            "ℹ️".blue()
        );
    }

    // Filter out flags to get the actual command and args
    let non_flag_args: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--") && !a.starts_with("-v") && !a.starts_with("-V"))
        .map(|s| s.clone())
        .collect();

    if non_flag_args.len() < 2 {
        print_help();
        process::exit(1);
    }

    let command = &non_flag_args[1];

    info!("Executing command: {}", command);

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
        "discover" => discover_devices(&config),
        "backup" => {
            if non_flag_args.len() < 3 {
                backup_file_cmd("/etc/fstab", config.dry_run)
            } else {
                backup_file_cmd(&non_flag_args[2], config.dry_run)
            }
        }
        "restore" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog restore <backup_path> [--force]".red());
                process::exit(1);
            }
            let force = args.contains(&"--force".to_string());
            restore_backup_cmd(&args[2], config.dry_run, force)
        }
        "list-backups" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog list-backups <file>".red());
                process::exit(1);
            }
            list_backups_cmd(&args[2])
        }
        "backup-stats" => backup_stats_cmd(),
        "backup-health" => backup_health_cmd(),
        "backup-drill" => backup_drill_cmd(),
        "suggest" => {
            let device_filter = if args.len() >= 3 {
                Some(args[2].as_str())
            } else {
                None
            };
            suggest_mounts(device_filter)
        }
        "generate" | "generate-fstab" => {
            let output_file = if args.len() >= 3 {
                Some(args[2].as_str())
            } else {
                None
            };
            generate_fstab(output_file, config.dry_run)
        }
        // Bark (alert) commands
        "monitor" => {
            let interval = if args.len() >= 3 {
                args[2].parse::<u64>().unwrap_or(300)
            } else {
                300
            };
            start_monitoring(interval)
        }
        "check" => run_health_check(),
        "barks" | "alerts" => {
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
        "bark" | "alert" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog bark <bark_id>".red());
                process::exit(1);
            }
            show_alert(&args[2])
        }
        "ack" | "acknowledge" | "pet" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog ack <bark_id>".red());
                process::exit(1);
            }
            acknowledge_alert(&args[2])
        }
        "resolve" | "quiet" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog resolve <bark_id>".red());
                process::exit(1);
            }
            resolve_alert(&args[2])
        }
        "silence" | "hush" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog silence <bark_id>".red());
                process::exit(1);
            }
            silence_alert(&args[2])
        }
        // Corpus commands
        "corpus" => {
            if args.len() < 3 {
                eprintln!("{}", "Usage: catdog corpus <ingest|search|stats>".red());
                process::exit(1);
            }
            match args[2].as_str() {
                "ingest" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog corpus ingest <file>".red());
                        process::exit(1);
                    }
                    corpus_ingest(&args[3])
                }
                "search" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog corpus search <query>".red());
                        process::exit(1);
                    }
                    let query = args[3..].join(" ");
                    corpus_search(&query)
                }
                "stats" => corpus_stats(),
                _ => {
                    eprintln!(
                        "{}",
                        "Unknown corpus command. Try: ingest, search, stats".red()
                    );
                    process::exit(1);
                }
            }
        }
        // Service management commands
        "service" | "svc" => {
            if args.len() < 3 {
                eprintln!(
                    "{}",
                    "Usage: catdog service <start|stop|restart|enable|disable|status|list>".red()
                );
                process::exit(1);
            }
            match args[2].as_str() {
                "start" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog service start <service>".red());
                        process::exit(1);
                    }
                    service_start(&args[3], &config)
                }
                "stop" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog service stop <service>".red());
                        process::exit(1);
                    }
                    service_stop(&args[3], &config)
                }
                "restart" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog service restart <service>".red());
                        process::exit(1);
                    }
                    service_restart(&args[3], &config)
                }
                "enable" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog service enable <service>".red());
                        process::exit(1);
                    }
                    service_enable(&args[3], &config)
                }
                "disable" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog service disable <service>".red());
                        process::exit(1);
                    }
                    service_disable(&args[3], &config)
                }
                "status" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog service status <service>".red());
                        process::exit(1);
                    }
                    service_status(&args[3], &config)
                }
                "list" => service_list(&config),
                _ => {
                    eprintln!(
                        "{}",
                        "Unknown service command. Try: start, stop, restart, enable, disable, status, list"
                            .red()
                    );
                    process::exit(1);
                }
            }
        }
        // System information command
        "info" | "sysinfo" => sys_info(&config),
        // Package management commands
        "pkg" | "package" => {
            if args.len() < 3 {
                eprintln!(
                    "{}",
                    "Usage: catdog pkg <install|remove|update|upgrade|search|list|info>".red()
                );
                process::exit(1);
            }
            match args[2].as_str() {
                "install" | "add" => {
                    if args.len() < 4 {
                        eprintln!(
                            "{}",
                            "Usage: catdog pkg install <package1> [package2...]".red()
                        );
                        process::exit(1);
                    }
                    let packages: Vec<String> = args[3..].to_vec();
                    pkg_install(&packages, &config)
                }
                "remove" | "uninstall" | "delete" => {
                    if args.len() < 4 {
                        eprintln!(
                            "{}",
                            "Usage: catdog pkg remove <package1> [package2...]".red()
                        );
                        process::exit(1);
                    }
                    let packages: Vec<String> = args[3..].to_vec();
                    pkg_remove(&packages, &config)
                }
                "update" | "refresh" => pkg_update(&config),
                "upgrade" => pkg_upgrade(&config),
                "search" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog pkg search <query>".red());
                        process::exit(1);
                    }
                    let query = args[3..].join(" ");
                    pkg_search(&query, &config)
                }
                "list" | "installed" => pkg_list(&config),
                "info" | "check" => {
                    if args.len() < 4 {
                        eprintln!("{}", "Usage: catdog pkg info <package>".red());
                        process::exit(1);
                    }
                    pkg_info(&args[3], &config)
                }
                _ => {
                    eprintln!(
                        "{}",
                        "Unknown package command. Try: install, remove, update, upgrade, search, list, info"
                            .red()
                    );
                    process::exit(1);
                }
            }
        }
        "diff" => {
            if args.len() < 4 {
                eprintln!("{}", "Usage: catdog diff <file1> <file2>".red());
                eprintln!(
                    "       catdog diff --current <file>   {}",
                    "(compare with /etc/fstab)".truecolor(150, 150, 150)
                );
                process::exit(1);
            }
            if args[2] == "--current" {
                diff::compare_with_current(&args[3])
            } else {
                diff::diff_files(&args[2], &args[3])
            }
        }
        "version" | "--version" | "-V" => {
            print_version();
            Ok(())
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

    result
}

fn cat_fstab() -> Result<()> {
    let fstab_path = "/etc/fstab";
    let contents =
        fs::read_to_string(fstab_path).with_context(|| format!("Failed to read {}", fstab_path))?;
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

    println!(
        "{:<30} {:<20} {:<10} {:<30} {} {}",
        "DEVICE".cyan().bold(),
        "MOUNT POINT".cyan().bold(),
        "TYPE".cyan().bold(),
        "OPTIONS".cyan().bold(),
        "DUMP".cyan().bold(),
        "PASS".cyan().bold()
    );
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

        println!(
            "{:<30} {:<20} {:<10} {:<30} {:<4} {}",
            device.to_string(),
            mount_color.to_string(),
            entry.fs_type,
            entry.options.truecolor(180, 180, 180).to_string(),
            entry.dump,
            entry.pass
        );
    }

    println!(
        "\n{} Good dog! Retrieved {} entries",
        "🐕".bold(),
        entries.len().to_string().green().bold()
    );
    Ok(())
}

fn parse_fstab() -> Result<Vec<FstabEntry>> {
    let fstab_path = "/etc/fstab";
    parse_fstab_from_path(fstab_path)
}

fn parse_fstab_from_path(path: &str) -> Result<Vec<FstabEntry>> {
    let contents = fs::read_to_string(path).with_context(|| format!("Failed to read {}", path))?;

    let mut entries = Vec::new();

    for (line_num, line) in contents.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.len() < 6 {
            eprintln!(
                "{} Line {}: Expected 6 fields, found {} - skipping",
                "Warning:".yellow(),
                line_num + 1,
                parts.len()
            );
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
        println!(
            "  {} {} {} {}",
            entry.device.bright_blue(),
            "->".bright_black(),
            entry.mount_point.bright_green(),
            format!("({})", entry.fs_type).truecolor(180, 180, 180)
        );
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
        println!(
            "{} '{}'",
            "No entries found matching".yellow(),
            search.bright_white()
        );
        return Ok(());
    }

    println!(
        "{} {} matching entries:\n",
        "Found".green().bold(),
        found.len().to_string().bright_white().bold()
    );
    println!(
        "{:<30} {:<20} {:<10} {:<30} {} {}",
        "DEVICE".cyan().bold(),
        "MOUNT POINT".cyan().bold(),
        "TYPE".cyan().bold(),
        "OPTIONS".cyan().bold(),
        "DUMP".cyan().bold(),
        "PASS".cyan().bold()
    );
    println!("{}", "=".repeat(120).bright_black());

    for entry in found {
        println!(
            "{:<30} {:<20} {:<10} {:<30} {:<4} {}",
            entry.device, entry.mount_point, entry.fs_type, entry.options, entry.dump, entry.pass
        );
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
        println!(
            "{}",
            "⚠️  /etc/fstab is empty or contains no valid entries".yellow()
        );
        return Ok(());
    }

    // Check for duplicate mount points
    let mut mount_points = std::collections::HashSet::new();
    for (i, entry) in entries.iter().enumerate() {
        if entry.mount_point != "none" && entry.mount_point != "swap" {
            if !mount_points.insert(&entry.mount_point) {
                println!(
                    "{} Entry {}: Duplicate mount point '{}'",
                    "⚠️ ".yellow(),
                    i + 1,
                    entry.mount_point.bright_white()
                );
                issues += 1;
            }
        }
    }

    // Check each entry for common issues
    for (i, entry) in entries.iter().enumerate() {
        // Check root filesystem pass value
        if entry.mount_point == "/" && entry.pass != "1" {
            println!(
                "{} Entry {}: Root filesystem should have pass=1, found pass={}",
                "⚠️ ".yellow(),
                i + 1,
                entry.pass.bright_white()
            );
            issues += 1;
        }

        // Check mount point format
        if entry.mount_point != "none" && entry.mount_point != "swap" {
            if !entry.mount_point.starts_with('/') {
                println!(
                    "{} Entry {}: Mount point '{}' doesn't start with /",
                    "❌".red(),
                    i + 1,
                    entry.mount_point.bright_white()
                );
                issues += 1;
            }
        }

        // Check swap partition configuration
        if entry.fs_type == "swap" && entry.mount_point != "none" && entry.mount_point != "swap" {
            println!(
                "{} Entry {}: Swap partition should have mount point 'none' or 'swap'",
                "⚠️ ".yellow(),
                i + 1
            );
            issues += 1;
        }

        // Check for potentially dangerous options
        if entry.options.contains("noauto") && entry.mount_point == "/" {
            println!(
                "{} Entry {}: Root filesystem with 'noauto' option will not mount at boot!",
                "❌".red(),
                i + 1
            );
            issues += 1;
        }

        // Check pass value validity
        if let Err(_) = entry.pass.parse::<u32>() {
            println!(
                "{} Entry {}: Invalid pass value '{}' (should be 0, 1, or 2)",
                "❌".red(),
                i + 1,
                entry.pass.bright_white()
            );
            issues += 1;
        }

        // Check dump value validity
        if let Err(_) = entry.dump.parse::<u32>() {
            println!(
                "{} Entry {}: Invalid dump value '{}' (should be 0 or 1)",
                "⚠️ ".yellow(),
                i + 1,
                entry.dump.bright_white()
            );
            warnings += 1;
        }

        // Warn about missing mount points
        if entry.mount_point != "none" && entry.mount_point != "swap" {
            if !Path::new(&entry.mount_point).exists() {
                println!(
                    "{} Entry {}: Mount point directory '{}' does not exist",
                    "ℹ️ ".blue(),
                    i + 1,
                    entry.mount_point.bright_white()
                );
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
            println!(
                "{} Found {} critical issue(s)",
                "❌".red(),
                issues.to_string().red().bold()
            );
        }
        if warnings > 0 {
            println!(
                "{} Found {} warning(s)",
                "⚠️ ".yellow(),
                warnings.to_string().yellow().bold()
            );
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
            eprintln!(
                "{} Device discovery not supported on {}",
                "Warning:".yellow(),
                os
            );
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
        } else if line.starts_with("Type (Bundle):") || line.starts_with("File System Personality:")
        {
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
        .args(&[
            "-J",
            "-o",
            "NAME,UUID,PARTUUID,LABEL,FSTYPE,SIZE,MOUNTPOINT,RM,ROTA",
        ])
        .output()
        .context("Failed to run lsblk. Make sure lsblk is installed.")?;

    if !output.status.success() {
        anyhow::bail!("lsblk command failed");
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&json_str).context("Failed to parse lsblk JSON output")?;

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

fn discover_devices(config: &CliConfig) -> Result<()> {
    let devices = discover_block_devices()?;

    if devices.is_empty() {
        if config.json_output {
            println!(
                "{}",
                serde_json::json!({
                    "devices": [],
                    "count": 0
                })
            );
        } else {
            println!("No block devices found");
        }
        return Ok(());
    }

    if config.json_output {
        // JSON output for automation
        let json_devices: Vec<serde_json::Value> = devices
            .iter()
            .map(|d| {
                serde_json::json!({
                    "device": d.device,
                    "uuid": d.uuid,
                    "partuuid": d.partuuid,
                    "label": d.label,
                    "filesystem": d.fs_type,
                    "size": d.size,
                    "mount_point": d.mount_point,
                    "is_ssd": d.is_ssd,
                    "is_removable": d.is_removable
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "devices": json_devices,
                "count": devices.len()
            }))?
        );
    } else {
        // Human-readable output
        println!("Discovering block devices...\n");

        println!(
            "{:<20} {:<38} {:<20} {:<10} {:<10} {:<20}",
            "DEVICE".cyan().bold(),
            "UUID".cyan().bold(),
            "LABEL".cyan().bold(),
            "TYPE".cyan().bold(),
            "SIZE".cyan().bold(),
            "MOUNT POINT".cyan().bold()
        );
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

            print!(
                "{:<20} {:<38} {:<20} {:<10} {:<10} {:<20}",
                device_color.to_string(),
                uuid_display.truecolor(150, 150, 150).to_string(),
                label_display.bright_white().to_string(),
                fs_display.yellow().to_string(),
                size_display,
                mount_display.green().to_string()
            );

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

        println!("\nFound {} block device(s)", devices.len());
    }
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
                rationale
                    .push("noatime: Reduces SSD wear by not updating access times".to_string());
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
    let unmounted: Vec<_> = devices
        .into_iter()
        .filter(|d| {
            let not_system_mounted = d.mount_point.is_none()
                || matches!(
                    d.mount_point.as_deref(),
                    Some("/") | Some("/boot") | Some("/home")
                );

            let matches_filter = if let Some(filter) = device_filter {
                d.device.contains(filter)
                    || d.label.as_ref().map_or(false, |l| l.contains(filter))
                    || d.uuid.as_ref().map_or(false, |u| u.contains(filter))
            } else {
                true
            };

            not_system_mounted && matches_filter && d.fs_type.is_some()
        })
        .collect();

    if unmounted.is_empty() {
        println!(
            "{}",
            "No devices available for mounting suggestions".yellow()
        );
        return Ok(());
    }

    for device in unmounted {
        let suggestion = suggest_mount_options(&device);

        println!("{}", "─".repeat(100).bright_black());
        println!(
            "{} {}",
            "Device:".cyan().bold(),
            device.device.bright_white()
        );

        if let Some(uuid) = &device.uuid {
            println!(
                "  {} {}",
                "UUID:".truecolor(150, 150, 150),
                uuid.truecolor(150, 150, 150)
            );
        }
        if let Some(label) = &device.label {
            println!("  {} {}", "Label:".cyan(), label.bright_white());
        }
        println!(
            "  {} {}",
            "Type:".cyan(),
            suggestion.suggested_fs_type.yellow()
        );
        if let Some(size) = &device.size {
            println!("  {} {}", "Size:".cyan(), size);
        }

        println!("\n{}", "Suggested fstab entry:".green().bold());
        println!(
            "  {} {} {} {} {} {}",
            suggestion.suggested_device_id.bright_yellow(),
            suggestion.suggested_mount_point.bright_green(),
            suggestion.suggested_fs_type.yellow(),
            suggestion
                .suggested_options
                .join(",")
                .truecolor(180, 180, 180),
            "0".truecolor(150, 150, 150),
            "2".truecolor(150, 150, 150)
        );

        if !suggestion.rationale.is_empty() {
            println!("\n{}", "Rationale:".blue().bold());
            for reason in &suggestion.rationale {
                println!("  {} {}", "•".blue(), reason.truecolor(200, 200, 200));
            }
        }

        println!();
    }

    println!("{}", "=".repeat(100).bright_black());
    println!(
        "{} Remember to create the mount point directory before mounting:",
        "Note:".yellow().bold()
    );
    println!("  {}", "sudo mkdir -p <mount_point>".bright_white());
    println!(
        "  {}",
        "sudo mount -a  # Test the configuration".bright_white()
    );

    Ok(())
}

fn print_version() {
    println!("catdog version {}", VERSION);
    println!("Authors: {}", AUTHORS);
    println!("Build: {}", env!("CARGO_PKG_VERSION"));
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

fn get_corpus_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".catdog").join("corpus")
}

fn corpus_ingest(file_path: &str) -> Result<()> {
    println!("{} Adding fstab configuration to library...", "📚".bold());

    let content =
        fs::read_to_string(file_path).with_context(|| format!("Failed to read {}", file_path))?;

    // Parse the fstab
    let entries = parse_fstab_from_path(file_path)?;

    if entries.is_empty() {
        println!("{}", "No valid fstab entries found to ingest".yellow());
        return Ok(());
    }

    // Create corpus storage directory
    let corpus_path = get_corpus_path();
    fs::create_dir_all(&corpus_path)?;

    // Create a storage file for this config
    let config_id = uuid::Uuid::new_v4().to_string();
    let storage_file = corpus_path.join(format!("{}.json", config_id));

    // Store metadata
    let metadata = serde_json::json!({
        "id": config_id,
        "source_file": file_path,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "entry_count": entries.len(),
        "entries": entries.iter().map(|e| serde_json::json!({
            "device": e.device,
            "mount_point": e.mount_point,
            "fs_type": e.fs_type,
            "options": e.options,
            "dump": e.dump,
            "pass": e.pass,
        })).collect::<Vec<_>>(),
    });

    fs::write(&storage_file, serde_json::to_string_pretty(&metadata)?)?;

    println!(
        "{} Successfully added to configuration library",
        "✓".green().bold()
    );
    println!("  {} {}", "Config ID:".cyan(), config_id.bright_white());
    println!("  {} {}", "Source:".cyan(), file_path);
    println!("  {} {}", "Entries:".cyan(), entries.len());
    println!(
        "\n{}",
        "This configuration can now be searched and referenced.".truecolor(150, 150, 150)
    );

    Ok(())
}

fn corpus_search(query: &str) -> Result<()> {
    println!(
        "{} Searching configuration library for: {}\n",
        "🔍".bold(),
        query.bright_white()
    );

    let corpus_path = get_corpus_path();

    if !corpus_path.exists() {
        println!("{}", "No configurations in library yet.".yellow());
        println!(
            "  Use {} to add fstab files",
            "catdog corpus ingest <file>".bright_white()
        );
        return Ok(());
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    // Read all stored configurations
    for entry in fs::read_dir(&corpus_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(&path)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        // Search through entries
        if let Some(entries) = config["entries"].as_array() {
            for (idx, entry) in entries.iter().enumerate() {
                let device = entry["device"].as_str().unwrap_or("");
                let mount_point = entry["mount_point"].as_str().unwrap_or("");
                let fs_type = entry["fs_type"].as_str().unwrap_or("");
                let options = entry["options"].as_str().unwrap_or("");

                // Check if query matches any field
                if device.to_lowercase().contains(&query_lower)
                    || mount_point.to_lowercase().contains(&query_lower)
                    || fs_type.to_lowercase().contains(&query_lower)
                    || options.to_lowercase().contains(&query_lower)
                {
                    matches.push((
                        config["id"].as_str().unwrap_or("unknown").to_string(),
                        config["source_file"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string(),
                        entry.clone(),
                    ));
                }
            }
        }
    }

    if matches.is_empty() {
        println!("{}", "No matching configurations found.".yellow());
        return Ok(());
    }

    println!(
        "{} Found {} matching configuration(s):\n",
        "✓".green().bold(),
        matches.len()
    );

    for (config_id, source, entry) in matches {
        println!("{}", "─".repeat(80).bright_black());
        println!(
            "{} {} {}",
            "From:".cyan().bold(),
            source.bright_white(),
            format!("({})", &config_id[..8]).truecolor(150, 150, 150)
        );
        println!(
            "  {} {}",
            "Device:".cyan(),
            entry["device"].as_str().unwrap_or("")
        );
        println!(
            "  {} {}",
            "Mount:".cyan(),
            entry["mount_point"].as_str().unwrap_or("")
        );
        println!(
            "  {} {}",
            "Type:".cyan(),
            entry["fs_type"].as_str().unwrap_or("")
        );
        println!(
            "  {} {}",
            "Options:".cyan(),
            entry["options"].as_str().unwrap_or("")
        );
        println!();
    }

    Ok(())
}

fn corpus_stats() -> Result<()> {
    println!("{} Configuration Library Statistics\n", "📊".bold());

    let corpus_path = get_corpus_path();

    if !corpus_path.exists() {
        println!("{}", "No configurations in library yet.".yellow());
        println!(
            "  Use {} to add fstab files",
            "catdog corpus ingest <file>".bright_white()
        );
        return Ok(());
    }

    let mut total_configs = 0;
    let mut total_entries = 0;
    let mut fs_types: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut mount_options: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    // Read all stored configurations
    for entry in fs::read_dir(&corpus_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        total_configs += 1;

        let content = fs::read_to_string(&path)?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(entries) = config["entries"].as_array() {
            total_entries += entries.len();

            for entry in entries {
                // Count filesystem types
                if let Some(fs_type) = entry["fs_type"].as_str() {
                    *fs_types.entry(fs_type.to_string()).or_insert(0) += 1;
                }

                // Count mount options
                if let Some(options) = entry["options"].as_str() {
                    for opt in options.split(',') {
                        *mount_options.entry(opt.trim().to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    println!("{}", "Library Overview:".cyan().bold());
    println!(
        "  {} {}",
        "Configurations:".truecolor(150, 150, 150),
        total_configs.to_string().bright_white()
    );
    println!(
        "  {} {}",
        "Total Entries:".truecolor(150, 150, 150),
        total_entries.to_string().bright_white()
    );

    if !fs_types.is_empty() {
        println!("\n{}", "Filesystem Types:".cyan().bold());
        let mut fs_vec: Vec<_> = fs_types.iter().collect();
        fs_vec.sort_by(|a, b| b.1.cmp(a.1));
        for (fs, count) in fs_vec.iter().take(10) {
            println!(
                "  {} {} ({})",
                "•".blue(),
                fs.bright_white(),
                count.to_string().truecolor(150, 150, 150)
            );
        }
    }

    if !mount_options.is_empty() {
        println!("\n{}", "Most Common Mount Options:".cyan().bold());
        let mut opts_vec: Vec<_> = mount_options.iter().collect();
        opts_vec.sort_by(|a, b| b.1.cmp(a.1));
        for (opt, count) in opts_vec.iter().take(10) {
            println!(
                "  {} {} ({})",
                "•".blue(),
                opt.bright_white(),
                count.to_string().truecolor(150, 150, 150)
            );
        }
    }

    println!(
        "\n{}",
        "Use 'catdog corpus search <query>' to find specific configurations"
            .truecolor(150, 150, 150)
    );

    Ok(())
}

// Service management functions
fn service_start(service_name: &str, config: &CliConfig) -> Result<()> {
    println!("{} Starting service...\n", "⚙️".bold());

    let sm = service::detect_service_manager()?;
    println!(
        "{} {}",
        "Detected service manager:".cyan(),
        sm.name().bright_white()
    );

    if sm == service::ServiceManager::Unknown {
        anyhow::bail!("Unable to detect service manager on this system");
    }

    println!();
    service::start_service(service_name, &sm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!(
            "\n{} Service {} started",
            "✓".green().bold(),
            service_name.bright_white()
        );
    }

    Ok(())
}

fn service_stop(service_name: &str, config: &CliConfig) -> Result<()> {
    println!("{} Stopping service...\n", "⚙️".bold());

    let sm = service::detect_service_manager()?;
    println!(
        "{} {}",
        "Detected service manager:".cyan(),
        sm.name().bright_white()
    );

    if sm == service::ServiceManager::Unknown {
        anyhow::bail!("Unable to detect service manager on this system");
    }

    println!();
    service::stop_service(service_name, &sm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!(
            "\n{} Service {} stopped",
            "✓".green().bold(),
            service_name.bright_white()
        );
    }

    Ok(())
}

fn service_restart(service_name: &str, config: &CliConfig) -> Result<()> {
    println!("{} Restarting service...\n", "🔄".bold());

    let sm = service::detect_service_manager()?;
    println!(
        "{} {}",
        "Detected service manager:".cyan(),
        sm.name().bright_white()
    );

    if sm == service::ServiceManager::Unknown {
        anyhow::bail!("Unable to detect service manager on this system");
    }

    println!();
    service::restart_service(service_name, &sm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!(
            "\n{} Service {} restarted",
            "✓".green().bold(),
            service_name.bright_white()
        );
    }

    Ok(())
}

fn service_enable(service_name: &str, config: &CliConfig) -> Result<()> {
    println!("{} Enabling service...\n", "⚙️".bold());

    let sm = service::detect_service_manager()?;
    println!(
        "{} {}",
        "Detected service manager:".cyan(),
        sm.name().bright_white()
    );

    if sm == service::ServiceManager::Unknown {
        anyhow::bail!("Unable to detect service manager on this system");
    }

    println!();
    service::enable_service(service_name, &sm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!(
            "\n{} Service {} enabled",
            "✓".green().bold(),
            service_name.bright_white()
        );
    }

    Ok(())
}

fn service_disable(service_name: &str, config: &CliConfig) -> Result<()> {
    println!("{} Disabling service...\n", "⚙️".bold());

    let sm = service::detect_service_manager()?;
    println!(
        "{} {}",
        "Detected service manager:".cyan(),
        sm.name().bright_white()
    );

    if sm == service::ServiceManager::Unknown {
        anyhow::bail!("Unable to detect service manager on this system");
    }

    println!();
    service::disable_service(service_name, &sm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!(
            "\n{} Service {} disabled",
            "✓".green().bold(),
            service_name.bright_white()
        );
    }

    Ok(())
}

fn service_status(service_name: &str, config: &CliConfig) -> Result<()> {
    let sm = service::detect_service_manager()?;

    if sm == service::ServiceManager::Unknown {
        anyhow::bail!("Unable to detect service manager on this system");
    }

    let info = service::get_service_status(service_name, &sm)?;

    if config.json_output {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("{} Service Status\n", "ℹ️".bold());
        println!("{} {}", "Service:".cyan().bold(), info.name.bright_white());

        let status_str = match info.status {
            service::ServiceStatus::Running => "Running ✓".green().bold(),
            service::ServiceStatus::Stopped => "Stopped".yellow(),
            service::ServiceStatus::Failed => "Failed ✗".red().bold(),
            service::ServiceStatus::Unknown => "Unknown".bright_black(),
        };

        println!("{} {}", "Status:".cyan(), status_str);

        if let Some(enabled) = info.enabled {
            let enabled_str = if enabled {
                "Enabled ✓".green()
            } else {
                "Disabled".yellow()
            };
            println!("{} {}", "Enabled:".cyan(), enabled_str);
        }

        if let Some(pid) = info.pid {
            println!("{} {}", "PID:".cyan(), pid.to_string().bright_white());
        }
    }

    Ok(())
}

fn service_list(config: &CliConfig) -> Result<()> {
    println!("{} Listing services...\n", "📋".bold());

    let sm = service::detect_service_manager()?;

    if sm == service::ServiceManager::Unknown {
        anyhow::bail!("Unable to detect service manager on this system");
    }

    let services = service::list_services(&sm)?;

    if services.is_empty() {
        println!("{}", "No services found".yellow());
        return Ok(());
    }

    if config.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "count": services.len(),
                "services": services
            }))?
        );
    } else {
        println!("{} {} service(s):\n", "✓".green().bold(), services.len());

        println!("{:<40} {}", "SERVICE".cyan().bold(), "STATUS".cyan().bold());
        println!("{}", "=".repeat(60).bright_black());

        for svc in services.iter().take(50) {
            let status_str = match svc.status {
                service::ServiceStatus::Running => "running".green(),
                service::ServiceStatus::Stopped => "stopped".yellow(),
                service::ServiceStatus::Failed => "failed".red(),
                service::ServiceStatus::Unknown => "unknown".bright_black(),
            };

            println!("  {:<38} {}", svc.name.bright_white(), status_str);
        }

        if services.len() > 50 {
            println!(
                "\n{} Showing 50 of {} services",
                "ℹ️".blue(),
                services.len()
            );
        }
    }

    Ok(())
}

// System information function
fn sys_info(config: &CliConfig) -> Result<()> {
    println!("{} Gathering system information...\n", "💻".bold());

    let info = sysinfo::gather_system_info()?;

    if config.json_output {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        // OS Information
        println!("{}", "═".repeat(60).bright_blue());
        println!("{}", "OPERATING SYSTEM".cyan().bold());
        println!("{}", "═".repeat(60).bright_blue());
        println!("{:<20} {}", "Name:".cyan(), info.os.name.bright_white());
        println!(
            "{:<20} {}",
            "Version:".cyan(),
            info.os.version.bright_white()
        );
        println!("{:<20} {}", "Kernel:".cyan(), info.os.kernel.bright_white());
        println!(
            "{:<20} {}",
            "Architecture:".cyan(),
            info.os.architecture.bright_white()
        );
        println!(
            "{:<20} {}",
            "Hostname:".cyan(),
            info.hostname.bright_white()
        );
        if let Some(uptime) = info.uptime {
            println!("{:<20} {}", "Uptime:".cyan(), uptime.bright_white());
        }

        // CPU Information
        println!("\n{}", "═".repeat(60).bright_blue());
        println!("{}", "CPU".cyan().bold());
        println!("{}", "═".repeat(60).bright_blue());
        println!("{:<20} {}", "Model:".cyan(), info.cpu.model.bright_white());
        println!(
            "{:<20} {}",
            "Physical Cores:".cyan(),
            info.cpu.cores.to_string().bright_white()
        );
        if let Some(threads) = info.cpu.threads {
            println!(
                "{:<20} {}",
                "Logical Cores:".cyan(),
                threads.to_string().bright_white()
            );
        }
        if let Some(freq) = info.cpu.frequency {
            println!("{:<20} {}", "Frequency:".cyan(), freq.bright_white());
        }

        // Memory Information
        println!("\n{}", "═".repeat(60).bright_blue());
        println!("{}", "MEMORY".cyan().bold());
        println!("{}", "═".repeat(60).bright_blue());
        println!(
            "{:<20} {}",
            "Total:".cyan(),
            info.memory.total.bright_white()
        );
        println!("{:<20} {}", "Used:".cyan(), info.memory.used.bright_white());
        println!(
            "{:<20} {}",
            "Available:".cyan(),
            info.memory.available.bright_white()
        );
        println!("{:<20} {:.1}%", "Usage:".cyan(), info.memory.percent_used);

        // Disk Information
        if !info.disks.is_empty() {
            println!("\n{}", "═".repeat(60).bright_blue());
            println!("{}", "DISKS".cyan().bold());
            println!("{}", "═".repeat(60).bright_blue());

            for disk in &info.disks {
                println!("\n{} {}", "Mount:".cyan(), disk.mount_point.bright_white());
                println!(
                    "  {:<18} {}",
                    "Device:".truecolor(150, 150, 150),
                    disk.device
                );
                println!(
                    "  {:<18} {}",
                    "Filesystem:".truecolor(150, 150, 150),
                    disk.filesystem
                );
                println!("  {:<18} {}", "Total:".truecolor(150, 150, 150), disk.total);
                println!("  {:<18} {}", "Used:".truecolor(150, 150, 150), disk.used);
                println!(
                    "  {:<18} {}",
                    "Available:".truecolor(150, 150, 150),
                    disk.available
                );

                let usage_color = if disk.percent_used >= 90.0 {
                    disk.percent_used.to_string().red()
                } else if disk.percent_used >= 75.0 {
                    disk.percent_used.to_string().yellow()
                } else {
                    disk.percent_used.to_string().green()
                };
                println!(
                    "  {:<18} {}%",
                    "Usage:".truecolor(150, 150, 150),
                    usage_color
                );
            }
        }

        // Network Information
        if !info.network.interfaces.is_empty() {
            println!("\n{}", "═".repeat(60).bright_blue());
            println!("{}", "NETWORK".cyan().bold());
            println!("{}", "═".repeat(60).bright_blue());

            for iface in &info.network.interfaces {
                // Skip loopback and other virtual interfaces for cleaner output
                if iface.name.starts_with("lo") || iface.ip_address.is_none() {
                    continue;
                }

                println!("\n{} {}", "Interface:".cyan(), iface.name.bright_white());
                if let Some(ref ip) = iface.ip_address {
                    println!("  {:<18} {}", "IP Address:".truecolor(150, 150, 150), ip);
                }
                if let Some(ref mac) = iface.mac_address {
                    println!("  {:<18} {}", "MAC Address:".truecolor(150, 150, 150), mac);
                }
            }
        }

        println!("\n{}", "═".repeat(60).bright_blue());
    }

    Ok(())
}

// Package management functions
fn pkg_install(packages: &[String], config: &CliConfig) -> Result<()> {
    println!("{} Installing packages...\n", "📦".bold());

    let pm = package::detect_package_manager()?;
    println!(
        "{} {}",
        "Detected package manager:".cyan(),
        pm.name().bright_white()
    );

    if pm == package::PackageManager::Unknown {
        anyhow::bail!("Unable to detect package manager on this system");
    }

    println!();
    package::install_packages(packages, &pm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!(
            "\n{} Successfully installed {} package(s)",
            "✓".green().bold(),
            packages.len()
        );
    }

    Ok(())
}

fn pkg_remove(packages: &[String], config: &CliConfig) -> Result<()> {
    println!("{} Removing packages...\n", "📦".bold());

    let pm = package::detect_package_manager()?;
    println!(
        "{} {}",
        "Detected package manager:".cyan(),
        pm.name().bright_white()
    );

    if pm == package::PackageManager::Unknown {
        anyhow::bail!("Unable to detect package manager on this system");
    }

    println!();
    package::remove_packages(packages, &pm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!(
            "\n{} Successfully removed {} package(s)",
            "✓".green().bold(),
            packages.len()
        );
    }

    Ok(())
}

fn pkg_update(config: &CliConfig) -> Result<()> {
    println!("{} Updating package cache...\n", "🔄".bold());

    let pm = package::detect_package_manager()?;
    println!(
        "{} {}",
        "Detected package manager:".cyan(),
        pm.name().bright_white()
    );

    if pm == package::PackageManager::Unknown {
        anyhow::bail!("Unable to detect package manager on this system");
    }

    println!();
    package::update_cache(&pm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!("\n{} Package cache updated", "✓".green().bold());
    }

    Ok(())
}

fn pkg_upgrade(config: &CliConfig) -> Result<()> {
    println!("{} Upgrading all packages...\n", "⬆️".bold());

    let pm = package::detect_package_manager()?;
    println!(
        "{} {}",
        "Detected package manager:".cyan(),
        pm.name().bright_white()
    );

    if pm == package::PackageManager::Unknown {
        anyhow::bail!("Unable to detect package manager on this system");
    }

    println!();
    package::upgrade_packages(&pm, config.dry_run, config.verbose)?;

    if !config.dry_run {
        println!("\n{} All packages upgraded", "✓".green().bold());
    }

    Ok(())
}

fn pkg_search(query: &str, config: &CliConfig) -> Result<()> {
    println!(
        "{} Searching for packages matching: {}\n",
        "🔍".bold(),
        query.bright_white()
    );

    let pm = package::detect_package_manager()?;

    if pm == package::PackageManager::Unknown {
        anyhow::bail!("Unable to detect package manager on this system");
    }

    let packages = package::search_packages(query, &pm)?;

    if packages.is_empty() {
        println!("{}", "No packages found".yellow());
        return Ok(());
    }

    if config.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "query": query,
                "count": packages.len(),
                "packages": packages
            }))?
        );
    } else {
        println!(
            "{} Found {} package(s):\n",
            "✓".green().bold(),
            packages.len()
        );

        for pkg in packages.iter().take(50) {
            // Limit to first 50 results
            print!("  {} {}", "•".blue(), pkg.name.bright_white());
            if let Some(version) = &pkg.version {
                print!(" {}", version.truecolor(150, 150, 150));
            }
            if let Some(description) = &pkg.description {
                print!(" - {}", description.truecolor(180, 180, 180));
            }
            println!();
        }

        if packages.len() > 50 {
            println!("\n{} Showing 50 of {} results", "ℹ️".blue(), packages.len());
        }
    }

    Ok(())
}

fn pkg_list(config: &CliConfig) -> Result<()> {
    println!("{} Listing installed packages...\n", "📋".bold());

    let pm = package::detect_package_manager()?;

    if pm == package::PackageManager::Unknown {
        anyhow::bail!("Unable to detect package manager on this system");
    }

    let packages = package::list_installed(&pm)?;

    if packages.is_empty() {
        println!("{}", "No packages installed".yellow());
        return Ok(());
    }

    if config.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "count": packages.len(),
                "packages": packages
            }))?
        );
    } else {
        println!(
            "{} {} installed package(s):\n",
            "✓".green().bold(),
            packages.len()
        );

        println!(
            "{:<40} {}",
            "PACKAGE".cyan().bold(),
            "VERSION".cyan().bold()
        );
        println!("{}", "=".repeat(60).bright_black());

        for pkg in &packages {
            print!("  {:<38}", pkg.name.bright_white());
            if let Some(version) = &pkg.version {
                print!(" {}", version.truecolor(150, 150, 150));
            }
            println!();
        }

        println!("\n{} Total: {} packages", "📦".bold(), packages.len());
    }

    Ok(())
}

fn pkg_info(package_name: &str, config: &CliConfig) -> Result<()> {
    println!(
        "{} Checking package: {}\n",
        "ℹ️".bold(),
        package_name.bright_white()
    );

    let pm = package::detect_package_manager()?;

    if pm == package::PackageManager::Unknown {
        anyhow::bail!("Unable to detect package manager on this system");
    }

    let is_installed = package::is_package_installed(package_name, &pm)?;

    if config.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "package": package_name,
                "installed": is_installed,
                "package_manager": pm.name()
            }))?
        );
    } else {
        println!(
            "{} {}",
            "Package:".cyan().bold(),
            package_name.bright_white()
        );
        println!("{} {}", "Package Manager:".cyan(), pm.name().bright_white());

        if is_installed {
            println!("{} {}", "Status:".cyan(), "Installed ✓".green().bold());
        } else {
            println!("{} {}", "Status:".cyan(), "Not installed".yellow());
        }
    }

    Ok(())
}

fn generate_fstab(output_file: Option<&str>, dry_run: bool) -> Result<()> {
    println!("{} Generating fstab entries...\n", "🔧".bold());

    let devices = discover_block_devices()?;

    if devices.is_empty() {
        println!("{}", "No block devices found".yellow());
        return Ok(());
    }

    // Build the fstab content
    let mut fstab_content = String::new();

    // Add header
    fstab_content.push_str("# /etc/fstab: static file system information\n");
    fstab_content.push_str("#\n");
    fstab_content.push_str(
        "# Generated by catdog - A filesystem utility that takes itself way too seriously\n",
    );
    fstab_content.push_str(&format!(
        "# Generated at: {}\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));
    fstab_content.push_str("#\n");
    fstab_content.push_str("# <device>                                <mount point>    <type>  <options>              <dump> <pass>\n");
    fstab_content.push_str("#\n\n");

    let mut entry_count = 0;

    // Generate entries for each device
    for device in devices {
        // Skip devices that are already mounted at system locations
        if let Some(ref mp) = device.mount_point {
            if mp == "/" || mp == "/boot" || mp == "/boot/efi" {
                continue;
            }
        }

        // Skip if no filesystem
        if device.fs_type.is_none() {
            continue;
        }

        let suggestion = suggest_mount_options(&device);

        // Add comment with device info
        fstab_content.push_str(&format!("# Device: {}\n", device.device));
        if let Some(ref label) = device.label {
            fstab_content.push_str(&format!("# Label: {}\n", label));
        }
        if let Some(ref size) = device.size {
            fstab_content.push_str(&format!("# Size: {}\n", size));
        }
        if device.is_ssd {
            fstab_content.push_str("# Type: SSD (optimized options applied)\n");
        }
        if device.is_removable {
            fstab_content.push_str("# Type: Removable (nofail option applied)\n");
        }

        // Add the fstab entry
        fstab_content.push_str(&format!(
            "{:<40} {:<20} {:<7} {:<22} {} {}\n",
            suggestion.suggested_device_id,
            suggestion.suggested_mount_point,
            suggestion.suggested_fs_type,
            suggestion.suggested_options.join(","),
            "0",
            if suggestion.suggested_mount_point == "/" {
                "1"
            } else {
                "2"
            }
        ));
        fstab_content.push('\n');

        entry_count += 1;
    }

    if entry_count == 0 {
        println!("{}", "No devices found that need fstab entries".yellow());
        println!("  Discovered devices are either already mounted at system locations");
        println!("  or don't have filesystems that can be mounted.");
        return Ok(());
    }

    // Add footer
    fstab_content.push_str("# End of generated fstab entries\n");
    fstab_content.push_str(&format!("# Total entries generated: {}\n", entry_count));
    fstab_content.push_str("#\n");
    fstab_content.push_str("# IMPORTANT: Review these entries carefully before using!\n");
    fstab_content.push_str("# 1. Create mount point directories: sudo mkdir -p <mount_point>\n");
    fstab_content.push_str("# 2. Test with: sudo mount -a\n");
    fstab_content.push_str("# 3. Check with: df -h\n");

    // Output the result
    match output_file {
        Some(file_path) => {
            if dry_run {
                println!(
                    "{} Would write fstab to: {}",
                    "[DRY-RUN]".yellow().bold(),
                    file_path.bright_white()
                );
                println!("\n{}", "Preview of content:".cyan().bold());
                println!("{}", "=".repeat(100).bright_black());
                print!("{}", fstab_content);
                println!("{}", "=".repeat(100).bright_black());
            } else {
                // Create backup before writing if file exists
                let path = Path::new(file_path);
                if path.exists() {
                    println!("{} Creating backup before modification...", "💾".blue());
                    let backup_metadata = backup::create_backup(
                        file_path,
                        backup::BackupReason::PreFstabModification,
                        false,
                    )?;
                    println!(
                        "{} Backup created: {}",
                        "✓".green(),
                        backup_metadata.backup_path.bright_white()
                    );
                }

                fs::write(file_path, &fstab_content)
                    .with_context(|| format!("Failed to write to {}", file_path))?;
                println!(
                    "{} Generated fstab written to: {}",
                    "✓".green().bold(),
                    file_path.bright_white()
                );
            }
            println!("\n{}", "Next steps:".cyan().bold());
            println!(
                "  1. Review the file: {}",
                format!("cat {}", file_path).bright_white()
            );
            println!("  2. Create mount directories for each entry");
            println!(
                "  3. Back up your current fstab: {}",
                "sudo cp /etc/fstab /etc/fstab.backup".bright_white()
            );
            println!("  4. Merge with your existing fstab if needed");
            println!(
                "\n{} Generated {} fstab entries",
                "📝".bold(),
                entry_count.to_string().green().bold()
            );
        }
        None => {
            // Print to stdout
            println!("{}", "Generated fstab content:".cyan().bold());
            println!("{}", "=".repeat(100).bright_black());
            print!("{}", fstab_content);
            println!("{}", "=".repeat(100).bright_black());
            println!("\n{}", "To save to a file, use:".cyan().bold());
            println!("  {}", "catdog generate fstab.new".bright_white());
            println!(
                "\n{} Generated {} fstab entries",
                "📝".bold(),
                entry_count.to_string().green().bold()
            );
        }
    }

    Ok(())
}

// Backup command handlers
fn backup_file_cmd(file_path: &str, dry_run: bool) -> Result<()> {
    println!("{} Creating backup...\n", "💾".bold());

    let metadata = backup::create_backup(file_path, backup::BackupReason::Manual, dry_run)?;

    if !dry_run {
        println!("{} Backup created successfully", "✓".green().bold());
        backup::display_backup_info(&metadata);
    }

    Ok(())
}

fn restore_backup_cmd(backup_path: &str, dry_run: bool, force: bool) -> Result<()> {
    println!("{} Restoring from backup...\n", "♻️".bold());

    backup::restore_backup(backup_path, dry_run, force)?;

    if !dry_run {
        println!("\n{} Backup restored successfully", "✓".green().bold());
    }

    Ok(())
}

fn list_backups_cmd(file_path: &str) -> Result<()> {
    println!(
        "{} Listing backups for: {}\n",
        "📋".bold(),
        file_path.bright_white()
    );

    let backups = backup::list_backups(file_path)?;
    backup::display_backups(&backups);

    Ok(())
}

fn backup_stats_cmd() -> Result<()> {
    let stats = backup::get_backup_stats()?;
    stats.display();
    Ok(())
}

fn backup_health_cmd() -> Result<()> {
    println!("{} Running backup health check...\n", "🏥".bold());

    let health = backup::run_health_check()?;
    health.display();

    // Emit event
    if health.is_healthy() {
        let _ = backup::emit_backup_event(
            backup::BackupEventType::HealthCheckPassed,
            "all",
            &format!(
                "{}/{} backups healthy",
                health.healthy_backups, health.total_backups
            ),
            backup::EventSeverity::Info,
        );
    } else {
        let _ = backup::emit_backup_event(
            backup::BackupEventType::HealthCheckFailed,
            "all",
            &format!(
                "{} corrupted, {} errors",
                health.corrupted_backups.len(),
                health.errors.len()
            ),
            backup::EventSeverity::Critical,
        );
    }

    // Exit with error code if unhealthy
    if !health.is_healthy() {
        process::exit(1);
    }

    Ok(())
}

fn backup_drill_cmd() -> Result<()> {
    println!("{} Running backup restoration drill...\n", "🎯".bold());
    println!(
        "{} This will verify all backups can be restored (read-only test)\n",
        "ℹ️".blue()
    );

    let drill = backup::run_restoration_drill()?;
    drill.display();

    // Emit event
    let success_rate = if drill.total_tested > 0 {
        (drill.successful as f64 / drill.total_tested as f64) * 100.0
    } else {
        0.0
    };

    if success_rate == 100.0 {
        let _ = backup::emit_backup_event(
            backup::BackupEventType::DrillPassed,
            "all",
            &format!(
                "{}/{} backups verified in {} ms",
                drill.successful, drill.total_tested, drill.duration_ms
            ),
            backup::EventSeverity::Info,
        );
    } else {
        let _ = backup::emit_backup_event(
            backup::BackupEventType::DrillFailed,
            "all",
            &format!(
                "{} of {} backups failed verification",
                drill.failed.len(),
                drill.total_tested
            ),
            backup::EventSeverity::Warning,
        );
    }

    // Exit with error code if failures
    if !drill.failed.is_empty() {
        process::exit(1);
    }

    Ok(())
}

fn print_help() {
    println!(
        "{} {} A professional filesystem management tool",
        "catdog".bright_green().bold(),
        VERSION.bright_black()
    );
    println!("\n{}", "USAGE:".cyan().bold());
    println!("    catdog [FLAGS] <COMMAND> [ARGS]\n");

    println!("{}", "FLAGS:".cyan().bold());
    println!(
        "    {}         Output in JSON format (for automation)",
        "--json".bright_yellow()
    );
    println!(
        "    {}      Disable colored output",
        "--no-color".bright_yellow()
    );
    println!(
        "    {}       Show preview without making changes",
        "--dry-run".bright_yellow()
    );
    println!(
        "    {}    Enable verbose logging",
        "-v, --verbose".bright_yellow()
    );
    println!(
        "    {}  Show version information",
        "-V, --version".bright_yellow()
    );
    println!();

    println!(
        "{} {}",
        "FILESYSTEM".cyan().bold(),
        "COMMANDS:".cyan().bold()
    );
    println!(
        "    {}          Display raw /etc/fstab file",
        "cat".bright_yellow()
    );
    println!(
        "    {}          Parse and display /etc/fstab in table format",
        "dog".bright_yellow()
    );
    println!(
        "    {}     List all mount points",
        "list, ls".bright_yellow()
    );
    println!(
        "    {}  Find entries matching device or mount point",
        "find <term>".bright_yellow()
    );
    println!(
        "    {}     Check /etc/fstab for common issues",
        "validate".bright_yellow()
    );
    println!(
        "    {}    Discover available block devices (supports --json)",
        "discover".bright_yellow()
    );
    println!(
        "    {}       Generate smart mount suggestions for devices",
        "suggest [device]".bright_yellow()
    );
    println!(
        "    {}       Generate complete fstab from discovered devices",
        "generate [file]".bright_yellow()
    );
    println!(
        "    {}        Create verified backup with metadata",
        "backup [file]".bright_yellow()
    );
    println!(
        "    {}      Restore from a backup (use --force to override)",
        "restore <backup>".bright_yellow()
    );
    println!(
        "    {}  List all backups for a file",
        "list-backups <file>".bright_yellow()
    );
    println!(
        "    {}   Show backup statistics and disk usage",
        "backup-stats".bright_yellow()
    );
    println!(
        "    {}  Run backup health check and verification",
        "backup-health".bright_yellow()
    );
    println!(
        "    {}   Test backup restoration (dry-run drill)",
        "backup-drill".bright_yellow()
    );
    println!(
        "    {}  Compare two fstab files with colored diff",
        "diff <file1> <file2>".bright_yellow()
    );

    println!(
        "\n{} {} {}",
        "BARK".cyan().bold(),
        "(ALERTING)".bright_black(),
        "COMMANDS:".cyan().bold()
    );
    println!(
        "    {}       Run filesystem health checks once",
        "check".bright_yellow()
    );
    println!(
        "    {}       Start continuous monitoring (default: 300s interval)",
        "monitor [interval]".bright_yellow()
    );
    println!(
        "    {}        List all barks (optionally filter: firing/acknowledged/resolved/silenced)",
        "barks [status]".bright_yellow()
    );
    println!(
        "    {}         Show detailed information about a bark",
        "bark <id>".bright_yellow()
    );
    println!(
        "    {}           Acknowledge a bark (alias: pet)",
        "ack <id>".bright_yellow()
    );
    println!(
        "    {}      Resolve a bark (alias: quiet)",
        "resolve <id>".bright_yellow()
    );
    println!(
        "    {}     Silence a bark (alias: hush)",
        "silence <id>".bright_yellow()
    );

    println!("\n{} {}", "CORPUS".cyan().bold(), "COMMANDS:".cyan().bold());
    println!(
        "    {}       Ingest a file into the corpus",
        "corpus ingest <file>".bright_yellow()
    );
    println!(
        "    {}       Search the corpus",
        "corpus search <query>".bright_yellow()
    );
    println!(
        "    {}       Show corpus statistics",
        "corpus stats".bright_yellow()
    );

    println!(
        "\n{} {}",
        "SERVICE".cyan().bold(),
        "MANAGEMENT:".cyan().bold()
    );
    println!(
        "    {}       Start a service",
        "service start <service>".bright_yellow()
    );
    println!(
        "    {}        Stop a service",
        "service stop <service>".bright_yellow()
    );
    println!(
        "    {}     Restart a service",
        "service restart <service>".bright_yellow()
    );
    println!(
        "    {}      Enable a service to start on boot",
        "service enable <service>".bright_yellow()
    );
    println!(
        "    {}     Disable a service from starting on boot",
        "service disable <service>".bright_yellow()
    );
    println!(
        "    {}      Get service status",
        "service status <service>".bright_yellow()
    );
    println!(
        "    {}       List all services (supports --json)",
        "service list".bright_yellow()
    );

    println!(
        "\n{} {}",
        "SYSTEM".cyan().bold(),
        "INFORMATION:".cyan().bold()
    );
    println!(
        "    {}         Show comprehensive system information (supports --json)",
        "info".bright_yellow()
    );

    println!(
        "\n{} {}",
        "PACKAGE".cyan().bold(),
        "MANAGEMENT:".cyan().bold()
    );
    println!(
        "    {}       Install packages (supports --dry-run)",
        "pkg install <pkg1> [pkg2...]".bright_yellow()
    );
    println!(
        "    {}        Remove packages",
        "pkg remove <pkg1> [pkg2...]".bright_yellow()
    );
    println!(
        "    {}       Update package cache/repositories",
        "pkg update".bright_yellow()
    );
    println!(
        "    {}       Upgrade all installed packages",
        "pkg upgrade".bright_yellow()
    );
    println!(
        "    {}       Search for packages",
        "pkg search <query>".bright_yellow()
    );
    println!(
        "    {}       List all installed packages (supports --json)",
        "pkg list".bright_yellow()
    );
    println!(
        "    {}       Check if a package is installed",
        "pkg info <package>".bright_yellow()
    );

    println!(
        "\n    {}         Show this help message",
        "help".bright_yellow()
    );

    println!("\n{}", "EXAMPLES:".cyan().bold());
    println!(
        "    catdog cat                 {} Show raw fstab file",
        "#".bright_black()
    );
    println!(
        "    catdog dog                 {} Parse and display fstab nicely",
        "#".bright_black()
    );
    println!(
        "    catdog find /dev           {} Find all entries with /dev",
        "#".bright_black()
    );
    println!(
        "    catdog validate            {} Check for common issues",
        "#".bright_black()
    );
    println!(
        "    catdog discover            {} List all block devices with details",
        "#".bright_black()
    );
    println!(
        "    catdog suggest             {} Generate fstab entries with smart defaults",
        "#".bright_black()
    );
    println!(
        "    catdog generate fstab.new  {} Generate complete fstab file",
        "#".bright_black()
    );
    println!(
        "    catdog diff fstab.old fstab.new {} Compare two fstab files",
        "#".bright_black()
    );
    println!(
        "    catdog backup /etc/fstab     {} Create verified backup with checksum",
        "#".bright_black()
    );
    println!(
        "    catdog list-backups /etc/fstab {} Show all backups for a file",
        "#".bright_black()
    );
    println!(
        "    catdog restore <backup_path> {} Restore from a backup",
        "#".bright_black()
    );
    println!(
        "    catdog backup-stats          {} Show backup storage statistics",
        "#".bright_black()
    );
    println!(
        "    catdog backup-health         {} Verify all backups are healthy",
        "#".bright_black()
    );
    println!(
        "    catdog backup-drill          {} Test restoration of all backups",
        "#".bright_black()
    );
    println!(
        "    catdog check               {} Run health checks once",
        "#".bright_black()
    );
    println!(
        "    catdog monitor 60          {} Start monitoring with 60s interval",
        "#".bright_black()
    );
    println!(
        "    catdog barks               {} List all barks (alerts)",
        "#".bright_black()
    );
    println!(
        "    catdog barks firing        {} List only firing barks",
        "#".bright_black()
    );
    println!(
        "    catdog bark <id>           {} Show bark details",
        "#".bright_black()
    );
    println!(
        "    catdog pet <id>            {} Pet the dog (acknowledge bark)",
        "#".bright_black()
    );
    println!(
        "    catdog quiet <id>          {} Quiet the dog (resolve bark)",
        "#".bright_black()
    );
    println!(
        "    catdog pkg install nginx   {} Install nginx package",
        "#".bright_black()
    );
    println!(
        "    catdog pkg search docker   {} Search for docker packages",
        "#".bright_black()
    );
    println!(
        "    catdog pkg list            {} List all installed packages",
        "#".bright_black()
    );
    println!(
        "    catdog --json pkg list     {} Get installed packages as JSON",
        "#".bright_black()
    );
    println!(
        "    catdog service status ssh  {} Check SSH service status",
        "#".bright_black()
    );
    println!(
        "    catdog service restart nginx {} Restart nginx service",
        "#".bright_black()
    );
    println!(
        "    catdog info                {} Show complete system information",
        "#".bright_black()
    );
    println!(
        "    catdog info --json         {} Get system info as JSON",
        "#".bright_black()
    );
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
