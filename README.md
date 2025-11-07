# catdog 

<img width="625" height="625" alt="Quant Point (7)" src="https://github.com/user-attachments/assets/838ee246-c394-493b-8024-f0de0e7e556c" />

An fstab utility that combines the best of both worlds:
- **cat** - Shows the raw `/etc/fstab` file (just like the `cat` command)
- **dog** - Fetches and parses `/etc/fstab` into a nice formatted table.

## Installation

```bash
cargo build --release
sudo cp target/release/catdog /usr/local/bin/
```

## Usage

### Show raw fstab (cat mode)
```bash
catdog cat
```

## Parse `lsblk -J` types correctly (they’re not strings)

On many distros rm/rota are numbers (0/1), not strings; mountpoint is string or null, for example: 

```rust
fn parse_linux_device(device: &serde_json::Value, devices: &mut Vec<BlockDevice>) {
    let name = device["name"].as_str().unwrap_or("");
    let device_path = if name.starts_with("/dev/") { name.to_string() } else { format!("/dev/{name}") };

    let rm = device["rm"].as_u64().unwrap_or(0) == 1;
    let rota_is_hdd = device["rota"].as_u64().unwrap_or(1) == 1;

    let block_device = BlockDevice {
        device: device_path,
        uuid: device["uuid"].as_str().map(String::from),
        partuuid: device["partuuid"].as_str().map(String::from),
        label: device["label"].as_str().map(String::from),
        fs_type: device["fstype"].as_str().map(String::from),
        size: device["size"].as_str().map(String::from),
        mount_point: device["mountpoint"].as_str().map(String::from), // may be None
        is_removable: rm,
        is_ssd: !rota_is_hdd,
    };

    if block_device.fs_type.is_some() {
        devices.push(block_device);
    }

    if let Some(children) = device["children"].as_array() {
        for child in children {
            parse_linux_device(child, devices);
        }
    }
}
```

Tip: some newer `util-linux` builds expose mountpoints (array). If you encounter that, prefer the first non-null entry.

### Parse and display nicely (dog mode)
```bash
catdog dog
```

### List all mount points
```bash
catdog list
```

### Find specific entries
```bash
catdog find /dev/sda1
catdog find /home
```

### Validate fstab for common issues
```bash
catdog validate
```

## Why "catdog"?

- **cat** = simple, straightforward display of the file
- **dog** = fetches the information and brings it back in a friendly, formatted way

<img width="1024" height="1024" alt="ChatGPT Image Nov 7, 2025, 08_37_07 AM" src="https://github.com/user-attachments/assets/d19f5c41-6a13-4326-a4f9-468fa0a4f631" />

Just like the difference between a cat (independent, shows you things as-is) and a dog (eager to fetch and present things nicely)!

## Commands

| Command | Description |
|---------|-------------|
| `cat` | Display raw `/etc/fstab` file |
| `dog` | Parse and display fstab in a formatted table |
| `list` | List all mount points |
| `find <term>` | Find entries matching device or mount point |
| `validate` | Check fstab for common configuration issues |
| `help` | Show help message |

## Author

Michael Mendy (c) 2025 