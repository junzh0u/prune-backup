use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate};
use clap::Parser;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "rotate")]
#[command(about = "Rotate files based on creation date, keeping recent files and moving old ones to trash")]
struct Args {
    /// Directory to scan for files
    directory: PathBuf,

    /// Number of latest files to keep
    #[arg(short = 'n', long, default_value = "10")]
    latest: usize,

    /// Keep 1 file per day for this many days
    #[arg(short = 'd', long, default_value = "10")]
    days: u32,

    /// Keep 1 file per week for this many weeks
    #[arg(short = 'w', long, default_value = "4")]
    weeks: u32,

    /// Keep 1 file per month for this many months
    #[arg(short = 'm', long, default_value = "12")]
    months: u32,

    /// Keep 1 file per year for this many years
    #[arg(short = 'y', long, default_value = "10")]
    years: u32,

    /// Dry run - show what would be moved without actually moving
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone)]
struct FileInfo {
    path: PathBuf,
    created: DateTime<Local>,
}

#[derive(Debug, Clone)]
struct RetentionConfig {
    latest: usize,
    days: u32,
    weeks: u32,
    months: u32,
    years: u32,
}

impl From<&Args> for RetentionConfig {
    fn from(args: &Args) -> Self {
        Self {
            latest: args.latest,
            days: args.days,
            weeks: args.weeks,
            months: args.months,
            years: args.years,
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

fn get_week_key(date: NaiveDate) -> (i32, u32) {
    (date.iso_week().year(), date.iso_week().week())
}

fn get_month_key(date: NaiveDate) -> (i32, u32) {
    (date.year(), date.month())
}

fn get_year_key(date: NaiveDate) -> i32 {
    date.year()
}

fn select_files_to_keep_with_date(
    files: &[FileInfo],
    config: &RetentionConfig,
    today: NaiveDate,
) -> HashSet<usize> {
    let mut keep_indices: HashSet<usize> = HashSet::new();

    // 1. Keep latest N files
    for i in 0..config.latest.min(files.len()) {
        keep_indices.insert(i);
    }

    // Track which periods we've already covered
    let mut covered_days: HashSet<NaiveDate> = HashSet::new();
    let mut covered_weeks: HashSet<(i32, u32)> = HashSet::new();
    let mut covered_months: HashSet<(i32, u32)> = HashSet::new();
    let mut covered_years: HashSet<i32> = HashSet::new();

    // Calculate date boundaries
    let day_boundary = today - chrono::Duration::days(config.days as i64);
    let week_boundary = today - chrono::Duration::weeks(config.weeks as i64);
    let month_boundary = today - chrono::Duration::days(config.months as i64 * 30);
    let year_boundary = today - chrono::Duration::days(config.years as i64 * 365);

    // Iterate through files (already sorted newest first)
    for (i, file) in files.iter().enumerate() {
        let file_date = file.created.date_naive();

        // 2. Keep 1 file per day for D days
        if file_date >= day_boundary && !covered_days.contains(&file_date) {
            covered_days.insert(file_date);
            keep_indices.insert(i);
        }

        // 3. Keep 1 file per week for W weeks
        let week_key = get_week_key(file_date);
        if file_date >= week_boundary && !covered_weeks.contains(&week_key) {
            covered_weeks.insert(week_key);
            keep_indices.insert(i);
        }

        // 4. Keep 1 file per month for M months
        let month_key = get_month_key(file_date);
        if file_date >= month_boundary && !covered_months.contains(&month_key) {
            covered_months.insert(month_key);
            keep_indices.insert(i);
        }

        // 5. Keep 1 file per year for Y years
        let year_key = get_year_key(file_date);
        if file_date >= year_boundary && !covered_years.contains(&year_key) {
            covered_years.insert(year_key);
            keep_indices.insert(i);
        }
    }

    keep_indices
}

fn select_files_to_keep(files: &[FileInfo], config: &RetentionConfig) -> HashSet<usize> {
    let today = Local::now().date_naive();
    select_files_to_keep_with_date(files, config, today)
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
    use chrono::{TimeZone, NaiveDate};

    fn make_file_info(name: &str, date: NaiveDate) -> FileInfo {
        let datetime = Local.from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap();
        FileInfo {
            path: PathBuf::from(name),
            created: datetime,
        }
    }

    fn default_config() -> RetentionConfig {
        RetentionConfig {
            latest: 10,
            days: 10,
            weeks: 4,
            months: 12,
            years: 10,
        }
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
    fn test_keep_latest_n_files() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let config = RetentionConfig {
            latest: 3,
            days: 0,
            weeks: 0,
            months: 0,
            years: 0,
        };

        // Create 5 files, all on the same day (so only latest N applies)
        let files: Vec<FileInfo> = (0..5)
            .map(|i| {
                let datetime = Local.from_local_datetime(
                    &today.and_hms_opt(12, i, 0).unwrap()
                ).single().unwrap();
                FileInfo {
                    path: PathBuf::from(format!("file{}.txt", i)),
                    created: datetime,
                }
            })
            .rev() // newest first
            .collect();

        let keep = select_files_to_keep_with_date(&files, &config, today);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(keep.contains(&2));
    }

    #[test]
    fn test_keep_one_per_day() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let config = RetentionConfig {
            latest: 0,
            days: 5,
            weeks: 0,
            months: 0,
            years: 0,
        };

        // Create files for 3 different days within range
        let files = vec![
            make_file_info("file1.txt", today),
            make_file_info("file2.txt", today - chrono::Duration::days(1)),
            make_file_info("file3.txt", today - chrono::Duration::days(2)),
            make_file_info("file4.txt", today - chrono::Duration::days(2)), // duplicate day
        ];

        let keep = select_files_to_keep_with_date(&files, &config, today);
        // Should keep 3 files (one per unique day), file4 is duplicate
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0)); // today
        assert!(keep.contains(&1)); // yesterday
        assert!(keep.contains(&2)); // 2 days ago (first one)
        assert!(!keep.contains(&3)); // duplicate day, not kept
    }

    #[test]
    fn test_keep_one_per_week() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(); // Saturday
        let config = RetentionConfig {
            latest: 0,
            days: 0,
            weeks: 4,
            months: 0,
            years: 0,
        };

        // Create files spanning 3 weeks
        let files = vec![
            make_file_info("file1.txt", today),
            make_file_info("file2.txt", today - chrono::Duration::weeks(1)),
            make_file_info("file3.txt", today - chrono::Duration::weeks(2)),
            make_file_info("file4.txt", today - chrono::Duration::weeks(2) + chrono::Duration::days(1)), // same week as file3
        ];

        let keep = select_files_to_keep_with_date(&files, &config, today);
        assert_eq!(keep.len(), 3);
    }

    #[test]
    fn test_keep_one_per_month() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let config = RetentionConfig {
            latest: 0,
            days: 0,
            weeks: 0,
            months: 6,
            years: 0,
        };

        // Create files in different months
        let files = vec![
            make_file_info("file1.txt", NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()),
            make_file_info("file2.txt", NaiveDate::from_ymd_opt(2024, 5, 10).unwrap()),
            make_file_info("file3.txt", NaiveDate::from_ymd_opt(2024, 5, 5).unwrap()), // same month as file2
            make_file_info("file4.txt", NaiveDate::from_ymd_opt(2024, 4, 20).unwrap()),
        ];

        let keep = select_files_to_keep_with_date(&files, &config, today);
        assert_eq!(keep.len(), 3); // June, May (first one), April
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(!keep.contains(&2)); // duplicate month
        assert!(keep.contains(&3));
    }

    #[test]
    fn test_keep_one_per_year() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let config = RetentionConfig {
            latest: 0,
            days: 0,
            weeks: 0,
            months: 0,
            years: 5,
        };

        // Create files in different years
        let files = vec![
            make_file_info("file1.txt", NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()),
            make_file_info("file2.txt", NaiveDate::from_ymd_opt(2023, 3, 10).unwrap()),
            make_file_info("file3.txt", NaiveDate::from_ymd_opt(2023, 1, 5).unwrap()), // same year as file2
            make_file_info("file4.txt", NaiveDate::from_ymd_opt(2022, 12, 20).unwrap()),
        ];

        let keep = select_files_to_keep_with_date(&files, &config, today);
        assert_eq!(keep.len(), 3); // 2024, 2023 (first one), 2022
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(!keep.contains(&2)); // duplicate year
        assert!(keep.contains(&3));
    }

    #[test]
    fn test_empty_files() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let config = default_config();
        let files: Vec<FileInfo> = vec![];

        let keep = select_files_to_keep_with_date(&files, &config, today);
        assert!(keep.is_empty());
    }

    #[test]
    fn test_files_outside_retention_window() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let config = RetentionConfig {
            latest: 0,
            days: 5,
            weeks: 0,
            months: 0,
            years: 0,
        };

        // File is 10 days old, outside the 5-day window
        let files = vec![
            make_file_info("old_file.txt", today - chrono::Duration::days(10)),
        ];

        let keep = select_files_to_keep_with_date(&files, &config, today);
        assert!(keep.is_empty());
    }

    #[test]
    fn test_combined_retention_policies() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let config = RetentionConfig {
            latest: 2,
            days: 3,
            weeks: 2,
            months: 2,
            years: 1,
        };

        let files = vec![
            make_file_info("file1.txt", today),                                    // kept by latest + day + week + month + year
            make_file_info("file2.txt", today - chrono::Duration::days(1)),        // kept by latest + day
            make_file_info("file3.txt", today - chrono::Duration::days(2)),        // kept by day
            make_file_info("file4.txt", today - chrono::Duration::days(10)),       // kept by week
            make_file_info("file5.txt", today - chrono::Duration::days(40)),       // kept by month
        ];

        let keep = select_files_to_keep_with_date(&files, &config, today);
        assert_eq!(keep.len(), 5); // All files should be kept by various policies
    }

    #[test]
    fn test_latest_more_than_files() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let config = RetentionConfig {
            latest: 100, // More than available files
            days: 0,
            weeks: 0,
            months: 0,
            years: 0,
        };

        let files = vec![
            make_file_info("file1.txt", today),
            make_file_info("file2.txt", today - chrono::Duration::days(1)),
        ];

        let keep = select_files_to_keep_with_date(&files, &config, today);
        assert_eq!(keep.len(), 2); // Should keep all files
    }

    #[test]
    fn test_retention_config_from_args() {
        let args = Args {
            directory: PathBuf::from("/tmp"),
            latest: 5,
            days: 7,
            weeks: 3,
            months: 6,
            years: 2,
            dry_run: false,
        };

        let config = RetentionConfig::from(&args);
        assert_eq!(config.latest, 5);
        assert_eq!(config.days, 7);
        assert_eq!(config.weeks, 3);
        assert_eq!(config.months, 6);
        assert_eq!(config.years, 2);
    }
}
