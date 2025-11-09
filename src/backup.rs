use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use colored::*;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_BACKUPS_PER_FILE: usize = 10;
const BACKUP_DIR_NAME: &str = ".catdog_backups";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub original_path: String,
    pub backup_path: String,
    pub timestamp: String,
    pub reason: BackupReason,
    pub checksum: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupReason {
    Manual,
    PrePackageOperation(String),
    PreServiceOperation(String),
    PreFstabModification,
    PreSystemChange,
}

impl BackupReason {
    pub fn description(&self) -> String {
        match self {
            BackupReason::Manual => "Manual backup".to_string(),
            BackupReason::PrePackageOperation(op) => format!("Before package operation: {}", op),
            BackupReason::PreServiceOperation(op) => format!("Before service operation: {}", op),
            BackupReason::PreFstabModification => "Before fstab modification".to_string(),
            BackupReason::PreSystemChange => "Before system change".to_string(),
        }
    }
}

/// Get the backup directory for a given file
fn get_backup_dir(file_path: &Path) -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    let backup_base = home.join(BACKUP_DIR_NAME);

    // Create subdirectory based on original file path to organize backups
    let sanitized_path = file_path
        .to_string_lossy()
        .replace('/', "_")
        .replace('\\', "_")
        .trim_start_matches('_')
        .to_string();

    let backup_dir = backup_base.join(sanitized_path);
    fs::create_dir_all(&backup_dir).with_context(|| {
        format!(
            "Failed to create backup directory: {}",
            backup_dir.display()
        )
    })?;

    Ok(backup_dir)
}

/// Create a backup of a file with metadata
pub fn create_backup(
    file_path: &str,
    reason: BackupReason,
    dry_run: bool,
) -> Result<BackupMetadata> {
    let source = Path::new(file_path);

    if !source.exists() {
        anyhow::bail!("Source file does not exist: {}", file_path);
    }

    // Get file metadata
    let metadata = fs::metadata(source)
        .with_context(|| format!("Failed to read metadata for {}", file_path))?;

    if !metadata.is_file() {
        anyhow::bail!("Path is not a regular file: {}", file_path);
    }

    let size_bytes = metadata.len();
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();

    // Calculate checksum
    let checksum = calculate_checksum(source)?;

    // Get backup directory and create backup filename
    let backup_dir = get_backup_dir(source)?;
    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let backup_filename = format!("{}.backup.{}", filename, timestamp);
    let backup_path = backup_dir.join(backup_filename);

    if dry_run {
        println!(
            "{} Would create backup: {}",
            "[DRY-RUN]".yellow().bold(),
            backup_path.display().to_string().bright_white()
        );

        return Ok(BackupMetadata {
            original_path: file_path.to_string(),
            backup_path: backup_path.to_string_lossy().to_string(),
            timestamp: timestamp.clone(),
            reason,
            checksum,
            size_bytes,
        });
    }

    // Perform the backup
    debug!(
        "Creating backup: {} -> {}",
        file_path,
        backup_path.display()
    );
    fs::copy(source, &backup_path)
        .with_context(|| format!("Failed to create backup at {}", backup_path.display()))?;

    // Verify the backup
    verify_backup(source, &backup_path)?;

    let metadata = BackupMetadata {
        original_path: file_path.to_string(),
        backup_path: backup_path.to_string_lossy().to_string(),
        timestamp: timestamp.clone(),
        reason: reason.clone(),
        checksum: checksum.clone(),
        size_bytes,
    };

    // Save metadata
    save_metadata(&metadata)?;

    info!(
        "Created backup: {} (reason: {})",
        backup_path.display(),
        reason.description()
    );

    // Emit backup event
    let _ = emit_backup_event(
        BackupEventType::BackupCreated,
        file_path,
        &format!(
            "Backup created: {} bytes, checksum {}",
            size_bytes,
            &checksum[..16]
        ),
        EventSeverity::Info,
    );

    // Cleanup old backups
    cleanup_old_backups(&backup_dir)?;

    Ok(metadata)
}

/// Verify a backup by comparing checksums
fn verify_backup(original: &Path, backup: &Path) -> Result<()> {
    let original_checksum = calculate_checksum(original)?;
    let backup_checksum = calculate_checksum(backup)?;

    if original_checksum != backup_checksum {
        anyhow::bail!(
            "Backup verification failed: checksums don't match\nOriginal: {}\nBackup: {}",
            original_checksum,
            backup_checksum
        );
    }

    debug!("Backup verified successfully: {}", backup.display());
    Ok(())
}

/// Calculate SHA-256 checksum of a file
fn calculate_checksum(path: &Path) -> Result<String> {
    use std::io::Read;

    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open file for checksum: {}", path.display()))?;

    let mut hasher = sha256::Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finish())
}

/// Save backup metadata to a JSON file
fn save_metadata(metadata: &BackupMetadata) -> Result<()> {
    let backup_path = Path::new(&metadata.backup_path);
    let metadata_path = backup_path.with_extension("backup.json");

    let json =
        serde_json::to_string_pretty(metadata).context("Failed to serialize backup metadata")?;

    fs::write(&metadata_path, json)
        .with_context(|| format!("Failed to write metadata to {}", metadata_path.display()))?;

    Ok(())
}

/// Load backup metadata from a JSON file
fn load_metadata(backup_path: &Path) -> Result<BackupMetadata> {
    let metadata_path = backup_path.with_extension("backup.json");

    let json = fs::read_to_string(&metadata_path)
        .with_context(|| format!("Failed to read metadata from {}", metadata_path.display()))?;

    let metadata: BackupMetadata =
        serde_json::from_str(&json).context("Failed to parse backup metadata")?;

    Ok(metadata)
}

/// Cleanup old backups, keeping only MAX_BACKUPS_PER_FILE most recent
fn cleanup_old_backups(backup_dir: &Path) -> Result<()> {
    let mut backups: Vec<PathBuf> = Vec::new();

    for entry in fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only consider backup files (not metadata)
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.contains(".backup.") {
                    backups.push(path);
                }
            }
        }
    }

    // Sort by modification time (newest first)
    backups.sort_by(|a, b| {
        let a_time = fs::metadata(a).and_then(|m| m.modified()).ok();
        let b_time = fs::metadata(b).and_then(|m| m.modified()).ok();
        b_time.cmp(&a_time)
    });

    // Remove old backups
    if backups.len() > MAX_BACKUPS_PER_FILE {
        for backup in backups.iter().skip(MAX_BACKUPS_PER_FILE) {
            debug!("Removing old backup: {}", backup.display());

            // Remove backup file
            if let Err(e) = fs::remove_file(backup) {
                warn!("Failed to remove old backup {}: {}", backup.display(), e);
            }

            // Remove metadata file
            let metadata_path = backup.with_extension("backup.json");
            if metadata_path.exists() {
                if let Err(e) = fs::remove_file(&metadata_path) {
                    warn!(
                        "Failed to remove metadata {}: {}",
                        metadata_path.display(),
                        e
                    );
                }
            }
        }

        let removed_count = backups.len() - MAX_BACKUPS_PER_FILE;
        info!("Cleaned up {} old backup(s)", removed_count);
    }

    Ok(())
}

/// List all backups for a specific file
pub fn list_backups(file_path: &str) -> Result<Vec<BackupMetadata>> {
    let source = Path::new(file_path);
    let backup_dir = get_backup_dir(source)?;

    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();

    for entry in fs::read_dir(&backup_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Look for metadata files
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(metadata) = load_metadata(&path.with_extension("")) {
                backups.push(metadata);
            }
        }
    }

    // Sort by timestamp (newest first)
    backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(backups)
}

/// Restore a file from a backup
pub fn restore_backup(backup_path: &str, dry_run: bool, force: bool) -> Result<()> {
    let backup = Path::new(backup_path);

    if !backup.exists() {
        anyhow::bail!("Backup file does not exist: {}", backup_path);
    }

    // Load metadata
    let metadata = load_metadata(backup).context("Failed to load backup metadata")?;

    let original = Path::new(&metadata.original_path);

    // Check if original file exists and hasn't been modified
    if original.exists() && !force {
        let current_checksum = calculate_checksum(original)?;
        if current_checksum != metadata.checksum {
            anyhow::bail!(
                "Original file has been modified since backup. Use --force to override.\nOriginal: {}\nCurrent: {}",
                metadata.original_path,
                original.display()
            );
        }
    }

    if dry_run {
        println!(
            "{} Would restore: {} -> {}",
            "[DRY-RUN]".yellow().bold(),
            backup_path.bright_white(),
            metadata.original_path.bright_white()
        );
        return Ok(());
    }

    // Create backup of current state before restoring
    if original.exists() {
        let pre_restore_backup = create_backup(
            &metadata.original_path,
            BackupReason::PreSystemChange,
            false,
        )?;
        info!(
            "Created pre-restore backup: {}",
            pre_restore_backup.backup_path
        );
    }

    // Perform the restore
    fs::copy(backup, original)
        .with_context(|| format!("Failed to restore backup to {}", original.display()))?;

    // Verify the restore
    verify_backup(backup, original)?;

    info!("Successfully restored: {}", metadata.original_path);

    // Emit restore event
    let _ = emit_backup_event(
        BackupEventType::BackupRestored,
        &metadata.original_path,
        &format!("Restored from backup: {}", backup_path),
        EventSeverity::Info,
    );

    Ok(())
}

/// Display backup information
pub fn display_backup_info(metadata: &BackupMetadata) {
    println!("{}", "─".repeat(80).bright_black());
    println!(
        "{} {}",
        "Backup:".cyan().bold(),
        metadata.backup_path.bright_white()
    );
    println!("  {} {}", "Original:".cyan(), metadata.original_path);
    println!("  {} {}", "Timestamp:".cyan(), metadata.timestamp);
    println!("  {} {}", "Reason:".cyan(), metadata.reason.description());
    println!("  {} {}", "Size:".cyan(), format_bytes(metadata.size_bytes));
    println!(
        "  {} {}",
        "Checksum:".cyan(),
        &metadata.checksum[..16].truecolor(150, 150, 150)
    );
}

/// Display list of backups
pub fn display_backups(backups: &[BackupMetadata]) {
    if backups.is_empty() {
        println!("{}", "No backups found".yellow());
        return;
    }

    println!(
        "\n{} Found {} backup(s):\n",
        "✓".green().bold(),
        backups.len().to_string().bright_white()
    );

    for backup in backups {
        display_backup_info(backup);
    }

    println!("\n{}", "─".repeat(80).bright_black());
    println!(
        "{} Use 'catdog restore <backup_path>' to restore a backup",
        "Tip:".blue().bold()
    );
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

/// Get backup statistics
pub fn get_backup_stats() -> Result<BackupStats> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    let backup_base = home.join(BACKUP_DIR_NAME);

    if !backup_base.exists() {
        return Ok(BackupStats {
            total_backups: 0,
            total_size_bytes: 0,
            oldest_backup: None,
            newest_backup: None,
        });
    }

    let mut total_backups = 0;
    let mut total_size_bytes = 0u64;
    let mut oldest: Option<String> = None;
    let mut newest: Option<String> = None;

    // Walk through all backup directories
    for entry in walkdir::WalkDir::new(&backup_base)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Count backup files (not metadata)
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.contains(".backup.") && !filename.ends_with(".json") {
                    total_backups += 1;

                    if let Ok(metadata) = fs::metadata(path) {
                        total_size_bytes += metadata.len();
                    }

                    // Track oldest and newest
                    if let Ok(meta) = load_metadata(path) {
                        if oldest.is_none() || Some(&meta.timestamp) < oldest.as_ref() {
                            oldest = Some(meta.timestamp.clone());
                        }
                        if newest.is_none() || Some(&meta.timestamp) > newest.as_ref() {
                            newest = Some(meta.timestamp);
                        }
                    }
                }
            }
        }
    }

    Ok(BackupStats {
        total_backups,
        total_size_bytes,
        oldest_backup: oldest,
        newest_backup: newest,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStats {
    pub total_backups: usize,
    pub total_size_bytes: u64,
    pub oldest_backup: Option<String>,
    pub newest_backup: Option<String>,
}

impl BackupStats {
    pub fn display(&self) {
        println!("{} Backup Statistics\n", "📊".bold());
        println!(
            "{} {}",
            "Total Backups:".cyan(),
            self.total_backups.to_string().bright_white()
        );
        println!(
            "{} {}",
            "Total Size:".cyan(),
            format_bytes(self.total_size_bytes).bright_white()
        );

        if let Some(ref oldest) = self.oldest_backup {
            println!("{} {}", "Oldest Backup:".cyan(), oldest.bright_white());
        }

        if let Some(ref newest) = self.newest_backup {
            println!("{} {}", "Newest Backup:".cyan(), newest.bright_white());
        }

        let home = dirs::home_dir().unwrap_or_default();
        let backup_dir = home.join(BACKUP_DIR_NAME);
        println!(
            "\n{} {}",
            "Backup Directory:".cyan(),
            backup_dir.display().to_string().truecolor(150, 150, 150)
        );
    }
}

// Simple SHA-256 implementation
mod sha256 {
    pub struct Sha256 {
        state: [u32; 8],
        data: Vec<u8>,
        data_len: u64,
    }

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    impl Sha256 {
        pub fn new() -> Self {
            Sha256 {
                state: [
                    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                    0x1f83d9ab, 0x5be0cd19,
                ],
                data: Vec::new(),
                data_len: 0,
            }
        }

        pub fn update(&mut self, input: &[u8]) {
            self.data.extend_from_slice(input);
            self.data_len += input.len() as u64;

            while self.data.len() >= 64 {
                let block: [u8; 64] = self
                    .data
                    .drain(..64)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap();
                self.process_block(&block);
            }
        }

        pub fn finish(mut self) -> String {
            let bit_len = self.data_len * 8;
            self.data.push(0x80);

            while (self.data.len() + 8) % 64 != 0 {
                self.data.push(0x00);
            }

            self.data.extend_from_slice(&bit_len.to_be_bytes());

            while !self.data.is_empty() {
                let block: [u8; 64] = self
                    .data
                    .drain(..64)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap();
                self.process_block(&block);
            }

            self.state.iter().map(|&x| format!("{:08x}", x)).collect()
        }

        fn process_block(&mut self, block: &[u8; 64]) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([
                    block[i * 4],
                    block[i * 4 + 1],
                    block[i * 4 + 2],
                    block[i * 4 + 3],
                ]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }

            let mut a = self.state[0];
            let mut b = self.state[1];
            let mut c = self.state[2];
            let mut d = self.state[3];
            let mut e = self.state[4];
            let mut f = self.state[5];
            let mut g = self.state[6];
            let mut h = self.state[7];

            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = h
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);

                h = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }

            self.state[0] = self.state[0].wrapping_add(a);
            self.state[1] = self.state[1].wrapping_add(b);
            self.state[2] = self.state[2].wrapping_add(c);
            self.state[3] = self.state[3].wrapping_add(d);
            self.state[4] = self.state[4].wrapping_add(e);
            self.state[5] = self.state[5].wrapping_add(f);
            self.state[6] = self.state[6].wrapping_add(g);
            self.state[7] = self.state[7].wrapping_add(h);
        }
    }
}

/// Backup health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupHealthCheck {
    pub total_backups: usize,
    pub healthy_backups: usize,
    pub corrupted_backups: Vec<String>,
    pub missing_metadata: Vec<String>,
    pub old_backups: Vec<BackupAge>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupAge {
    pub file_path: String,
    pub days_since_backup: i64,
    pub last_backup: String,
}

impl BackupHealthCheck {
    pub fn is_healthy(&self) -> bool {
        self.corrupted_backups.is_empty() && self.errors.is_empty() && self.healthy_backups > 0
    }

    pub fn display(&self) {
        println!("\n{} Backup Health Check Report\n", "🏥".bold());

        if self.is_healthy() {
            println!("{} All backups are healthy!", "✓".green().bold());
        } else {
            println!("{} Issues detected!", "⚠️".yellow().bold());
        }

        println!("\n{}", "Summary:".cyan().bold());
        println!("  {} {}", "Total Backups:".cyan(), self.total_backups);
        println!("  {} {}", "Healthy:".green(), self.healthy_backups);
        println!("  {} {}", "Corrupted:".red(), self.corrupted_backups.len());
        println!(
            "  {} {}",
            "Missing Metadata:".yellow(),
            self.missing_metadata.len()
        );

        if !self.corrupted_backups.is_empty() {
            println!("\n{}", "❌ Corrupted Backups:".red().bold());
            for backup in &self.corrupted_backups {
                println!("  - {}", backup.red());
            }
        }

        if !self.missing_metadata.is_empty() {
            println!("\n{}", "⚠️  Missing Metadata:".yellow().bold());
            for backup in &self.missing_metadata {
                println!("  - {}", backup);
            }
        }

        if !self.old_backups.is_empty() {
            println!("\n{}", "📅 Stale Backups (>30 days):".blue().bold());
            for age in &self.old_backups {
                println!(
                    "  {} {} ({} days old)",
                    "•".blue(),
                    age.file_path.bright_white(),
                    age.days_since_backup.to_string().yellow()
                );
            }
        }

        if !self.warnings.is_empty() {
            println!("\n{}", "⚠️  Warnings:".yellow().bold());
            for warning in &self.warnings {
                println!("  - {}", warning);
            }
        }

        if !self.errors.is_empty() {
            println!("\n{}", "❌ Errors:".red().bold());
            for error in &self.errors {
                println!("  - {}", error.red());
            }
        }
    }
}

/// Run comprehensive health check on all backups
pub fn run_health_check() -> Result<BackupHealthCheck> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    let backup_base = home.join(BACKUP_DIR_NAME);

    let mut health = BackupHealthCheck {
        total_backups: 0,
        healthy_backups: 0,
        corrupted_backups: Vec::new(),
        missing_metadata: Vec::new(),
        old_backups: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    if !backup_base.exists() {
        health
            .warnings
            .push("No backup directory found. No backups have been created yet.".to_string());
        return Ok(health);
    }

    // Walk through all backups
    for entry in walkdir::WalkDir::new(&backup_base)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Only check backup files (not metadata)
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.contains(".backup.") && !filename.ends_with(".json") {
                    health.total_backups += 1;

                    // Check for metadata
                    let metadata_path = path.with_extension("backup.json");
                    if !metadata_path.exists() {
                        health.missing_metadata.push(path.display().to_string());
                        continue;
                    }

                    // Load and verify metadata
                    match load_metadata(path) {
                        Ok(metadata) => {
                            // Verify checksum
                            match calculate_checksum(path) {
                                Ok(current_checksum) => {
                                    if current_checksum == metadata.checksum {
                                        health.healthy_backups += 1;

                                        // Check age
                                        if let Ok(age_days) =
                                            calculate_backup_age(&metadata.timestamp)
                                        {
                                            if age_days > 30 {
                                                health.old_backups.push(BackupAge {
                                                    file_path: metadata.original_path.clone(),
                                                    days_since_backup: age_days,
                                                    last_backup: metadata.timestamp.clone(),
                                                });
                                            }
                                        }
                                    } else {
                                        health.corrupted_backups.push(path.display().to_string());
                                        error!("Corrupted backup detected: {}", path.display());
                                    }
                                }
                                Err(e) => {
                                    health.errors.push(format!(
                                        "Failed to verify {}: {}",
                                        path.display(),
                                        e
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            health.errors.push(format!(
                                "Failed to load metadata for {}: {}",
                                path.display(),
                                e
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(health)
}

fn calculate_backup_age(timestamp: &str) -> Result<i64> {
    // Parse timestamp format: YYYYMMDD_HHMMSS
    let date_str = &timestamp[..8];
    let year: i32 = date_str[0..4].parse()?;
    let month: u32 = date_str[4..6].parse()?;
    let day: u32 = date_str[6..8].parse()?;

    let backup_date =
        chrono::NaiveDate::from_ymd_opt(year, month, day).context("Invalid date in timestamp")?;
    let backup_datetime = backup_date
        .and_hms_opt(0, 0, 0)
        .context("Failed to create datetime")?;

    let now = Utc::now().naive_utc();
    let duration = now.signed_duration_since(backup_datetime);

    Ok(duration.num_days())
}

/// Restoration drill - verify all backups can be restored
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorationDrill {
    pub total_tested: usize,
    pub successful: usize,
    pub failed: Vec<DrillFailure>,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillFailure {
    pub backup_path: String,
    pub original_path: String,
    pub error: String,
}

impl RestorationDrill {
    pub fn display(&self) {
        println!("\n{} Backup Restoration Drill Report\n", "🎯".bold());

        let success_rate = if self.total_tested > 0 {
            (self.successful as f64 / self.total_tested as f64) * 100.0
        } else {
            0.0
        };

        println!("{}", "Summary:".cyan().bold());
        println!("  {} {}", "Backups Tested:".cyan(), self.total_tested);
        println!("  {} {}", "Successful:".green(), self.successful);
        println!("  {} {}", "Failed:".red(), self.failed.len());
        println!("  {} {:.1}%", "Success Rate:".cyan(), success_rate);
        println!("  {} {} ms", "Duration:".cyan(), self.duration_ms);

        if success_rate == 100.0 {
            println!(
                "\n{} All backups verified successfully!",
                "✓".green().bold()
            );
        } else if success_rate >= 90.0 {
            println!(
                "\n{} Most backups verified, some issues found",
                "⚠️".yellow().bold()
            );
        } else {
            println!(
                "\n{} Critical: Many backups cannot be restored!",
                "❌".red().bold()
            );
        }

        if !self.failed.is_empty() {
            println!("\n{}", "❌ Failed Restorations:".red().bold());
            for failure in &self.failed {
                println!("\n  {} {}", "Backup:".cyan(), failure.backup_path);
                println!("  {} {}", "Original:".cyan(), failure.original_path);
                println!("  {} {}", "Error:".red(), failure.error);
            }
        }

        println!("\n{}", "─".repeat(80).bright_black());
        if self.failed.is_empty() {
            println!(
                "{} All critical files can be safely restored from backup",
                "✓".green().bold()
            );
        } else {
            println!(
                "{} Action required: Fix failed backups before disaster strikes!",
                "⚠️".yellow().bold()
            );
        }
    }
}

/// Run a restoration drill - verify backups without actually modifying files
pub fn run_restoration_drill() -> Result<RestorationDrill> {
    use std::time::Instant;

    let start = Instant::now();

    let home = dirs::home_dir().context("Failed to get home directory")?;
    let backup_base = home.join(BACKUP_DIR_NAME);

    let mut drill = RestorationDrill {
        total_tested: 0,
        successful: 0,
        failed: Vec::new(),
        duration_ms: 0,
    };

    if !backup_base.exists() {
        return Ok(drill);
    }

    info!("Starting restoration drill...");

    // Walk through all backups
    for entry in walkdir::WalkDir::new(&backup_base)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.contains(".backup.") && !filename.ends_with(".json") {
                    drill.total_tested += 1;

                    // Try to load metadata
                    match load_metadata(path) {
                        Ok(metadata) => {
                            // Verify backup integrity
                            match calculate_checksum(path) {
                                Ok(backup_checksum) => {
                                    if backup_checksum == metadata.checksum {
                                        // Verify original file (if exists)
                                        let original = Path::new(&metadata.original_path);
                                        if original.exists() {
                                            match verify_backup(path, original) {
                                                Ok(_) => {
                                                    drill.successful += 1;
                                                    debug!(
                                                        "✓ Verified: {}",
                                                        metadata.original_path
                                                    );
                                                }
                                                Err(e) => {
                                                    // Original has changed - this is OK, just note it
                                                    drill.successful += 1;
                                                    debug!(
                                                        "Original file modified: {} ({})",
                                                        metadata.original_path, e
                                                    );
                                                }
                                            }
                                        } else {
                                            // Original doesn't exist - backup can still be restored
                                            drill.successful += 1;
                                            debug!(
                                                "✓ Backup valid (original file missing): {}",
                                                metadata.original_path
                                            );
                                        }
                                    } else {
                                        drill.failed.push(DrillFailure {
                                            backup_path: path.display().to_string(),
                                            original_path: metadata.original_path.clone(),
                                            error: "Checksum mismatch - backup is corrupted"
                                                .to_string(),
                                        });
                                    }
                                }
                                Err(e) => {
                                    drill.failed.push(DrillFailure {
                                        backup_path: path.display().to_string(),
                                        original_path: metadata.original_path.clone(),
                                        error: format!("Failed to calculate checksum: {}", e),
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            drill.failed.push(DrillFailure {
                                backup_path: path.display().to_string(),
                                original_path: "unknown".to_string(),
                                error: format!("Failed to load metadata: {}", e),
                            });
                        }
                    }
                }
            }
        }
    }

    drill.duration_ms = start.elapsed().as_millis();

    info!(
        "Restoration drill completed: {}/{} successful",
        drill.successful, drill.total_tested
    );

    Ok(drill)
}

/// Backup monitoring event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEvent {
    pub timestamp: String,
    pub event_type: BackupEventType,
    pub file_path: String,
    pub details: String,
    pub severity: EventSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupEventType {
    BackupCreated,
    BackupRestored,
    BackupCorrupted,
    BackupFailed,
    HealthCheckPassed,
    HealthCheckFailed,
    DrillPassed,
    DrillFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventSeverity {
    Info,
    Warning,
    Critical,
}

impl BackupEvent {
    pub fn log_to_file(&self) -> Result<()> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        let log_dir = home.join(".catdog");
        fs::create_dir_all(&log_dir)?;

        let log_file = log_dir.join("backup_events.log");
        let json = serde_json::to_string(self)?;

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        writeln!(file, "{}", json)?;

        Ok(())
    }

    pub fn should_alert(&self) -> bool {
        matches!(
            self.severity,
            EventSeverity::Warning | EventSeverity::Critical
        )
    }
}

/// Create backup event and optionally send alerts
pub fn emit_backup_event(
    event_type: BackupEventType,
    file_path: &str,
    details: &str,
    severity: EventSeverity,
) -> Result<()> {
    let event = BackupEvent {
        timestamp: Utc::now().to_rfc3339(),
        event_type,
        file_path: file_path.to_string(),
        details: details.to_string(),
        severity,
    };

    // Log to file
    event.log_to_file()?;

    // Log to console based on severity
    match event.severity {
        EventSeverity::Info => info!("Backup event: {}", details),
        EventSeverity::Warning => warn!("Backup warning: {}", details),
        EventSeverity::Critical => error!("Backup critical: {}", details),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_create_and_verify_backup() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test content").unwrap();
        temp_file.flush().unwrap();

        let path = temp_file.path().to_str().unwrap();

        let metadata = create_backup(path, BackupReason::Manual, false).unwrap();

        assert_eq!(metadata.original_path, path);
        assert!(Path::new(&metadata.backup_path).exists());
        assert_eq!(metadata.size_bytes, 12);
    }

    #[test]
    fn test_list_backups() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test content").unwrap();
        temp_file.flush().unwrap();

        let path = temp_file.path().to_str().unwrap();

        create_backup(path, BackupReason::Manual, false).unwrap();

        let backups = list_backups(path).unwrap();
        assert!(!backups.is_empty());
    }

    #[test]
    fn test_checksum_calculation() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        let checksum1 = calculate_checksum(temp_file.path()).unwrap();
        let checksum2 = calculate_checksum(temp_file.path()).unwrap();

        assert_eq!(checksum1, checksum2);
        assert!(!checksum1.is_empty());
    }

    #[test]
    fn test_health_check() {
        let health = run_health_check().unwrap();
        // Should not panic, even with no backups
        assert!(health.total_backups >= 0);
    }

    #[test]
    fn test_restoration_drill() {
        let drill = run_restoration_drill().unwrap();
        // Should not panic, even with no backups
        assert!(drill.total_tested >= 0);
    }
}
