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
| `--keep-hourly <N>` | 24 | Keep one backup per hour for the last N hours |
| `--keep-daily <N>` | 7 | Keep one backup per day for the last N days |
| `--keep-weekly <N>` | 4 | Keep one backup per week for the last N weeks (ISO week system) |
| `--keep-monthly <N>` | 12 | Keep one backup per month for the last N months |
| `--keep-yearly <N>` | 10 | Keep one backup per year for the last N years |
| `--dry-run` | - | Show what would be moved without actually moving files |

### Examples

```bash
# Use default retention policies
prune-backup /path/to/backups

# Keep only the last 10 backups and 30 days of daily backups
prune-backup /path/to/backups --keep-last 10 --keep-daily 30

# Preview what would be pruned
prune-backup /path/to/backups --dry-run
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

1. Scans all non-hidden files in the target directory
2. Reads file modification timestamps (falls back to creation time if unavailable)
3. Applies retention policies in cascading order (each policy only considers files not already retained):
   - **keep-last**: Keeps the N most recent files
   - **keep-hourly**: Keeps the oldest file from each hour
   - **keep-daily**: Keeps the oldest file from each day
   - **keep-weekly**: Keeps the oldest file from each ISO week (Monday-Sunday)
   - **keep-monthly**: Keeps the oldest file from each month
   - **keep-yearly**: Keeps the oldest file from each year
4. Moves files not matching any policy to the system trash

Files are never permanently deleted—they're moved to your system's trash/recycle bin for manual review or recovery.

## License

MIT
