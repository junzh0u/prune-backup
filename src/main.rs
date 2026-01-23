use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};
use clap::Parser;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "rotate")]
#[command(about = "Rotate backup files based on creation date, keeping recent files and moving old ones to trash")]
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

#[derive(Debug, Clone)]
struct FileInfo {
    path: PathBuf,
    created: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq)]
struct RetentionConfig {
    keep_last: usize,
    keep_hourly: u32,
    keep_daily: u32,
    keep_weekly: u32,
    keep_monthly: u32,
    keep_yearly: u32,
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

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            keep_last: 5,
            keep_hourly: 24,
            keep_daily: 7,
            keep_weekly: 4,
            keep_monthly: 12,
            keep_yearly: 10,
        }
    }
}

fn get_file_creation_time(path: &Path) -> Result<DateTime<Local>> {
    let metadata = fs::metadata(path).context("Failed to read file metadata")?;
    let created = metadata
        .created()
        .or_else(|_| metadata.modified())
        .context("Failed to get file creation/modification time")?;
    Ok(DateTime::from(created))
}

fn scan_files(dir: &Path) -> Result<Vec<FileInfo>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(dir).context("Failed to read directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        // Skip directories and hidden files
        if path.is_dir() || path.file_name().map_or(false, |n| n.to_string_lossy().starts_with('.')) {
            continue;
        }

        match get_file_creation_time(&path) {
            Ok(created) => files.push(FileInfo { path, created }),
            Err(e) => eprintln!("Warning: Skipping {:?}: {}", path, e),
        }
    }

    // Sort by creation time, newest first
    files.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(files)
}

/// Returns (year, month, day, hour) as a unique key for the hour
fn get_hour_key(dt: DateTime<Local>) -> (i32, u32, u32, u32) {
    (dt.year(), dt.month(), dt.day(), dt.hour())
}

/// Returns (year, week) using ISO week system
fn get_week_key(date: NaiveDate) -> (i32, u32) {
    (date.iso_week().year(), date.iso_week().week())
}

fn get_month_key(date: NaiveDate) -> (i32, u32) {
    (date.year(), date.month())
}

fn get_year_key(date: NaiveDate) -> i32 {
    date.year()
}

fn select_files_to_keep_with_datetime(
    files: &[FileInfo],
    config: &RetentionConfig,
    now: DateTime<Local>,
) -> HashSet<usize> {
    let mut keep_indices: HashSet<usize> = HashSet::new();
    let today = now.date_naive();

    // 1. Keep last N files
    for i in 0..config.keep_last.min(files.len()) {
        keep_indices.insert(i);
    }

    // Track which periods we've already covered
    let mut covered_hours: HashSet<(i32, u32, u32, u32)> = HashSet::new();
    let mut covered_days: HashSet<NaiveDate> = HashSet::new();
    let mut covered_weeks: HashSet<(i32, u32)> = HashSet::new();
    let mut covered_months: HashSet<(i32, u32)> = HashSet::new();
    let mut covered_years: HashSet<i32> = HashSet::new();

    // Calculate boundaries
    let hour_boundary = now - chrono::Duration::hours(config.keep_hourly as i64);
    let day_boundary = today - chrono::Duration::days(config.keep_daily as i64);
    let week_boundary = today - chrono::Duration::weeks(config.keep_weekly as i64);
    let month_boundary = today - chrono::Duration::days(config.keep_monthly as i64 * 30);
    let year_boundary = today - chrono::Duration::days(config.keep_yearly as i64 * 365);

    // Iterate through files (already sorted newest first)
    for (i, file) in files.iter().enumerate() {
        let file_datetime = file.created;
        let file_date = file_datetime.date_naive();

        // 2. Keep 1 file per hour for N hours
        let hour_key = get_hour_key(file_datetime);
        if file_datetime >= hour_boundary && !covered_hours.contains(&hour_key) {
            covered_hours.insert(hour_key);
            keep_indices.insert(i);
        }

        // 3. Keep 1 file per day for N days
        if file_date >= day_boundary && !covered_days.contains(&file_date) {
            covered_days.insert(file_date);
            keep_indices.insert(i);
        }

        // 4. Keep 1 file per week for N weeks (ISO week system)
        let week_key = get_week_key(file_date);
        if file_date >= week_boundary && !covered_weeks.contains(&week_key) {
            covered_weeks.insert(week_key);
            keep_indices.insert(i);
        }

        // 5. Keep 1 file per month for N months
        let month_key = get_month_key(file_date);
        if file_date >= month_boundary && !covered_months.contains(&month_key) {
            covered_months.insert(month_key);
            keep_indices.insert(i);
        }

        // 6. Keep 1 file per year for N years
        let year_key = get_year_key(file_date);
        if file_date >= year_boundary && !covered_years.contains(&year_key) {
            covered_years.insert(year_key);
            keep_indices.insert(i);
        }
    }

    keep_indices
}

fn select_files_to_keep(files: &[FileInfo], config: &RetentionConfig) -> HashSet<usize> {
    let now = Local::now();
    select_files_to_keep_with_datetime(files, config, now)
}

fn move_to_trash(file: &Path, trash_dir: &Path, dry_run: bool) -> Result<()> {
    let file_name = file
        .file_name()
        .context("Failed to get file name")?;
    let dest = trash_dir.join(file_name);

    if dry_run {
        println!("Would move: {:?} -> {:?}", file, dest);
    } else {
        // Handle name conflicts by appending a number
        let mut final_dest = dest.clone();
        let mut counter = 1;
        while final_dest.exists() {
            let stem = dest.file_stem().unwrap_or_default().to_string_lossy();
            let ext = dest.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            final_dest = trash_dir.join(format!("{}_{}{}", stem, counter, ext));
            counter += 1;
        }
        fs::rename(file, &final_dest).context("Failed to move file to trash")?;
        println!("Moved: {:?} -> {:?}", file, final_dest);
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate directory exists
    if !args.directory.is_dir() {
        anyhow::bail!("Directory does not exist: {:?}", args.directory);
    }

    // Create trash directory
    let trash_dir = args.directory.join(".trash");
    if !args.dry_run && !trash_dir.exists() {
        fs::create_dir(&trash_dir).context("Failed to create .trash directory")?;
    }

    // Scan files
    let files = scan_files(&args.directory)?;
    println!("Found {} files in {:?}", files.len(), args.directory);

    if files.is_empty() {
        println!("No files to process.");
        return Ok(());
    }

    // Determine which files to keep
    let config = RetentionConfig::from(&args);
    let keep_indices = select_files_to_keep(&files, &config);
    println!(
        "Keeping {} files, moving {} to trash",
        keep_indices.len(),
        files.len() - keep_indices.len()
    );

    // Move files that are not in keep set
    let mut moved_count = 0;
    for (i, file) in files.iter().enumerate() {
        if !keep_indices.contains(&i) {
            move_to_trash(&file.path, &trash_dir, args.dry_run)?;
            moved_count += 1;
        }
    }

    if args.dry_run {
        println!("Dry run complete. {} files would be moved.", moved_count);
    } else {
        println!("Done. Moved {} files to {:?}", moved_count, trash_dir);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_file_info_with_time(name: &str, dt: DateTime<Local>) -> FileInfo {
        FileInfo {
            path: PathBuf::from(name),
            created: dt,
        }
    }

    fn make_file_info(name: &str, date: NaiveDate) -> FileInfo {
        let datetime = Local.from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap();
        FileInfo {
            path: PathBuf::from(name),
            created: datetime,
        }
    }

    fn zero_config() -> RetentionConfig {
        RetentionConfig {
            keep_last: 0,
            keep_hourly: 0,
            keep_daily: 0,
            keep_weekly: 0,
            keep_monthly: 0,
            keep_yearly: 0,
        }
    }

    #[test]
    fn test_default_config() {
        let config = RetentionConfig::default();
        assert_eq!(config.keep_last, 5);
        assert_eq!(config.keep_hourly, 24);
        assert_eq!(config.keep_daily, 7);
        assert_eq!(config.keep_weekly, 4);
        assert_eq!(config.keep_monthly, 12);
        assert_eq!(config.keep_yearly, 10);
    }

    #[test]
    fn test_get_hour_key() {
        let dt = Local.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        assert_eq!(get_hour_key(dt), (2024, 6, 15, 14));
    }

    #[test]
    fn test_get_week_key() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let (year, week) = get_week_key(date);
        assert_eq!(year, 2024);
        assert!(week >= 1 && week <= 53);
    }

    #[test]
    fn test_get_month_key() {
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        assert_eq!(get_month_key(date), (2024, 6));
    }

    #[test]
    fn test_get_year_key() {
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        assert_eq!(get_year_key(date), 2024);
    }

    #[test]
    fn test_keep_last_n_files() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_last: 3,
            ..zero_config()
        };

        // Create 5 files with different times
        let files: Vec<FileInfo> = (0..5)
            .map(|i| {
                let dt = now - chrono::Duration::minutes(i as i64);
                FileInfo {
                    path: PathBuf::from(format!("file{}.txt", i)),
                    created: dt,
                }
            })
            .collect();

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(keep.contains(&2));
    }

    #[test]
    fn test_keep_one_per_hour() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_hourly: 5,
            ..zero_config()
        };

        // Create files in different hours
        let files = vec![
            make_file_info_with_time("file1.txt", now),
            make_file_info_with_time("file2.txt", now - chrono::Duration::hours(1)),
            make_file_info_with_time("file3.txt", now - chrono::Duration::hours(2)),
            make_file_info_with_time("file4.txt", now - chrono::Duration::hours(2) + chrono::Duration::minutes(30)), // same hour as file3
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3); // 3 unique hours
        assert!(keep.contains(&0)); // hour 12
        assert!(keep.contains(&1)); // hour 11
        assert!(keep.contains(&2)); // hour 10 (first/latest one)
        assert!(!keep.contains(&3)); // same hour as file3, not kept
    }

    #[test]
    fn test_keep_one_per_day() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let today = now.date_naive();
        let config = RetentionConfig {
            keep_daily: 5,
            ..zero_config()
        };

        // Create files for different days
        let files = vec![
            make_file_info("file1.txt", today),
            make_file_info("file2.txt", today - chrono::Duration::days(1)),
            make_file_info("file3.txt", today - chrono::Duration::days(2)),
            make_file_info("file4.txt", today - chrono::Duration::days(2)), // duplicate day
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(keep.contains(&2));
        assert!(!keep.contains(&3)); // duplicate day
    }

    #[test]
    fn test_keep_one_per_week() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap(); // Saturday
        let today = now.date_naive();
        let config = RetentionConfig {
            keep_weekly: 4,
            ..zero_config()
        };

        // Create files spanning different weeks
        let files = vec![
            make_file_info("file1.txt", today),
            make_file_info("file2.txt", today - chrono::Duration::weeks(1)),
            make_file_info("file3.txt", today - chrono::Duration::weeks(2)),
            make_file_info("file4.txt", today - chrono::Duration::weeks(2) + chrono::Duration::days(1)), // same week as file3
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3);
    }

    #[test]
    fn test_keep_one_per_month() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_monthly: 6,
            ..zero_config()
        };

        // Create files in different months
        let files = vec![
            make_file_info("file1.txt", NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()),
            make_file_info("file2.txt", NaiveDate::from_ymd_opt(2024, 5, 10).unwrap()),
            make_file_info("file3.txt", NaiveDate::from_ymd_opt(2024, 5, 5).unwrap()), // same month
            make_file_info("file4.txt", NaiveDate::from_ymd_opt(2024, 4, 20).unwrap()),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(!keep.contains(&2)); // duplicate month
        assert!(keep.contains(&3));
    }

    #[test]
    fn test_keep_one_per_year() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_yearly: 5,
            ..zero_config()
        };

        // Create files in different years
        let files = vec![
            make_file_info("file1.txt", NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()),
            make_file_info("file2.txt", NaiveDate::from_ymd_opt(2023, 3, 10).unwrap()),
            make_file_info("file3.txt", NaiveDate::from_ymd_opt(2023, 1, 5).unwrap()), // same year
            make_file_info("file4.txt", NaiveDate::from_ymd_opt(2022, 12, 20).unwrap()),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(!keep.contains(&2)); // duplicate year
        assert!(keep.contains(&3));
    }

    #[test]
    fn test_empty_files() {
        let now = Local::now();
        let config = RetentionConfig::default();
        let files: Vec<FileInfo> = vec![];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert!(keep.is_empty());
    }

    #[test]
    fn test_files_outside_retention_window() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_daily: 5,
            ..zero_config()
        };

        // File is 10 days old, outside the 5-day window
        let files = vec![
            make_file_info("old_file.txt", now.date_naive() - chrono::Duration::days(10)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert!(keep.is_empty());
    }

    #[test]
    fn test_combined_retention_policies() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_last: 2,
            keep_hourly: 3,
            keep_daily: 3,
            keep_weekly: 2,
            keep_monthly: 2,
            keep_yearly: 1,
        };

        let files = vec![
            make_file_info_with_time("file1.txt", now),
            make_file_info_with_time("file2.txt", now - chrono::Duration::hours(1)),
            make_file_info_with_time("file3.txt", now - chrono::Duration::days(1)),
            make_file_info_with_time("file4.txt", now - chrono::Duration::days(10)),
            make_file_info_with_time("file5.txt", now - chrono::Duration::days(40)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 5); // All files kept by various policies
    }

    #[test]
    fn test_keep_last_more_than_files() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_last: 100,
            ..zero_config()
        };

        let files = vec![
            make_file_info("file1.txt", now.date_naive()),
            make_file_info("file2.txt", now.date_naive() - chrono::Duration::days(1)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 2);
    }

    #[test]
    fn test_retention_config_from_args() {
        let args = Args {
            directory: PathBuf::from("/tmp"),
            keep_last: 5,
            keep_hourly: 24,
            keep_daily: 7,
            keep_weekly: 4,
            keep_monthly: 12,
            keep_yearly: 10,
            dry_run: false,
        };

        let config = RetentionConfig::from(&args);
        assert_eq!(config, RetentionConfig::default());
    }

    #[test]
    fn test_iso_week_year_boundary() {
        // Test that ISO week handles year boundaries correctly
        // Dec 31, 2024 is in ISO week 1 of 2025
        let date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let (year, week) = get_week_key(date);
        // The ISO week year for Dec 31, 2024 should be 2025
        assert_eq!(year, 2025);
        assert_eq!(week, 1);
    }
}
