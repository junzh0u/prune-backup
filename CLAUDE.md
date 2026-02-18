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

**Core algorithm in `select_files_to_keep_with_reasons()`:**
Retention policies are applied independently to all files. A file can be kept by multiple policies, and logs show all matching policies:
1. keep-last (absolute count of newest files)
2. keep-hourly (oldest file per hour for N most recent hours that have files)
3. keep-daily (oldest file per day for N most recent days that have files)
4. keep-weekly (oldest file per ISO week for N most recent weeks that have files)
5. keep-monthly (oldest file per month for N most recent months that have files)
6. keep-yearly (oldest file per year for N most recent years that have files)

Time-based policies count only periods that actually have backup files — gaps without backups do not consume slots. For each policy, the N most recent unique periods are found (first pass, newest-first), then the oldest file in each period is selected (second pass, oldest-first).

**Testing approach:**
- Unit tests in `src/lib.rs` use mock `FileInfo` with controlled timestamps
- Integration tests in `tests/integration.rs` use `tempfile` + `filetime` to create real files with specific modification times
