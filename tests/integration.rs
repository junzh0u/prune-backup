use filetime::{set_file_mtime, FileTime};
use prune_backup::{rotate_files, RetentionConfig};
use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn create_file_with_age(dir: &Path, name: &str, age_secs: u64) {
    let path = dir.join(name);
    File::create(&path).expect("Failed to create file");

    let mtime = SystemTime::now() - Duration::from_secs(age_secs);
    let file_time = FileTime::from_system_time(mtime);
    set_file_mtime(&path, file_time).expect("Failed to set file mtime");
}

fn file_exists(dir: &Path, name: &str) -> bool {
    dir.join(name).exists()
}

fn trash_exists(dir: &Path, name: &str) -> bool {
    dir.join(".trash").join(name).exists()
}

#[test]
fn test_rotate_keeps_recent_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    // Create 3 files with different ages
    create_file_with_age(dir, "file1.txt", 0); // now
    create_file_with_age(dir, "file2.txt", 60); // 1 minute ago
    create_file_with_age(dir, "file3.txt", 120); // 2 minutes ago

    let config = RetentionConfig {
        keep_last: 2,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false).expect("rotate_files failed");

    assert_eq!(kept, 2);
    assert_eq!(moved, 1);

    // Check files
    assert!(file_exists(dir, "file1.txt"));
    assert!(file_exists(dir, "file2.txt"));
    assert!(!file_exists(dir, "file3.txt"));
    assert!(trash_exists(dir, "file3.txt"));
}

#[test]
fn test_rotate_dry_run_does_not_move_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    create_file_with_age(dir, "file1.txt", 0);
    create_file_with_age(dir, "file2.txt", 60);

    let config = RetentionConfig {
        keep_last: 1,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, true).expect("rotate_files failed");

    assert_eq!(kept, 1);
    assert_eq!(moved, 1);

    // Both files should still exist (dry run)
    assert!(file_exists(dir, "file1.txt"));
    assert!(file_exists(dir, "file2.txt"));
    // Trash dir should not exist
    assert!(!dir.join(".trash").exists());
}

#[test]
fn test_rotate_creates_trash_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    create_file_with_age(dir, "file1.txt", 0);
    create_file_with_age(dir, "file2.txt", 60);

    let config = RetentionConfig {
        keep_last: 1,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    rotate_files(dir, &config, false).expect("rotate_files failed");

    assert!(dir.join(".trash").is_dir());
}

#[test]
fn test_rotate_hourly_with_real_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let hour_secs = 3600;

    // Create files at distinct hours (using full hour offsets to avoid edge cases)
    // We use 2+ hour differences to ensure files are in different hours
    create_file_with_age(dir, "hour0.txt", 0); // now
    create_file_with_age(dir, "hour0_b.txt", 60); // 1 min ago (same hour as hour0)
    create_file_with_age(dir, "hour2.txt", hour_secs * 2); // 2 hours ago
    create_file_with_age(dir, "hour3.txt", hour_secs * 3); // 3 hours ago

    let config = RetentionConfig {
        keep_last: 0,
        keep_hourly: 5,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false).expect("rotate_files failed");

    assert_eq!(kept, 3); // 3 unique hours (0, 2, 3 hours ago)
    assert_eq!(moved, 1); // hour0_b is duplicate

    assert!(file_exists(dir, "hour0.txt"));
    assert!(file_exists(dir, "hour2.txt"));
    assert!(file_exists(dir, "hour3.txt"));
    assert!(trash_exists(dir, "hour0_b.txt"));
}

#[test]
fn test_rotate_daily_with_real_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let day_secs = 86400;

    // Create files on different days
    create_file_with_age(dir, "day0.txt", 0); // today
    create_file_with_age(dir, "day0_b.txt", 3600); // also today
    create_file_with_age(dir, "day1.txt", day_secs); // yesterday
    create_file_with_age(dir, "day2.txt", day_secs * 2); // 2 days ago

    let config = RetentionConfig {
        keep_last: 0,
        keep_hourly: 0,
        keep_daily: 3,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false).expect("rotate_files failed");

    assert_eq!(kept, 3); // 3 unique days
    assert_eq!(moved, 1); // day0_b is duplicate

    assert!(file_exists(dir, "day0.txt"));
    assert!(file_exists(dir, "day1.txt"));
    assert!(file_exists(dir, "day2.txt"));
    assert!(trash_exists(dir, "day0_b.txt"));
}

#[test]
fn test_rotate_empty_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let config = RetentionConfig::default();

    let (kept, moved) = rotate_files(dir, &config, false).expect("rotate_files failed");

    assert_eq!(kept, 0);
    assert_eq!(moved, 0);
}

#[test]
fn test_rotate_skips_hidden_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    create_file_with_age(dir, "visible.txt", 0);
    create_file_with_age(dir, ".hidden", 0);

    let config = RetentionConfig {
        keep_last: 0,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false).expect("rotate_files failed");

    // visible.txt is not kept (no retention config)
    // .hidden is skipped entirely
    assert_eq!(kept, 0);
    assert_eq!(moved, 1);

    assert!(!file_exists(dir, "visible.txt"));
    assert!(file_exists(dir, ".hidden")); // hidden file still there
    assert!(trash_exists(dir, "visible.txt"));
}

#[test]
fn test_rotate_skips_directories() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    create_file_with_age(dir, "file.txt", 0);
    fs::create_dir(dir.join("subdir")).expect("Failed to create subdir");

    let config = RetentionConfig {
        keep_last: 1,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false).expect("rotate_files failed");

    assert_eq!(kept, 1);
    assert_eq!(moved, 0);

    assert!(file_exists(dir, "file.txt"));
    assert!(dir.join("subdir").is_dir()); // subdir still there
}

#[test]
fn test_rotate_handles_name_conflicts_in_trash() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    // Create trash with existing file
    fs::create_dir(dir.join(".trash")).expect("Failed to create trash dir");
    File::create(dir.join(".trash").join("file.txt")).expect("Failed to create file in trash");

    // Create file to be rotated
    create_file_with_age(dir, "file.txt", 0);

    let config = RetentionConfig {
        keep_last: 0,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false).expect("rotate_files failed");

    assert_eq!(kept, 0);
    assert_eq!(moved, 1);

    // Original file in trash still exists
    assert!(trash_exists(dir, "file.txt"));
    // New file renamed to avoid conflict
    assert!(trash_exists(dir, "file_1.txt"));
}

#[test]
fn test_rotate_cascading_with_real_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let hour_secs = 3600;
    let day_secs = 86400;

    // Create files that test cascading behavior
    create_file_with_age(dir, "recent1.txt", 0); // keep-last
    create_file_with_age(dir, "recent2.txt", 60); // keep-last
    create_file_with_age(dir, "hourly1.txt", hour_secs); // hourly (not covered by keep-last)
    create_file_with_age(dir, "hourly2.txt", hour_secs * 2); // hourly
    create_file_with_age(dir, "daily1.txt", day_secs); // daily
    create_file_with_age(dir, "daily2.txt", day_secs * 2); // daily

    let config = RetentionConfig {
        keep_last: 2,
        keep_hourly: 3,
        keep_daily: 3,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false).expect("rotate_files failed");

    // All 6 files should be kept by different policies
    assert_eq!(kept, 6);
    assert_eq!(moved, 0);

    assert!(file_exists(dir, "recent1.txt"));
    assert!(file_exists(dir, "recent2.txt"));
    assert!(file_exists(dir, "hourly1.txt"));
    assert!(file_exists(dir, "hourly2.txt"));
    assert!(file_exists(dir, "daily1.txt"));
    assert!(file_exists(dir, "daily2.txt"));
}
