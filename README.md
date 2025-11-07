# catdog 

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
