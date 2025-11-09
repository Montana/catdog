# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Configuration File Support**: TOML-based configuration at `~/.config/catdog/config.toml`
  - Configurable alert channels (console, slack, webhook)
  - Adjustable monitoring intervals and disk usage thresholds
  - Auto-creates default config on first run
  - Full serialization/deserialization with validation
  - Unit tests for config loading and parsing

- **Diff Command**: Compare fstab files with colored diff output
  - `catdog diff <file1> <file2>` - Compare any two files
  - `catdog diff --current <file>` - Compare with `/etc/fstab`
  - Color-coded additions (green) and deletions (red)
  - Line numbers for easy navigation
  - Summary statistics showing total additions/deletions
  - Built on `similar` crate for robust diff algorithm

- **User-Friendly Error Handling**: Intelligent error detection and helpful suggestions
  - Permission denied errors suggest using sudo
  - File not found errors provide context-specific help
  - Missing command errors suggest installation steps
  - Configuration errors show config file location
  - Proper Unix exit codes (2, 13, 65, 78, 127, etc.)
  - No more raw stack traces shown to users

- **Comprehensive Test Suite**:
  - Config module tests (3 tests)
  - Diff module tests (4 tests)
  - Error handling tests (5 tests)
  - Existing fstab parsing tests (4 tests)
  - Total: 16 passing tests

### Fixed

- **Dry-Run Mode**: `--dry-run` flag now actually works
  - `backup` command shows what would be backed up without creating files
  - `generate` command previews output without writing to disk
  - Clear `[DRY-RUN]` indicators in all output
  - Dry-run notice displayed at start of command execution

### Changed

- **Error Flow Architecture**: Complete rewrite of error handling
  - Main function now uses proper Result<()> pattern
  - Errors converted to user-friendly format before display
  - Separate error module with dedicated UserError type
  - Better separation of concerns

- **Module Organization**: Added new modules for better structure
  - `config.rs` - Configuration management
  - `error.rs` - User-friendly error handling
  - `diff.rs` - File comparison functionality

- **Help Text**: Updated with new commands and improved formatting
  - Added diff command documentation
  - Added example usage for diff command
  - Clearer flag descriptions

### Technical Improvements

- Fixed unused import warnings (removed unused `UserError` import)
- Fixed lifetime annotations in diff module for proper Rust semantics
- Added dependency on `toml` (0.8) for configuration
- Added dependency on `similar` (2.3) for diff functionality
- Added dependency on `dirs` (5.0) for cross-platform config directory detection
- Improved code formatting with `cargo fmt`
- All compiler warnings addressed

### Dependencies Added

```toml
toml = "0.8"
similar = "2.3"
dirs = "5.0"
```

### Breaking Changes

None - all changes are backward compatible.

### Migration Guide

No migration needed. The tool will auto-create a default config file on first run at:
- Linux: `~/.config/catdog/config.toml`
- macOS: `~/Library/Application Support/catdog/config.toml`

You can customize the config file to adjust:
- Alert notification channels
- Monitoring check intervals
- Disk usage warning/critical thresholds
- Slack webhook URLs
- Generic webhook URLs

## [0.1.0] - Previous Release

### Features

- Parse and display `/etc/fstab` files
- Discover block devices on Linux and macOS
- Generate fstab entries with smart defaults
- Suggest mount options optimized for hardware
- Alert system (bark) for filesystem monitoring
- Continuous health monitoring
- Corpus management for fstab configurations
- JSON output support
- Colored terminal output
- Backup functionality
- Validation of fstab entries

---

## Future Enhancements

Potential improvements for future versions:

- [ ] Switch to `clap` for better CLI argument parsing
- [ ] Shell completion scripts (bash, zsh, fish)
- [ ] Man page generation
- [ ] Interactive mode for mount suggestions
- [ ] Mount/unmount integration with sudo
- [ ] Systemd service integration
- [ ] Export/import in multiple formats (YAML, JSON, TOML)
- [ ] Snapshot management for fstab configurations
- [ ] Performance benchmarking tools
- [ ] Extended platform support (BSD, Windows WSL)
