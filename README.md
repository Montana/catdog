[![Build Status](https://app.travis-ci.com/Montana/catdog.svg?token=U865GtC2ptqX3Ezf3Fzb&branch=master)](https://app.travis-ci.com/Montana/catdog)

# catdog

A command-line tool for managing `/etc/fstab`, discovering block devices, and monitoring filesystem health.

## Features

- **Parse and validate** `/etc/fstab` files
- **Discover** block devices on Linux and macOS
- **Generate** complete fstab files automatically with smart defaults
- **Suggest** mount options optimized for your hardware (SSD detection, filesystem-specific options)
- **Bark alerts** when disk usage is high or filesystem issues detected
- **Validate** fstab entries before you reboot
- **Build a library** of fstab configurations for searching and reference
- **Package management** - Unified interface for apt, dnf, yum, pacman, zypper, brew, and apk
- **Cross-platform** package operations with automatic package manager detection
- **Service management** - Control services across systemd, launchd, init.d, and OpenRC
- **System information** - Comprehensive hardware and OS details with JSON output

## Installation

```bash
# Build optimized release
cargo build --release

# Install to system
sudo cp target/release/catdog /usr/local/bin/

# Verify installation
catdog --version
```

## Professional Features

- **JSON Output**: Use `--json` flag for machine-readable output (perfect for scripts)
- **Colored Output**: Automatic color support with `--no-color` override
- **Dry Run Mode**: Preview changes with `--dry-run` before applying
- **Logging**: Built-in structured logging with env_logger
- **Backup Functionality**: Automatic timestamped backups before changes
- **Cross-Platform**: Works on Linux and macOS

## Quick Start

```bash
# Display /etc/fstab
catdog cat

# Parse and display nicely
catdog dog

# Discover all block devices
catdog discover

# Generate fstab from discovered devices
catdog generate fstab.new

# Validate your current fstab
catdog validate

# Get smart mount suggestions
catdog suggest

# Monitor disk usage (300 second intervals)
catdog monitor 300
```

## Commands

### Filesystem Management

| Command | Description |
|---------|-------------|
| `catdog cat` | Display raw `/etc/fstab` |
| `catdog dog` | Parse and display fstab in a nice table |
| `catdog list` | List all mount points |
| `catdog find <term>` | Find entries matching a device or mount point |
| `catdog validate` | Check fstab for common errors |
| `catdog discover` | List all block devices with details (supports `--json`) |
| `catdog suggest [device]` | Get smart mount suggestions for devices |
| `catdog generate [file]` | Generate complete fstab from discovered devices |
| `catdog backup [file]` | Create verified backup with SHA-256 checksum and metadata |
| `catdog restore <backup>` | Restore from backup (use --force to override safety checks) |
| `catdog list-backups <file>` | List all backups for a specific file |
| `catdog backup-stats` | Show backup statistics and disk usage |

### Bark (Monitoring & Alerts)

When catdog detects problems, it barks! 🐕

| Command | Description |
|---------|-------------|
| `catdog check` | Run filesystem health checks once |
| `catdog monitor [interval]` | Start continuous monitoring (default: 300s) |
| `catdog barks [status]` | List barks (filter: firing/acknowledged/resolved/silenced) |
| `catdog bark <id>` | Show detailed bark information |
| `catdog ack <id>` or `pet <id>` | Pet the dog (acknowledge bark) |
| `catdog resolve <id>` or `quiet <id>` | Quiet the dog (resolve bark) |
| `catdog silence <id>` or `hush <id>` | Hush the dog (silence bark) |

### Configuration Library

| Command | Description |
|---------|-------------|
| `catdog corpus ingest <file>` | Add an fstab file to your configuration library |
| `catdog corpus search <query>` | Search stored configurations by filesystem, device, or options |
| `catdog corpus stats` | Show statistics about stored configurations |

## Usage Examples

### Generate an fstab file

```bash
# Preview what would be generated
catdog generate

# Save to file
catdog generate fstab.new

# Review the file
cat fstab.new

# Test it (without rebooting)
sudo mount -a -f fstab.new
```

The generator will:
- Auto-detect SSDs and apply optimizations (`noatime`, `discard`)
- Use UUIDs for stable device identification
- Add `nofail` for removable devices
- Skip system-critical mounts (/, /boot)
- Include helpful comments

**Example output:**
```
# Device: /dev/sda2
# Label: Data Drive
# Size: 1.0 TB
# Type: SSD (optimized options applied)
UUID=abc-123-def    /mnt/data    ext4    defaults,noatime,discard    0 2
```

### Validate your fstab

```bash
catdog validate
```

Checks for:
- Invalid syntax
- Duplicate mount points
- Missing directories
- Incorrect dump/pass values
- Security issues (missing noexec on /tmp)

### Monitor disk usage (barks!)

```bash
# Check once
catdog check

# Monitor every 5 minutes (catdog will bark when issues are detected)
catdog monitor 300

# View all barks
catdog barks

# View only firing barks
catdog barks firing

# Show details of a specific bark
catdog bark <bark-id>

# Pet the dog to acknowledge (good dog!)
catdog pet <bark-id>

# Quiet the dog when problem is resolved
catdog quiet <bark-id>

# Hush overly chatty barks
catdog hush <bark-id>
```

### Build a configuration library

Store and search fstab configurations from multiple systems:

```bash
# Add configurations to your library
catdog corpus ingest /etc/fstab
catdog corpus ingest server1-fstab.txt
catdog corpus ingest server2-fstab.txt

# Search for specific filesystem types
catdog corpus search ext4
catdog corpus search btrfs

# Search for mount options
catdog corpus search noatime
catdog corpus search discard

# View statistics
catdog corpus stats
```

**Example stats output:**
```
📊 Configuration Library Statistics

Library Overview:
  Configurations: 3
  Total Entries: 12

Filesystem Types:
  • ext4 (7)
  • btrfs (3)
  • xfs (2)

Most Common Mount Options:
  • defaults (12)
  • noatime (8)
  • discard (5)
```

This is useful for:
- Learning from existing configurations
- Finding examples of how to mount specific filesystems
- Tracking mount options across multiple servers
- Building a knowledge base of working configurations

## Package Management

catdog provides a unified interface for managing packages across different Linux distributions and macOS. It automatically detects your system's package manager and translates commands accordingly.

### Supported Package Managers

- **apt** (Debian/Ubuntu)
- **dnf** (Fedora/RHEL 8+)
- **yum** (CentOS/RHEL 7)
- **pacman** (Arch Linux)
- **zypper** (openSUSE)
- **brew** (macOS)
- **apk** (Alpine Linux)

### Package Commands

| Command | Description |
|---------|-------------|
| `catdog pkg install <pkg...>` | Install one or more packages |
| `catdog pkg remove <pkg...>` | Remove one or more packages |
| `catdog pkg update` | Update package cache/repositories |
| `catdog pkg upgrade` | Upgrade all installed packages |
| `catdog pkg search <query>` | Search for packages |
| `catdog pkg list` | List all installed packages |
| `catdog pkg info <package>` | Check if a package is installed |

### Package Management Examples

```bash
# Install packages
catdog pkg install nginx
catdog pkg install docker vim git

# Test install with dry-run
catdog --dry-run pkg install postgresql

# Remove packages
catdog pkg remove nginx

# Update package cache
catdog pkg update

# Upgrade all packages
catdog pkg upgrade

# Search for packages
catdog pkg search python
catdog pkg search docker

# List all installed packages
catdog pkg list

# Get package list as JSON
catdog pkg list --json

# Check if a package is installed
catdog pkg info nginx
```

**Example output:**
```bash
$ catdog pkg search docker
🔍 Searching for packages matching: docker

✓ Found 12 package(s):

  • docker - Container runtime
  • docker-compose - Multi-container orchestration
  • docker-buildx - Docker CLI plugin for BuildKit
  ...
```

### Why use catdog for package management?

1. **Unified Interface**: Same commands work across all distros
2. **JSON Output**: Perfect for automation and scripts
3. **Dry-run Mode**: Test package operations safely
4. **Fast**: Written in Rust, single binary
5. **Simple**: No need to remember distro-specific commands

**Comparison with Ansible:**
- ✅ No Python dependencies required
- ✅ Single binary, instant execution
- ✅ Interactive and scriptable
- ✅ Works with existing package managers (no abstractions)
- ✅ Perfect for quick operations and testing

## Automation & Integration

### JSON Output for Scripts

Use the `--json` flag for machine-readable output:

```bash
# Get device information as JSON
catdog --json discover > devices.json

# Parse with jq
catdog --json discover | jq '.devices[] | select(.is_ssd == true)'

# Use in scripts
DEVICE_COUNT=$(catdog --json discover | jq '.count')
echo "Found $DEVICE_COUNT devices"
```

**Example JSON output:**
```json
{
  "count": 3,
  "devices": [
    {
      "device": "/dev/sda2",
      "uuid": "abc-123-def",
      "label": "Data",
      "filesystem": "ext4",
      "size": "1.0 TB",
      "mount_point": "/data",
      "is_ssd": true,
      "is_removable": false
    }
  ]
}
```

### Backup Before Changes

Always create backups before modifying system files:

```bash
# Backup current fstab
catdog backup /etc/fstab
# Output: Backup created: /etc/fstab.backup.20250108_120000

# Generate new fstab safely
catdog --dry-run generate fstab.new
catdog backup /etc/fstab
catdog generate fstab.new
```

### Environment Variables

- `NO_COLOR=1` - Disable colored output
- `RUST_LOG=debug` - Enable debug logging

```bash
# Run with debug logging
RUST_LOG=debug catdog discover

# Disable colors for logging
NO_COLOR=1 catdog validate
```

## Bark Configuration

Barks (alerts) are stored in `~/.catdog/alerts.json`.

The monitoring system barks when it detects:
- Disk usage (barks at 90% by default)
- Inode exhaustion
- Mount point accessibility issues

Configure bark behavior in `~/.catdog/config.toml` (optional):

```toml
[alerting]
disk_usage_threshold = 90  # Bark when disk is 90% full

[alerting.webhooks]
endpoint = "https://your-webhook-url.com"  # Send barks here
```

## Platform Support

- **Linux**: Uses `lsblk` for device discovery
- **macOS**: Uses `diskutil` for device discovery
- **BSD**: Limited support

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy
```

## Why "catdog"?

- `cat` displays raw files → `catdog cat` shows raw fstab
- Dogs fetch things → `catdog dog` fetches and parses fstab nicely

It seemed clever at 2 AM.

## Safety Warning

⚠️ **Always review generated fstab files before using them!**

1. Back up your current fstab: `sudo cp /etc/fstab /etc/fstab.backup`
2. Review the generated file carefully
3. Test with `sudo mount -a` before rebooting
4. Make sure mount point directories exist

A bad fstab can prevent your system from booting. Be careful.

## Configuration Library Storage

Configurations are stored in `~/.catdog/corpus/` as JSON files. Each file contains:
- Source file path
- Timestamp
- Parsed fstab entries (device, mount point, type, options)

You can safely delete this directory to clear your library.

## License

MIT OR Apache-2.0


PRs welcome! Please:
- Run `cargo fmt` and `cargo clippy`

---

**Author:** Michael Mendy © 2025
