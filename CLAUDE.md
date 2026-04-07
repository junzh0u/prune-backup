# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`prune-backup` is a Rust CLI tool that manages backup file rotation based on creation dates. It scans a directory and applies retention policies (keep-last, hourly, daily, weekly, monthly, yearly), moving files that don't match any policy to a `.trash` subdirectory.

## Build and Test Commands

```bash
# Run all checks (fmt, clippy, test)
just check

# Individual commands
just fmt      # Check formatting
just clippy   # Lint with pedantic warnings
just test     # Run tests
just install  # Install binary
just fix      # Auto-fix formatting and clippy warnings

# Run a specific test
cargo test test_name

# Run with example
cargo run -- /path/to/backups --keep-last 5 --keep-daily 7

# Dry run (no files moved)
cargo run -- /path/to/backups --dry-run

# Deploy to remote Linux host
just deploy-linux <host> [path]
```

## Architecture

**Two-crate structure:**
- `src/main.rs` - CLI argument parsing with clap, converts args to `RetentionConfig`, calls library
- `src/lib.rs` - Core logic: file scanning, retention selection, trash operations

**Key types:**
- `FileInfo` - File path + creation timestamp
- `RetentionConfig` - All retention policy values (keep_last, keep_hourly, etc.)

**Core algorithm in `select_files_to_keep_with_reasons()`:**
Follows [Proxmox-style retention semantics](https://pve.proxmox.com/wiki/Backup_and_Restore#vzdump_retention).
Policies are applied sequentially — each policy only considers files not already kept by a previous policy. Each file gets exactly one retention reason:
1. keep-last (absolute count of newest files)
2. keep-hourly (newest un-kept file per hour for N most recent hours with un-kept files)
3. keep-daily (newest un-kept file per day for N most recent days with un-kept files)
4. keep-weekly (newest un-kept file per ISO week for N most recent weeks with un-kept files)
5. keep-monthly (newest un-kept file per month for N most recent months with un-kept files)
6. keep-yearly (newest un-kept file per year for N most recent years with un-kept files)

Time-based policies count only periods that have un-kept backup files — gaps without backups do not consume slots.

**Testing approach:**
- Unit tests in `src/lib.rs` use mock `FileInfo` with controlled timestamps
- Integration tests in `tests/integration.rs` use `tempfile` + `filetime` to create real files with specific modification times
