Create a rust project for a CLI tool, named `rotate`.
It accepts a directory as param, scan all files under the directory and read their creation dates.
The following retention options are available:

* keep-last <N>
Keep the last <N> backups.
* keep-hourly <N>
Keep backups for the last <N> hours. If there is more than one backup for a single hour, only the latest is kept.
* keep-daily <N>
Keep backups for the last <N> days. If there is more than one backup for a single day, only the latest is kept.
* keep-weekly <N>
Keep backups for the last <N> weeks. If there is more than one backup for a single week, only the latest is kept.
Note that weeks start on Monday and end on Sunday. The software uses the ISO week date-system and handles weeks at the end of the year correctly.
* keep-monthly <N>
Keep backups for the last <N> months. If there is more than one backup for a single month, only the latest is kept.
* keep-yearly <N>
Keep backups for the last <N> years. If there is more than one backup for a single year, only the latest is kept.

And move all other files to trash can, which is `.trash` directory under the working directory.

Default options are:
keep-last=5, keep-hourly=24, keep-daily=7, keep-weekly=4, keep-monthly=12, keep-yearly=10
