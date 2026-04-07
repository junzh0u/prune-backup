# Changelog

## Unreleased

- Adopt Proxmox-style retention semantics: keep newest (latest) file per period instead of oldest, apply policies sequentially (each policy only considers files not already kept by a previous policy)
- Replace Makefile with justfile

## 0.1.3 — 2026-02-18

- Count only periods with actual backup files in retention policies (gaps no longer consume slots)
- Use musl target for portable Linux binary
- Remove extra blank line in startup output

## 0.1.2 — 2026-01-27

- Apply retention policies independently instead of cascading
- Print retention policy at startup

## 0.1.1 — 2026-01-24

- Add `--trash-cmd` option for custom file removal commands
- Add `.retention` config file support (TOML format)
- Add example retention configuration file
- Use native system trash API instead of `.trash` directory

## 0.1.0 — 2026-01-23

- Initial release
- Retention policies: keep-last, keep-hourly, keep-daily, keep-weekly, keep-monthly, keep-yearly
- Dry-run mode
- Keep oldest file per period, enforce keep-last >= 1
