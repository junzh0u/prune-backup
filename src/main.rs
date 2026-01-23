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

#[derive(Debug)]
struct FileInfo {
    path: PathBuf,
    created: DateTime<Local>,
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

fn select_files_to_keep(files: &[FileInfo], args: &Args) -> HashSet<usize> {
    let mut keep_indices: HashSet<usize> = HashSet::new();
    let today = Local::now().date_naive();

    // 1. Keep latest N files
    for i in 0..args.latest.min(files.len()) {
        keep_indices.insert(i);
    }

    // Track which periods we've already covered
    let mut covered_days: HashSet<NaiveDate> = HashSet::new();
    let mut covered_weeks: HashSet<(i32, u32)> = HashSet::new();
    let mut covered_months: HashSet<(i32, u32)> = HashSet::new();
    let mut covered_years: HashSet<i32> = HashSet::new();

    // Calculate date boundaries
    let day_boundary = today - chrono::Duration::days(args.days as i64);
    let week_boundary = today - chrono::Duration::weeks(args.weeks as i64);
    let month_boundary = today - chrono::Duration::days(args.months as i64 * 30);
    let year_boundary = today - chrono::Duration::days(args.years as i64 * 365);

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
    let keep_indices = select_files_to_keep(&files, &args);
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
