# prune-backup

A CLI tool for managing backup file rotation. Scans a directory and applies retention policies based on file creation dates, moving old files to a `.trash` subdirectory.

## Installation

```bash
cargo install --path .
```

## Usage

```bash
prune-backup <DIRECTORY> [OPTIONS]
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--keep-last <N>` | 5 | Keep the last N backups |
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

# Preview what would be deleted
prune-backup /path/to/backups --dry-run
```

## How It Works

1. Scans all non-hidden files in the target directory
2. Reads file creation timestamps (falls back to modification time if creation time unavailable)
3. Applies retention policies in order (each policy only considers files not already retained):
   - **keep-last**: Keeps the N most recent files
   - **keep-hourly**: Keeps the latest file from each hour
   - **keep-daily**: Keeps the latest file from each day
   - **keep-weekly**: Keeps the latest file from each ISO week (Monday-Sunday)
   - **keep-monthly**: Keeps the latest file from each month
   - **keep-yearly**: Keeps the latest file from each year
4. Moves files not matching any policy to `.trash/` subdirectory

Files are never permanently deleted—they're moved to `.trash` for manual review.

## License

MIT
