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

#[test]
fn test_rotate_with_custom_trash_cmd() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    // Create a trash subdirectory for our custom command
    let trash_dir = dir.join("custom_trash");
    fs::create_dir(&trash_dir).expect("Failed to create trash dir");

    // Create files with different ages
    create_file_with_age(dir, "file1.txt", 0); // newest - kept
    create_file_with_age(dir, "file2.txt", 60); // older - will be trashed

    let config = RetentionConfig {
        keep_last: 1,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    // Use mv command with {} placeholder for file path
    let trash_cmd = format!("mv {{}} {}", trash_dir.display());

    let (kept, moved) =
        rotate_files(dir, &config, false, Some(&trash_cmd)).expect("rotate_files failed");

    assert_eq!(kept, 1);
    assert_eq!(moved, 1);

    // file1.txt should still be in the original directory
    assert!(file_exists(dir, "file1.txt"));
    // file2.txt should have been moved to trash_dir
    assert!(!file_exists(dir, "file2.txt"));
    assert!(file_exists(&trash_dir, "file2.txt"));
}

#[test]
fn test_rotate_with_custom_trash_cmd_handles_spaces_in_path() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    // Create a trash subdirectory
    let trash_dir = dir.join("custom_trash");
    fs::create_dir(&trash_dir).expect("Failed to create trash dir");

    // Create a file with spaces in the name
    create_file_with_age(dir, "file with spaces.txt", 60);
    create_file_with_age(dir, "keeper.txt", 0);

    let config = RetentionConfig {
        keep_last: 1,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let trash_cmd = format!("mv {{}} {}", trash_dir.display());

    let (kept, moved) =
        rotate_files(dir, &config, false, Some(&trash_cmd)).expect("rotate_files failed");

    assert_eq!(kept, 1);
    assert_eq!(moved, 1);

    assert!(file_exists(dir, "keeper.txt"));
    assert!(!file_exists(dir, "file with spaces.txt"));
    assert!(file_exists(&trash_dir, "file with spaces.txt"));
}

#[test]
fn test_rotate_with_failing_trash_cmd() {
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

    // Use a command that will fail
    let result = rotate_files(dir, &config, false, Some("false"));

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("exit code"));

    // Both files should still exist since the command failed
    assert!(file_exists(dir, "file1.txt"));
    assert!(file_exists(dir, "file2.txt"));
}

#[test]
fn test_rotate_with_trash_cmd_dry_run() {
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

    // Even with a trash_cmd, dry_run should not execute it
    let (kept, moved) = rotate_files(dir, &config, true, Some("rm")).expect("rotate_files failed");

    assert_eq!(kept, 1);
    assert_eq!(moved, 1);

    // Both files should still exist in dry run mode
    assert!(file_exists(dir, "file1.txt"));
    assert!(file_exists(dir, "file2.txt"));
}

/// This test requires a GUI environment (Finder on macOS) to move files to trash.
/// Run with: cargo test -- --ignored
#[test]
#[ignore]
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

    let (kept, moved) = rotate_files(dir, &config, false, None).expect("rotate_files failed");

    assert_eq!(kept, 2);
    assert_eq!(moved, 1);

    // Check files
    assert!(file_exists(dir, "file1.txt"));
    assert!(file_exists(dir, "file2.txt"));
    assert!(!file_exists(dir, "file3.txt")); // moved to system trash
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

    let (kept, moved) = rotate_files(dir, &config, true, None).expect("rotate_files failed");

    assert_eq!(kept, 1);
    assert_eq!(moved, 1);

    // Both files should still exist (dry run)
    assert!(file_exists(dir, "file1.txt"));
    assert!(file_exists(dir, "file2.txt"));
}

/// This test requires a GUI environment (Finder on macOS) to move files to trash.
/// Run with: cargo test -- --ignored
#[test]
#[ignore]
fn test_rotate_hourly_with_real_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let hour_secs = 3600;

    // Create files at distinct hours (using full hour offsets to avoid edge cases)
    // hour2 and hour2_b are in the same hour; hour2 is newer
    create_file_with_age(dir, "hour0.txt", 0); // now (kept by keep_last)
    create_file_with_age(dir, "hour1.txt", hour_secs); // 1 hour ago
    create_file_with_age(dir, "hour2.txt", hour_secs * 2); // 2 hours ago (newer in hour 2)
    create_file_with_age(dir, "hour2_b.txt", hour_secs * 2 + 1800); // 2.5 hours ago (older in hour 2)

    let config = RetentionConfig {
        keep_last: 1,
        keep_hourly: 5,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false, None).expect("rotate_files failed");

    assert_eq!(kept, 3); // hour0 (keep-last), hour1, hour2 (newest in hour 2)
    assert_eq!(moved, 1); // hour2_b is duplicate (older in same hour)

    assert!(file_exists(dir, "hour0.txt")); // kept by keep-last
    assert!(file_exists(dir, "hour1.txt")); // kept by hourly
    assert!(file_exists(dir, "hour2.txt")); // kept by hourly (newest in hour 2)
    assert!(!file_exists(dir, "hour2_b.txt")); // moved to system trash
}

#[test]
fn test_rotate_daily_with_real_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let day_secs = 86400;

    // Create files on different days
    // day0 and day0_b are on the same day; day0 is newer
    create_file_with_age(dir, "day0.txt", 0); // today (newest, kept by keep-last)
    create_file_with_age(dir, "day0_b.txt", 60); // also today, 1 min ago (older, kept by daily as newest un-kept)
    create_file_with_age(dir, "day1.txt", day_secs + day_secs / 2); // 1.5 days ago
    create_file_with_age(dir, "day2.txt", day_secs * 2 + day_secs / 2); // 2.5 days ago

    let config = RetentionConfig {
        keep_last: 1,
        keep_hourly: 0,
        keep_daily: 4,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false, None).expect("rotate_files failed");

    // day0.txt kept by keep-last (newest)
    // day0_b.txt kept by daily (newest un-kept on today)
    // day1.txt and day2.txt kept by daily
    assert_eq!(kept, 4);
    assert_eq!(moved, 0);

    assert!(file_exists(dir, "day0.txt")); // kept by keep-last
    assert!(file_exists(dir, "day0_b.txt")); // kept by daily (newest un-kept on today)
    assert!(file_exists(dir, "day1.txt"));
    assert!(file_exists(dir, "day2.txt"));
}

#[test]
fn test_rotate_empty_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let config = RetentionConfig::default();

    let (kept, moved) = rotate_files(dir, &config, false, None).expect("rotate_files failed");

    assert_eq!(kept, 0);
    assert_eq!(moved, 0);
}

/// This test requires a GUI environment (Finder on macOS) to move files to trash.
/// Run with: cargo test -- --ignored
#[test]
#[ignore]
fn test_rotate_skips_hidden_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    // Create two visible files and one hidden file
    create_file_with_age(dir, "visible1.txt", 0); // newest - kept by keep_last
    create_file_with_age(dir, "visible2.txt", 60); // older - will be moved
    create_file_with_age(dir, ".hidden", 0);

    let config = RetentionConfig {
        keep_last: 1,
        keep_hourly: 0,
        keep_daily: 0,
        keep_weekly: 0,
        keep_monthly: 0,
        keep_yearly: 0,
    };

    let (kept, moved) = rotate_files(dir, &config, false, None).expect("rotate_files failed");

    // visible1.txt is kept (by keep-last)
    // visible2.txt is moved
    // .hidden is skipped entirely (not counted)
    assert_eq!(kept, 1);
    assert_eq!(moved, 1);

    assert!(file_exists(dir, "visible1.txt"));
    assert!(!file_exists(dir, "visible2.txt")); // moved to system trash
    assert!(file_exists(dir, ".hidden")); // hidden file still there
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

    let (kept, moved) = rotate_files(dir, &config, false, None).expect("rotate_files failed");

    assert_eq!(kept, 1);
    assert_eq!(moved, 0);

    assert!(file_exists(dir, "file.txt"));
    assert!(dir.join("subdir").is_dir()); // subdir still there
}

#[test]
fn test_rotate_multiple_policies_with_real_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let dir = temp_dir.path();

    let hour_secs = 3600;
    let day_secs = 86400;

    // Create files kept by different policies (applied sequentially)
    create_file_with_age(dir, "recent1.txt", 0); // keep-last
    create_file_with_age(dir, "recent2.txt", 60); // keep-last
    create_file_with_age(dir, "hourly1.txt", hour_secs); // hourly
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

    let (kept, moved) = rotate_files(dir, &config, false, None).expect("rotate_files failed");

    // All 6 files kept (each by exactly one policy)
    assert_eq!(kept, 6);
    assert_eq!(moved, 0);

    assert!(file_exists(dir, "recent1.txt"));
    assert!(file_exists(dir, "recent2.txt"));
    assert!(file_exists(dir, "hourly1.txt"));
    assert!(file_exists(dir, "hourly2.txt"));
    assert!(file_exists(dir, "daily1.txt"));
    assert!(file_exists(dir, "daily2.txt"));
}
