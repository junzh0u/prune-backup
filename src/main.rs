use anyhow::Result;
use clap::Parser;
use prune_backup::{read_retention_file, resolve_config, rotate_files};
use std::path::PathBuf;

fn parse_keep_last(s: &str) -> Result<usize, String> {
    let val: usize = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid number"))?;
    if val == 0 {
        Err("keep-last must be at least 1".to_string())
    } else {
        Ok(val)
    }
}

#[derive(Parser, Debug)]
#[command(name = "prune-backup")]
#[command(
    about = "Prune backup files based on creation date, keeping recent files and moving old ones to trash"
)]
struct Args {
    /// Directory to scan for files
    directory: PathBuf,

    /// Keep the last N backups (must be at least 1)
    #[arg(long = "keep-last", value_parser = parse_keep_last)]
    keep_last: Option<usize>,

    /// Keep backups for the last N hours (1 per hour)
    #[arg(long = "keep-hourly")]
    keep_hourly: Option<u32>,

    /// Keep backups for the last N days (1 per day)
    #[arg(long = "keep-daily")]
    keep_daily: Option<u32>,

    /// Keep backups for the last N weeks (1 per week, ISO week system)
    #[arg(long = "keep-weekly")]
    keep_weekly: Option<u32>,

    /// Keep backups for the last N months (1 per month)
    #[arg(long = "keep-monthly")]
    keep_monthly: Option<u32>,

    /// Keep backups for the last N years (1 per year)
    #[arg(long = "keep-yearly")]
    keep_yearly: Option<u32>,

    /// Dry run - show what would be moved without actually moving
    #[arg(long)]
    dry_run: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate directory exists
    if !args.directory.is_dir() {
        anyhow::bail!("Directory does not exist: {}", args.directory.display());
    }

    // Read .retention file if it exists
    let file_config = read_retention_file(&args.directory)?;

    // Resolve final config: CLI args > file config > defaults
    let config = resolve_config(
        args.keep_last,
        args.keep_hourly,
        args.keep_daily,
        args.keep_weekly,
        args.keep_monthly,
        args.keep_yearly,
        file_config.as_ref(),
    );

    println!("Scanning {}...", args.directory.display());

    let (kept, moved) = rotate_files(&args.directory, &config, args.dry_run)?;

    if args.dry_run {
        println!("Dry run complete. Would keep {kept} files, move {moved} to trash.");
    } else {
        println!("Done. Kept {kept} files, moved {moved} to trash.");
    }

    Ok(())
}
