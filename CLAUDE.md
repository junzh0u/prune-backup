# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`prune-backup` is a Rust CLI tool that manages backup file rotation based on creation dates. It scans a directory and applies retention policies (keep-last, hourly, daily, weekly, monthly, yearly), moving files that don't match any policy to a `.trash` subdirectory.

## Build and Test Commands

```bash
# Run all checks (fmt, clippy, test)
make all

# Individual commands
make fmt      # Check formatting
make clippy   # Lint with pedantic warnings
make test     # Run tests
make install  # Install binary

# Run a specific test
cargo test test_name

# Run with example
cargo run -- /path/to/backups --keep-last 5 --keep-daily 7

# Dry run (no files moved)
cargo run -- /path/to/backups --dry-run
```

## Architecture

**Two-crate structure:**
- `src/main.rs` - CLI argument parsing with clap, converts args to `RetentionConfig`, calls library
- `src/lib.rs` - Core logic: file scanning, retention selection, trash operations

**Key types:**
- `FileInfo` - File path + creation timestamp
- `RetentionConfig` - All retention policy values (keep_last, keep_hourly, etc.)

**Core algorithm in `select_files_to_keep_with_datetime()`:**
Retention policies are applied independently to all files. A file can be kept by multiple policies, and logs show all matching policies:
1. keep-last (absolute count of newest files)
2. keep-hourly (oldest file per hour within N hours)
3. keep-daily (oldest file per day within N days)
4. keep-weekly (oldest file per ISO week within N weeks)
5. keep-monthly (oldest file per month within N months)
6. keep-yearly (oldest file per year within N years)

Files are sorted newest-first, then iterated in reverse (oldest-first) to select the oldest file per time period.

**Testing approach:**
- Unit tests in `src/lib.rs` use mock `FileInfo` with controlled timestamps
- Integration tests in `tests/integration.rs` use `tempfile` + `filetime` to create real files with specific modification times
- The `_with_datetime` variant of selection allows injecting a fixed "now" for deterministic tests
