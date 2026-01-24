use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Reason why a file was kept by the retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionReason {
    /// Kept as one of the last N files
    KeepLast,
    /// Kept as the representative for an hour
    Hourly,
    /// Kept as the representative for a day
    Daily,
    /// Kept as the representative for a week
    Weekly,
    /// Kept as the representative for a month
    Monthly,
    /// Kept as the representative for a year
    Yearly,
}

impl fmt::Display for RetentionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeepLast => write!(f, "keep-last"),
            Self::Hourly => write!(f, "hourly"),
            Self::Daily => write!(f, "daily"),
            Self::Weekly => write!(f, "weekly"),
            Self::Monthly => write!(f, "monthly"),
            Self::Yearly => write!(f, "yearly"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub created: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetentionConfig {
    pub keep_last: usize,
    pub keep_hourly: u32,
    pub keep_daily: u32,
    pub keep_weekly: u32,
    pub keep_monthly: u32,
    pub keep_yearly: u32,
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

/// Gets the modification time of a file, falling back to creation time.
///
/// Uses modification time as primary because it's more reliable across platforms
/// and is what backup tools typically track for file age.
///
/// # Errors
/// Returns an error if file metadata cannot be read or no timestamp is available.
pub fn get_file_creation_time(path: &Path) -> Result<DateTime<Local>> {
    let metadata = fs::metadata(path).context("Failed to read file metadata")?;
    let mtime = metadata
        .modified()
        .or_else(|_| metadata.created())
        .context("Failed to get file modification/creation time")?;
    Ok(DateTime::from(mtime))
}

/// Scans a directory for files and returns them sorted by creation time (newest first).
///
/// # Errors
/// Returns an error if the directory cannot be read.
pub fn scan_files(dir: &Path) -> Result<Vec<FileInfo>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(dir).context("Failed to read directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        // Skip directories and hidden files
        if path.is_dir()
            || path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }

        match get_file_creation_time(&path) {
            Ok(created) => files.push(FileInfo { path, created }),
            Err(e) => eprintln!("Warning: Skipping {}: {e}", path.display()),
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

/// Selects files to keep with reasons, using a specific datetime as "now".
#[must_use]
pub fn select_files_to_keep_with_reasons(
    files: &[FileInfo],
    config: &RetentionConfig,
    now: DateTime<Local>,
) -> HashMap<usize, RetentionReason> {
    let mut keep_reasons: HashMap<usize, RetentionReason> = HashMap::new();
    let today = now.date_naive();

    // 1. Keep last N files (processed first)
    for i in 0..config.keep_last.min(files.len()) {
        keep_reasons.insert(i, RetentionReason::KeepLast);
    }

    // 2. Keep 1 file per hour for N hours (oldest file in each hour)
    // Only consider files not already kept by previous policies
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_hourly > 0 {
        let hour_boundary = now - chrono::Duration::hours(i64::from(config.keep_hourly));
        let mut covered_hours: HashSet<(i32, u32, u32, u32)> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            if keep_reasons.contains_key(&i) {
                continue; // Skip files already kept by earlier policies
            }
            let file_datetime = file.created;
            let hour_key = get_hour_key(file_datetime);
            if file_datetime >= hour_boundary && !covered_hours.contains(&hour_key) {
                covered_hours.insert(hour_key);
                keep_reasons.insert(i, RetentionReason::Hourly);
            }
        }
    }

    // 3. Keep 1 file per day for N days (oldest file in each day)
    // Only consider files not already kept by previous policies
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_daily > 0 {
        let day_boundary = today - chrono::Duration::days(i64::from(config.keep_daily));
        let mut covered_days: HashSet<NaiveDate> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            if keep_reasons.contains_key(&i) {
                continue; // Skip files already kept by earlier policies
            }
            let file_date = file.created.date_naive();
            if file_date >= day_boundary && !covered_days.contains(&file_date) {
                covered_days.insert(file_date);
                keep_reasons.insert(i, RetentionReason::Daily);
            }
        }
    }

    // 4. Keep 1 file per week for N weeks (ISO week system, oldest file in each week)
    // Only consider files not already kept by previous policies
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_weekly > 0 {
        let week_boundary = today - chrono::Duration::weeks(i64::from(config.keep_weekly));
        let mut covered_weeks: HashSet<(i32, u32)> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            if keep_reasons.contains_key(&i) {
                continue; // Skip files already kept by earlier policies
            }
            let file_date = file.created.date_naive();
            let week_key = get_week_key(file_date);
            if file_date >= week_boundary && !covered_weeks.contains(&week_key) {
                covered_weeks.insert(week_key);
                keep_reasons.insert(i, RetentionReason::Weekly);
            }
        }
    }

    // 5. Keep 1 file per month for N months (oldest file in each month)
    // Only consider files not already kept by previous policies
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_monthly > 0 {
        let month_boundary = today - chrono::Duration::days(i64::from(config.keep_monthly) * 30);
        let mut covered_months: HashSet<(i32, u32)> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            if keep_reasons.contains_key(&i) {
                continue; // Skip files already kept by earlier policies
            }
            let file_date = file.created.date_naive();
            let month_key = get_month_key(file_date);
            if file_date >= month_boundary && !covered_months.contains(&month_key) {
                covered_months.insert(month_key);
                keep_reasons.insert(i, RetentionReason::Monthly);
            }
        }
    }

    // 6. Keep 1 file per year for N years (oldest file in each year)
    // Only consider files not already kept by previous policies
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_yearly > 0 {
        let year_boundary = today - chrono::Duration::days(i64::from(config.keep_yearly) * 365);
        let mut covered_years: HashSet<i32> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            if keep_reasons.contains_key(&i) {
                continue; // Skip files already kept by earlier policies
            }
            let file_date = file.created.date_naive();
            let year_key = get_year_key(file_date);
            if file_date >= year_boundary && !covered_years.contains(&year_key) {
                covered_years.insert(year_key);
                keep_reasons.insert(i, RetentionReason::Yearly);
            }
        }
    }

    keep_reasons
}

#[must_use]
pub fn select_files_to_keep_with_datetime(
    files: &[FileInfo],
    config: &RetentionConfig,
    now: DateTime<Local>,
) -> HashSet<usize> {
    select_files_to_keep_with_reasons(files, config, now)
        .into_keys()
        .collect()
}

#[must_use]
pub fn select_files_to_keep(files: &[FileInfo], config: &RetentionConfig) -> HashSet<usize> {
    let now = Local::now();
    select_files_to_keep_with_datetime(files, config, now)
}

/// Moves a file to the trash directory.
///
/// # Errors
/// Returns an error if the file cannot be moved or has no filename.
pub fn move_to_trash(file: &Path, trash_dir: &Path, dry_run: bool) -> Result<()> {
    let file_name = file.file_name().context("Failed to get file name")?;
    let dest = trash_dir.join(file_name);

    if dry_run {
        println!("Would move: {} -> {}", file.display(), dest.display());
    } else {
        // Handle name conflicts by appending a number
        let mut final_dest = dest.clone();
        let mut counter = 1;
        while final_dest.exists() {
            let stem = dest.file_stem().unwrap_or_default().to_string_lossy();
            let ext = dest
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            final_dest = trash_dir.join(format!("{stem}_{counter}{ext}"));
            counter += 1;
        }
        fs::rename(file, &final_dest).context("Failed to move file to trash")?;
        println!("Moved: {} -> {}", file.display(), final_dest.display());
    }

    Ok(())
}

/// Rotates files in a directory based on retention policies.
///
/// # Errors
/// Returns an error if:
/// - `keep_last` is 0 (must be at least 1)
/// - The directory cannot be read
/// - Trash directory cannot be created
/// - Files cannot be moved
pub fn rotate_files(dir: &Path, config: &RetentionConfig, dry_run: bool) -> Result<(usize, usize)> {
    if config.keep_last == 0 {
        anyhow::bail!("keep-last must be at least 1");
    }

    // Create trash directory
    let trash_dir = dir.join(".trash");
    if !dry_run && !trash_dir.exists() {
        fs::create_dir(&trash_dir).context("Failed to create .trash directory")?;
    }

    // Scan files
    let files = scan_files(dir)?;

    if files.is_empty() {
        return Ok((0, 0));
    }

    // Determine which files to keep and why
    let now = Local::now();
    let keep_reasons = select_files_to_keep_with_reasons(&files, config, now);

    // Print kept files with reasons
    for (i, file) in files.iter().enumerate() {
        if let Some(reason) = keep_reasons.get(&i) {
            let prefix = if dry_run { "Would keep" } else { "Keeping" };
            println!("{prefix}: {} ({reason})", file.path.display());
        }
    }

    // Move files that are not in keep set
    let mut moved_count = 0;
    for (i, file) in files.iter().enumerate() {
        if !keep_reasons.contains_key(&i) {
            move_to_trash(&file.path, &trash_dir, dry_run)?;
            moved_count += 1;
        }
    }

    Ok((keep_reasons.len(), moved_count))
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
        let datetime = Local
            .from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
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

        // Create files in different hours (sorted newest-first like scan_files does)
        // file4 is newer than file3 but in the same hour
        let files = vec![
            make_file_info_with_time("file1.txt", now),
            make_file_info_with_time("file2.txt", now - chrono::Duration::hours(1)),
            make_file_info_with_time(
                "file4.txt",
                now - chrono::Duration::hours(2) + chrono::Duration::minutes(30),
            ), // hour 10, newer
            make_file_info_with_time("file3.txt", now - chrono::Duration::hours(2)), // hour 10, older
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3); // 3 unique hours
        assert!(keep.contains(&0)); // hour 12
        assert!(keep.contains(&1)); // hour 11
        assert!(keep.contains(&3)); // hour 10 (oldest file in that hour)
        assert!(!keep.contains(&2)); // same hour as file3, not kept (newer)
    }

    #[test]
    fn test_keep_one_per_day() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let today = now.date_naive();
        let config = RetentionConfig {
            keep_daily: 5,
            ..zero_config()
        };

        // Create files for different days (sorted newest-first like scan_files does)
        // file3 and file4 are on the same day, but file4 is older
        let files = vec![
            make_file_info("file1.txt", today),
            make_file_info("file2.txt", today - chrono::Duration::days(1)),
            make_file_info("file3.txt", today - chrono::Duration::days(2)), // newer on day -2
            make_file_info("file4.txt", today - chrono::Duration::days(2)), // older on day -2 (same day)
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0)); // today
        assert!(keep.contains(&1)); // yesterday
        assert!(keep.contains(&3)); // 2 days ago (oldest file on that day)
        assert!(!keep.contains(&2)); // duplicate day, not kept
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
            make_file_info(
                "file4.txt",
                today - chrono::Duration::weeks(2) + chrono::Duration::days(1),
            ), // same week as file3
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

        // Create files in different months (sorted newest-first like scan_files does)
        // file2 and file3 are in the same month (May), but file3 is older
        let files = vec![
            make_file_info("file1.txt", NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()),
            make_file_info("file2.txt", NaiveDate::from_ymd_opt(2024, 5, 10).unwrap()), // May, newer
            make_file_info("file3.txt", NaiveDate::from_ymd_opt(2024, 5, 5).unwrap()), // May, older
            make_file_info("file4.txt", NaiveDate::from_ymd_opt(2024, 4, 20).unwrap()),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0)); // June
        assert!(keep.contains(&2)); // May (oldest file in that month)
        assert!(!keep.contains(&1)); // May duplicate, not kept (newer)
        assert!(keep.contains(&3)); // April
    }

    #[test]
    fn test_keep_one_per_year() {
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_yearly: 5,
            ..zero_config()
        };

        // Create files in different years (sorted newest-first like scan_files does)
        // file2 and file3 are in the same year (2023), but file3 is older
        let files = vec![
            make_file_info("file1.txt", NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()),
            make_file_info("file2.txt", NaiveDate::from_ymd_opt(2023, 3, 10).unwrap()), // 2023, newer
            make_file_info("file3.txt", NaiveDate::from_ymd_opt(2023, 1, 5).unwrap()), // 2023, older
            make_file_info("file4.txt", NaiveDate::from_ymd_opt(2022, 12, 20).unwrap()),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0)); // 2024
        assert!(keep.contains(&2)); // 2023 (oldest file in that year)
        assert!(!keep.contains(&1)); // 2023 duplicate, not kept (newer)
        assert!(keep.contains(&3)); // 2022
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
        let files = vec![make_file_info(
            "old_file.txt",
            now.date_naive() - chrono::Duration::days(10),
        )];

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
    fn test_iso_week_year_boundary() {
        // Test that ISO week handles year boundaries correctly
        // Dec 31, 2024 is in ISO week 1 of 2025
        let date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let (year, week) = get_week_key(date);
        // The ISO week year for Dec 31, 2024 should be 2025
        assert_eq!(year, 2025);
        assert_eq!(week, 1);
    }

    // ==================== CASCADING RETENTION TESTS ====================
    // These tests verify that retention options are processed in order and
    // each option only considers backups not already covered by previous options.

    #[test]
    fn test_cascading_keep_last_excludes_from_hourly() {
        // Files kept by keep-last should NOT count toward hourly coverage.
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 30, 0).unwrap();
        let config = RetentionConfig {
            keep_last: 2,
            keep_hourly: 2,
            ..zero_config()
        };

        // 3 files all in hour 12, plus 1 file in hour 11
        let files = vec![
            make_file_info_with_time("file0.txt", now),
            make_file_info_with_time("file1.txt", now - chrono::Duration::minutes(10)),
            make_file_info_with_time("file2.txt", now - chrono::Duration::minutes(20)),
            make_file_info_with_time("file3.txt", now - chrono::Duration::hours(1)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);

        assert_eq!(keep.len(), 4);
        assert!(keep.contains(&0)); // keep-last
        assert!(keep.contains(&1)); // keep-last
        assert!(keep.contains(&2)); // hourly (first hour 12 from hourly's perspective)
        assert!(keep.contains(&3)); // hourly (hour 11)
    }

    #[test]
    fn test_cascading_hourly_excludes_from_daily() {
        // Files kept by keep-hourly should NOT count toward daily coverage.
        let now = Local.with_ymd_and_hms(2024, 6, 15, 14, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_hourly: 2,
            keep_daily: 2,
            ..zero_config()
        };

        let files = vec![
            make_file_info_with_time("file0.txt", now),
            make_file_info_with_time("file1.txt", now - chrono::Duration::hours(1)),
            make_file_info_with_time("file2.txt", now - chrono::Duration::days(1)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);

        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0)); // hourly
        assert!(keep.contains(&1)); // hourly
        assert!(keep.contains(&2)); // daily
    }

    #[test]
    fn test_cascading_daily_excludes_from_weekly() {
        // Files kept by keep-daily should NOT count toward weekly coverage.
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap(); // Saturday
        let config = RetentionConfig {
            keep_daily: 3,
            keep_weekly: 2,
            ..zero_config()
        };

        let files = vec![
            make_file_info("file0.txt", now.date_naive()),
            make_file_info("file1.txt", now.date_naive() - chrono::Duration::days(1)),
            make_file_info("file2.txt", now.date_naive() - chrono::Duration::days(2)),
            make_file_info("file3.txt", now.date_naive() - chrono::Duration::weeks(1)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);

        assert_eq!(keep.len(), 4);
        assert!(keep.contains(&3)); // weekly should pick this up
    }

    #[test]
    fn test_cascading_weekly_excludes_from_monthly() {
        // Files kept by keep-weekly should NOT count toward monthly coverage.
        let now = Local.with_ymd_and_hms(2024, 6, 28, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_weekly: 2,
            keep_monthly: 3,
            ..zero_config()
        };

        let files = vec![
            make_file_info("file0.txt", NaiveDate::from_ymd_opt(2024, 6, 28).unwrap()),
            make_file_info("file1.txt", NaiveDate::from_ymd_opt(2024, 6, 21).unwrap()),
            make_file_info("file2.txt", NaiveDate::from_ymd_opt(2024, 5, 15).unwrap()),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);

        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&2)); // monthly
    }

    #[test]
    fn test_cascading_monthly_excludes_from_yearly() {
        // Files kept by keep-monthly should NOT count toward yearly coverage.
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_monthly: 2,
            keep_yearly: 3,
            ..zero_config()
        };

        let files = vec![
            make_file_info("file0.txt", NaiveDate::from_ymd_opt(2024, 6, 15).unwrap()),
            make_file_info("file1.txt", NaiveDate::from_ymd_opt(2024, 5, 15).unwrap()),
            make_file_info("file2.txt", NaiveDate::from_ymd_opt(2023, 12, 1).unwrap()),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);

        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&2)); // yearly
    }

    #[test]
    fn test_cascading_full_chain() {
        // Test the full cascading chain: keep-last -> hourly -> daily -> weekly -> monthly -> yearly
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_last: 1,
            keep_hourly: 1,
            keep_daily: 1,
            keep_weekly: 1,
            keep_monthly: 1,
            keep_yearly: 1,
        };

        let files = vec![
            make_file_info_with_time("file0.txt", now),
            make_file_info_with_time("file1.txt", now - chrono::Duration::minutes(30)),
            make_file_info_with_time("file2.txt", now - chrono::Duration::hours(5)),
            make_file_info_with_time("file3.txt", now - chrono::Duration::days(2)),
            make_file_info_with_time("file4.txt", now - chrono::Duration::weeks(2)),
            make_file_info("file5.txt", NaiveDate::from_ymd_opt(2023, 7, 15).unwrap()),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);

        assert_eq!(keep.len(), 6);
        assert!(keep.contains(&0)); // keep-last
        assert!(keep.contains(&1)); // hourly
        assert!(keep.contains(&2)); // daily
        assert!(keep.contains(&3)); // weekly
        assert!(keep.contains(&4)); // monthly
        assert!(keep.contains(&5)); // yearly
    }

    #[test]
    fn test_cascading_same_period_multiple_files() {
        // When multiple files exist in the same period, later policy picks the oldest uncovered file.
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_last: 1,
            keep_daily: 2,
            ..zero_config()
        };

        let files = vec![
            make_file_info_with_time("file0.txt", now),
            make_file_info_with_time("file1.txt", now - chrono::Duration::hours(2)),
            make_file_info_with_time("file2.txt", now - chrono::Duration::hours(4)),
            make_file_info_with_time("file3.txt", now - chrono::Duration::days(1)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);

        assert_eq!(keep.len(), 3);
        assert!(keep.contains(&0)); // keep-last
        assert!(keep.contains(&2)); // daily (oldest uncovered for today)
        assert!(!keep.contains(&1)); // today already covered by index 2
        assert!(keep.contains(&3)); // daily (yesterday)
    }
}
