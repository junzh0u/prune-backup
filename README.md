# prune-backup

A CLI tool for managing backup file rotation. Scans a directory and applies retention policies based on file timestamps, moving old files to the system trash.

## Installation

```bash
# From crates.io
cargo install prune-backup

# From source
cargo install --path .
```

## Usage

```bash
prune-backup <DIRECTORY> [OPTIONS]
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--keep-last <N>` | 5 | Keep the last N backups (must be at least 1) |
| `--keep-hourly <N>` | 24 | Keep the latest backup per hour for the last N hours with backups |
| `--keep-daily <N>` | 7 | Keep the latest backup per day for the last N days with backups |
| `--keep-weekly <N>` | 4 | Keep the latest backup per week for the last N weeks with backups (ISO week system) |
| `--keep-monthly <N>` | 12 | Keep the latest backup per month for the last N months with backups |
| `--keep-yearly <N>` | 10 | Keep the latest backup per year for the last N years with backups |
| `--dry-run` | - | Show what would be moved without actually moving files |
| `--trash-cmd <CMD>` | - | Use a custom command to trash files instead of the system trash |

### Examples

```bash
# Use default retention policies
prune-backup /path/to/backups

# Keep only the last 10 backups and 30 days of daily backups
prune-backup /path/to/backups --keep-last 10 --keep-daily 30

# Preview what would be pruned
prune-backup /path/to/backups --dry-run

# Use a custom trash command (permanently delete files)
prune-backup /path/to/backups --trash-cmd "rm"

# Use trash-cli on Linux
prune-backup /path/to/backups --trash-cmd "trash-put"

# Move files to a custom directory using {} placeholder
prune-backup /path/to/backups --trash-cmd "mv {} /path/to/trash/"
```

## Configuration File

You can place a `.retention` file in the target directory to set default retention policies. The file uses TOML format:

```toml
keep-last = 10
keep-hourly = 48
keep-daily = 14
keep-weekly = 8
keep-monthly = 24
keep-yearly = 5
```

Configuration priority (highest to lowest):
1. Command-line arguments
2. `.retention` file values
3. Built-in defaults

## How It Works

Follows [Proxmox-style retention semantics](https://pve.proxmox.com/wiki/Backup_and_Restore#vzdump_retention):

1. Scans all non-hidden files in the target directory
2. Reads file modification timestamps (falls back to creation time if unavailable)
3. Applies retention policies sequentially — each policy only considers files not already kept by a previous policy:
   - **keep-last**: Keeps the N most recent files
   - **keep-hourly**: Keeps the latest un-kept file per hour for the last N hours with backups
   - **keep-daily**: Keeps the latest un-kept file per day for the last N days with backups
   - **keep-weekly**: Keeps the latest un-kept file per ISO week (Monday-Sunday) for the last N weeks with backups
   - **keep-monthly**: Keeps the latest un-kept file per month for the last N months with backups
   - **keep-yearly**: Keeps the latest un-kept file per year for the last N years with backups
4. Logs which policy kept each file (e.g., `Keeping: backup.tar.gz (daily)`)
5. Moves files not matching any policy to the system trash (or uses a custom command if `--trash-cmd` is specified)

### Custom Trash Command

The `--trash-cmd` option allows you to specify a custom command for handling files to be removed. The file path is passed to the command in one of two ways:

- **With `{}` placeholder**: If your command contains `{}`, it will be replaced with the (shell-escaped) file path. Example: `--trash-cmd "mv {} /backup/trash/"`
- **Without placeholder**: If no `{}` is present, the file path is appended to the end of the command. Example: `--trash-cmd "rm"` becomes `rm /path/to/file`

Files are never permanently deleted by default—they're moved to your system's trash/recycle bin for manual review or recovery.

## License

MIT
