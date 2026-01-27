use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Reason why a file was kept by the retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Configuration read from a `.retention` file in TOML format.
/// All fields are optional; missing fields will use CLI args or defaults.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RetentionFileConfig {
    pub keep_last: Option<usize>,
    pub keep_hourly: Option<u32>,
    pub keep_daily: Option<u32>,
    pub keep_weekly: Option<u32>,
    pub keep_monthly: Option<u32>,
    pub keep_yearly: Option<u32>,
}

/// The name of the retention configuration file.
pub const RETENTION_FILE_NAME: &str = ".retention";

/// Reads a `.retention` file from the given directory.
///
/// # Returns
/// - `Ok(Some(config))` if the file exists and was parsed successfully
/// - `Ok(None)` if the file does not exist
///
/// # Errors
/// Returns an error if the file exists but cannot be read or parsed as valid TOML.
pub fn read_retention_file(dir: &Path) -> Result<Option<RetentionFileConfig>> {
    let file_path = dir.join(RETENTION_FILE_NAME);

    if !file_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;

    let config: RetentionFileConfig = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse {} as TOML", file_path.display()))?;

    Ok(Some(config))
}

/// Resolves the final retention configuration from CLI args and file config.
///
/// Priority (highest to lowest):
/// 1. CLI argument (if provided by user)
/// 2. File config value (if present in .retention file)
/// 3. Built-in default
#[must_use]
pub fn resolve_config(
    cli_keep_last: Option<usize>,
    cli_keep_hourly: Option<u32>,
    cli_keep_daily: Option<u32>,
    cli_keep_weekly: Option<u32>,
    cli_keep_monthly: Option<u32>,
    cli_keep_yearly: Option<u32>,
    file_config: Option<&RetentionFileConfig>,
) -> RetentionConfig {
    let defaults = RetentionConfig::default();

    RetentionConfig {
        keep_last: cli_keep_last
            .or_else(|| file_config.and_then(|f| f.keep_last))
            .unwrap_or(defaults.keep_last),
        keep_hourly: cli_keep_hourly
            .or_else(|| file_config.and_then(|f| f.keep_hourly))
            .unwrap_or(defaults.keep_hourly),
        keep_daily: cli_keep_daily
            .or_else(|| file_config.and_then(|f| f.keep_daily))
            .unwrap_or(defaults.keep_daily),
        keep_weekly: cli_keep_weekly
            .or_else(|| file_config.and_then(|f| f.keep_weekly))
            .unwrap_or(defaults.keep_weekly),
        keep_monthly: cli_keep_monthly
            .or_else(|| file_config.and_then(|f| f.keep_monthly))
            .unwrap_or(defaults.keep_monthly),
        keep_yearly: cli_keep_yearly
            .or_else(|| file_config.and_then(|f| f.keep_yearly))
            .unwrap_or(defaults.keep_yearly),
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
///
/// Each retention policy is applied independently to all files. A file may be kept
/// by multiple policies, and all matching policies are tracked in the returned map.
#[must_use]
pub fn select_files_to_keep_with_reasons(
    files: &[FileInfo],
    config: &RetentionConfig,
    now: DateTime<Local>,
) -> HashMap<usize, HashSet<RetentionReason>> {
    let mut keep_reasons: HashMap<usize, HashSet<RetentionReason>> = HashMap::new();
    let today = now.date_naive();

    // Helper to add a reason for a file
    let mut add_reason = |i: usize, reason: RetentionReason| {
        keep_reasons.entry(i).or_default().insert(reason);
    };

    // 1. Keep last N files
    for i in 0..config.keep_last.min(files.len()) {
        add_reason(i, RetentionReason::KeepLast);
    }

    // 2. Keep 1 file per hour for N hours (oldest file in each hour)
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_hourly > 0 {
        let hour_boundary = now - chrono::Duration::hours(i64::from(config.keep_hourly));
        let mut covered_hours: HashSet<(i32, u32, u32, u32)> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            let file_datetime = file.created;
            let hour_key = get_hour_key(file_datetime);
            if file_datetime >= hour_boundary && !covered_hours.contains(&hour_key) {
                covered_hours.insert(hour_key);
                add_reason(i, RetentionReason::Hourly);
            }
        }
    }

    // 3. Keep 1 file per day for N days (oldest file in each day)
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_daily > 0 {
        let day_boundary = today - chrono::Duration::days(i64::from(config.keep_daily));
        let mut covered_days: HashSet<NaiveDate> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            let file_date = file.created.date_naive();
            if file_date >= day_boundary && !covered_days.contains(&file_date) {
                covered_days.insert(file_date);
                add_reason(i, RetentionReason::Daily);
            }
        }
    }

    // 4. Keep 1 file per week for N weeks (ISO week system, oldest file in each week)
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_weekly > 0 {
        let week_boundary = today - chrono::Duration::weeks(i64::from(config.keep_weekly));
        let mut covered_weeks: HashSet<(i32, u32)> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            let file_date = file.created.date_naive();
            let week_key = get_week_key(file_date);
            if file_date >= week_boundary && !covered_weeks.contains(&week_key) {
                covered_weeks.insert(week_key);
                add_reason(i, RetentionReason::Weekly);
            }
        }
    }

    // 5. Keep 1 file per month for N months (oldest file in each month)
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_monthly > 0 {
        let month_boundary = today - chrono::Duration::days(i64::from(config.keep_monthly) * 30);
        let mut covered_months: HashSet<(i32, u32)> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            let file_date = file.created.date_naive();
            let month_key = get_month_key(file_date);
            if file_date >= month_boundary && !covered_months.contains(&month_key) {
                covered_months.insert(month_key);
                add_reason(i, RetentionReason::Monthly);
            }
        }
    }

    // 6. Keep 1 file per year for N years (oldest file in each year)
    // Iterate in reverse (oldest first) to keep oldest file per period
    if config.keep_yearly > 0 {
        let year_boundary = today - chrono::Duration::days(i64::from(config.keep_yearly) * 365);
        let mut covered_years: HashSet<i32> = HashSet::new();
        for (i, file) in files.iter().enumerate().rev() {
            let file_date = file.created.date_naive();
            let year_key = get_year_key(file_date);
            if file_date >= year_boundary && !covered_years.contains(&year_key) {
                covered_years.insert(year_key);
                add_reason(i, RetentionReason::Yearly);
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

/// Moves a file to the system trash, or uses a custom command if provided.
///
/// When using a custom command, `{}` in the command is replaced with the file path.
/// If `{}` is not present, the file path is appended to the command.
///
/// # Errors
/// Returns an error if the file cannot be moved to trash.
pub fn move_to_trash(file: &Path, dry_run: bool, trash_cmd: Option<&str>) -> Result<()> {
    if dry_run {
        println!("Would move to trash: {}", file.display());
    } else if let Some(cmd) = trash_cmd {
        let escaped_path = shell_escape::escape(file.to_string_lossy());
        let full_cmd = if cmd.contains("{}") {
            cmd.replace("{}", &escaped_path)
        } else {
            format!("{cmd} {escaped_path}")
        };
        let status = Command::new("sh")
            .arg("-c")
            .arg(&full_cmd)
            .status()
            .context("Failed to execute trash command")?;
        if !status.success() {
            anyhow::bail!(
                "Trash command failed with exit code: {}",
                status.code().unwrap_or(-1)
            );
        }
        println!("Moved to trash: {}", file.display());
    } else {
        trash::delete(file).context("Failed to move file to trash")?;
        println!("Moved to trash: {}", file.display());
    }

    Ok(())
}

/// Rotates files in a directory based on retention policies.
///
/// # Errors
/// Returns an error if:
/// - `keep_last` is 0 (must be at least 1)
/// - The directory cannot be read
/// - Files cannot be moved to trash
pub fn rotate_files(
    dir: &Path,
    config: &RetentionConfig,
    dry_run: bool,
    trash_cmd: Option<&str>,
) -> Result<(usize, usize)> {
    if config.keep_last == 0 {
        anyhow::bail!("keep-last must be at least 1");
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
        if let Some(reasons) = keep_reasons.get(&i) {
            let prefix = if dry_run { "Would keep" } else { "Keeping" };
            let reasons_str: Vec<String> = reasons.iter().map(ToString::to_string).collect();
            let reasons_display = reasons_str.join(", ");
            println!("{prefix}: {} ({reasons_display})", file.path.display());
        }
    }

    // Move files that are not in keep set to system trash
    let mut moved_count = 0;
    for (i, file) in files.iter().enumerate() {
        if !keep_reasons.contains_key(&i) {
            move_to_trash(&file.path, dry_run, trash_cmd)?;
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

    // ==================== INDEPENDENT RETENTION TESTS ====================
    // These tests verify that retention policies are applied independently,
    // and a file can be kept by multiple policies.

    #[test]
    fn test_independent_policies_multiple_reasons() {
        // A file can be kept by multiple policies simultaneously.
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_last: 1,
            keep_hourly: 2,
            keep_daily: 2,
            ..zero_config()
        };

        // file0 is keep-last and oldest in its hour (hour 12)
        // file1 is oldest in its hour (hour 11) AND oldest file on day 15
        let files = vec![
            make_file_info_with_time("file0.txt", now), // hour 12, day 15
            make_file_info_with_time("file1.txt", now - chrono::Duration::hours(1)), // hour 11, day 15
        ];

        let reasons = select_files_to_keep_with_reasons(&files, &config, now);

        // file0 should have keep-last and hourly (oldest in hour 12)
        let file0_reasons = reasons.get(&0).unwrap();
        assert!(file0_reasons.contains(&RetentionReason::KeepLast));
        assert!(file0_reasons.contains(&RetentionReason::Hourly));

        // file1 should have hourly (hour 11) and daily (oldest on day 15)
        let file1_reasons = reasons.get(&1).unwrap();
        assert!(file1_reasons.contains(&RetentionReason::Hourly));
        assert!(file1_reasons.contains(&RetentionReason::Daily));
    }

    #[test]
    fn test_independent_policies_overlapping_periods() {
        // Policies evaluate all files independently; overlapping periods are fine.
        let now = Local.with_ymd_and_hms(2024, 6, 15, 14, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_hourly: 3,
            keep_daily: 2,
            ..zero_config()
        };

        let files = vec![
            make_file_info_with_time("file0.txt", now),
            make_file_info_with_time("file1.txt", now - chrono::Duration::hours(1)),
            make_file_info_with_time("file2.txt", now - chrono::Duration::days(1)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);
        let reasons = select_files_to_keep_with_reasons(&files, &config, now);

        assert_eq!(keep.len(), 3);

        // file0: hourly only (hour 14)
        let file0_reasons = reasons.get(&0).unwrap();
        assert!(file0_reasons.contains(&RetentionReason::Hourly));
        assert!(!file0_reasons.contains(&RetentionReason::Daily)); // file1 is older on same day

        // file1: hourly (hour 13) + daily (oldest on day 15)
        let file1_reasons = reasons.get(&1).unwrap();
        assert!(file1_reasons.contains(&RetentionReason::Hourly));
        assert!(file1_reasons.contains(&RetentionReason::Daily));

        // file2: daily (day 14)
        assert!(reasons.get(&2).unwrap().contains(&RetentionReason::Daily));
    }

    #[test]
    fn test_independent_weekly_and_monthly() {
        // Weekly and monthly policies can both keep the same file.
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
        let reasons = select_files_to_keep_with_reasons(&files, &config, now);

        assert_eq!(keep.len(), 3);

        // file0: weekly (week 26)
        let file0_reasons = reasons.get(&0).unwrap();
        assert!(file0_reasons.contains(&RetentionReason::Weekly));

        // file1: weekly (week 25) + monthly (oldest in June)
        let file1_reasons = reasons.get(&1).unwrap();
        assert!(file1_reasons.contains(&RetentionReason::Weekly));
        assert!(file1_reasons.contains(&RetentionReason::Monthly));

        // file2: monthly (May)
        assert!(reasons.get(&2).unwrap().contains(&RetentionReason::Monthly));
    }

    #[test]
    fn test_independent_monthly_and_yearly() {
        // Monthly and yearly policies can both keep the same file.
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
        let reasons = select_files_to_keep_with_reasons(&files, &config, now);

        assert_eq!(keep.len(), 3);

        // file0: monthly (June 2024)
        let file0_reasons = reasons.get(&0).unwrap();
        assert!(file0_reasons.contains(&RetentionReason::Monthly));

        // file1: monthly (May 2024) + yearly (oldest in 2024)
        let file1_reasons = reasons.get(&1).unwrap();
        assert!(file1_reasons.contains(&RetentionReason::Monthly));
        assert!(file1_reasons.contains(&RetentionReason::Yearly));

        // file2: yearly (2023)
        assert!(reasons.get(&2).unwrap().contains(&RetentionReason::Yearly));
    }

    #[test]
    fn test_independent_full_chain() {
        // Test that all policies are applied independently.
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
        let reasons = select_files_to_keep_with_reasons(&files, &config, now);

        assert_eq!(keep.len(), 6);

        // file0 should have multiple reasons (keep-last + hourly)
        let file0_reasons = reasons.get(&0).unwrap();
        assert!(file0_reasons.contains(&RetentionReason::KeepLast));
        assert!(file0_reasons.contains(&RetentionReason::Hourly));

        // All files should be kept
        assert!(keep.contains(&0));
        assert!(keep.contains(&1));
        assert!(keep.contains(&2));
        assert!(keep.contains(&3));
        assert!(keep.contains(&4));
        assert!(keep.contains(&5));
    }

    #[test]
    fn test_independent_same_period_keeps_oldest() {
        // Within a period, policies keep the oldest file in that period.
        let now = Local.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let config = RetentionConfig {
            keep_daily: 2,
            ..zero_config()
        };

        // Multiple files on the same day - oldest should be kept
        let files = vec![
            make_file_info_with_time("file0.txt", now),
            make_file_info_with_time("file1.txt", now - chrono::Duration::hours(2)),
            make_file_info_with_time("file2.txt", now - chrono::Duration::hours(4)),
            make_file_info_with_time("file3.txt", now - chrono::Duration::days(1)),
        ];

        let keep = select_files_to_keep_with_datetime(&files, &config, now);

        assert_eq!(keep.len(), 2);
        assert!(keep.contains(&2)); // oldest on day 15
        assert!(!keep.contains(&0)); // not oldest on day 15
        assert!(!keep.contains(&1)); // not oldest on day 15
        assert!(keep.contains(&3)); // oldest on day 14
    }

    // ==================== RETENTION FILE CONFIG TESTS ====================

    #[test]
    fn test_resolve_config_all_defaults() {
        // No CLI args, no file config -> all defaults
        let config = resolve_config(None, None, None, None, None, None, None);
        assert_eq!(config, RetentionConfig::default());
    }

    #[test]
    fn test_resolve_config_file_values() {
        // File config values should be used when no CLI args
        let file_config = RetentionFileConfig {
            keep_last: Some(10),
            keep_hourly: Some(48),
            keep_daily: None,
            keep_weekly: Some(8),
            keep_monthly: None,
            keep_yearly: Some(5),
        };

        let config = resolve_config(None, None, None, None, None, None, Some(&file_config));

        assert_eq!(config.keep_last, 10);
        assert_eq!(config.keep_hourly, 48);
        assert_eq!(config.keep_daily, 7); // default
        assert_eq!(config.keep_weekly, 8);
        assert_eq!(config.keep_monthly, 12); // default
        assert_eq!(config.keep_yearly, 5);
    }

    #[test]
    fn test_resolve_config_cli_overrides_file() {
        // CLI args should override file config
        let file_config = RetentionFileConfig {
            keep_last: Some(10),
            keep_hourly: Some(48),
            keep_daily: Some(14),
            keep_weekly: Some(8),
            keep_monthly: Some(24),
            keep_yearly: Some(5),
        };

        let config = resolve_config(
            Some(3),  // CLI override
            None,     // use file
            Some(30), // CLI override
            None,     // use file
            None,     // use file
            Some(2),  // CLI override
            Some(&file_config),
        );

        assert_eq!(config.keep_last, 3); // CLI
        assert_eq!(config.keep_hourly, 48); // file
        assert_eq!(config.keep_daily, 30); // CLI
        assert_eq!(config.keep_weekly, 8); // file
        assert_eq!(config.keep_monthly, 24); // file
        assert_eq!(config.keep_yearly, 2); // CLI
    }

    #[test]
    fn test_resolve_config_cli_only() {
        // CLI args with no file config
        let config = resolve_config(Some(1), Some(12), Some(3), Some(2), Some(6), Some(3), None);

        assert_eq!(config.keep_last, 1);
        assert_eq!(config.keep_hourly, 12);
        assert_eq!(config.keep_daily, 3);
        assert_eq!(config.keep_weekly, 2);
        assert_eq!(config.keep_monthly, 6);
        assert_eq!(config.keep_yearly, 3);
    }

    #[test]
    fn test_retention_file_config_parse_toml() {
        let toml_content = r#"
keep-last = 10
keep-hourly = 48
keep-daily = 14
"#;
        let config: RetentionFileConfig = toml::from_str(toml_content).unwrap();

        assert_eq!(config.keep_last, Some(10));
        assert_eq!(config.keep_hourly, Some(48));
        assert_eq!(config.keep_daily, Some(14));
        assert_eq!(config.keep_weekly, None);
        assert_eq!(config.keep_monthly, None);
        assert_eq!(config.keep_yearly, None);
    }

    #[test]
    fn test_retention_file_config_empty_toml() {
        let toml_content = "";
        let config: RetentionFileConfig = toml::from_str(toml_content).unwrap();

        assert_eq!(config.keep_last, None);
        assert_eq!(config.keep_hourly, None);
    }

    #[test]
    fn test_read_retention_file_not_exists() {
        let dir = std::env::temp_dir().join("prune_backup_test_no_file");
        let _ = std::fs::create_dir(&dir);
        // Ensure no .retention file exists
        let _ = std::fs::remove_file(dir.join(RETENTION_FILE_NAME));

        let result = read_retention_file(&dir);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_read_retention_file_exists() {
        let dir = std::env::temp_dir().join("prune_backup_test_with_file");
        let _ = std::fs::create_dir_all(&dir);

        let file_path = dir.join(RETENTION_FILE_NAME);
        std::fs::write(&file_path, "keep-last = 3\nkeep-daily = 10\n").unwrap();

        let result = read_retention_file(&dir);
        assert!(result.is_ok());
        let config = result.unwrap().unwrap();
        assert_eq!(config.keep_last, Some(3));
        assert_eq!(config.keep_daily, Some(10));
        assert_eq!(config.keep_hourly, None);

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_read_retention_file_invalid_toml() {
        let dir = std::env::temp_dir().join("prune_backup_test_invalid");
        let _ = std::fs::create_dir_all(&dir);

        let file_path = dir.join(RETENTION_FILE_NAME);
        std::fs::write(&file_path, "this is not valid toml {{{{").unwrap();

        let result = read_retention_file(&dir);
        assert!(result.is_err());

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);
    }
}
