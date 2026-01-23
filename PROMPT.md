Create a rust project for a CLI tool, named `rotate`.
It accepts an directory as param, and scan all files under the directory and read their creation dates.
Based on creations date, it preserves:
* Lastest N files
* Plus 1 file per day for D days.
* Plus 1 file per week for W days.
* Plus 1 file per month for M months.
* Plus 1 file per year for Y years.
And move all other files to trash can, which is `.trash` directory under the working directory.
Default D=10, W=4, M=12, Y=10. But make it overridable via CLI flags.
