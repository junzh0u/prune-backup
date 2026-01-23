use anyhow::Result;
use clap::Parser;
use prune_backup::{rotate_files, RetentionConfig};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "prune-backup")]
#[command(about = "Prune backup files based on creation date, keeping recent files and moving old ones to trash")]
struct Args {
    /// Directory to scan for files
    directory: PathBuf,

    /// Keep the last N backups
    #[arg(long = "keep-last", default_value = "5")]
    keep_last: usize,

    /// Keep backups for the last N hours (1 per hour)
    #[arg(long = "keep-hourly", default_value = "24")]
    keep_hourly: u32,

    /// Keep backups for the last N days (1 per day)
    #[arg(long = "keep-daily", default_value = "7")]
    keep_daily: u32,

    /// Keep backups for the last N weeks (1 per week, ISO week system)
    #[arg(long = "keep-weekly", default_value = "4")]
    keep_weekly: u32,

    /// Keep backups for the last N months (1 per month)
    #[arg(long = "keep-monthly", default_value = "12")]
    keep_monthly: u32,

    /// Keep backups for the last N years (1 per year)
    #[arg(long = "keep-yearly", default_value = "10")]
    keep_yearly: u32,

    /// Dry run - show what would be moved without actually moving
    #[arg(long)]
    dry_run: bool,
}

impl From<&Args> for RetentionConfig {
    fn from(args: &Args) -> Self {
        Self {
            keep_last: args.keep_last,
            keep_hourly: args.keep_hourly,
            keep_daily: args.keep_daily,
            keep_weekly: args.keep_weekly,
            keep_monthly: args.keep_monthly,
            keep_yearly: args.keep_yearly,
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate directory exists
    if !args.directory.is_dir() {
        anyhow::bail!("Directory does not exist: {:?}", args.directory);
    }

    let config = RetentionConfig::from(&args);
    println!("Scanning {:?}...", args.directory);

    let (kept, moved) = rotate_files(&args.directory, &config, args.dry_run)?;

    if args.dry_run {
        println!("Dry run complete. Would keep {} files, move {} to trash.", kept, moved);
    } else {
        println!("Done. Kept {} files, moved {} to trash.", kept, moved);
    }

    Ok(())
}
