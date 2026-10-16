# Ebbi

Ebbi is a local-first desktop app for habits, tasks, study reviews, and personal analytics. It brings daily planning and spaced repetition into one native Linux application, with private SQLite storage and no account or network service.

The interface is built with Rust and Iced for Omarchy, Wayland, and Linux desktops. The core workflows are implemented, including import and backup tools for existing Habit Tracker and Repasito data. The app is currently built from source; login startup, single-instance view routing, packaged distribution, and in-app graphical restore are planned.

## Build and run

Requires Rust 1.89 or newer and a working Linux graphical session. On Arch, the usual Iced runtime libraries include `libxkbcommon`, Wayland and a working graphics driver; a desktop portal is needed for the optional file chooser. No account or server is required. If the chooser is unavailable, type the database path in Settings.

```sh
cargo run --locked
cargo run --locked -- --view=study
cargo run --locked -- --view=analytics
cargo run --locked -- --view=settings
cargo run --locked -- --db /tmp/ebbi-demo/ebbi.db
cargo build --release --locked
```

Default storage: `$XDG_DATA_HOME/ebbi/ebbi.db`, falling back to `~/.local/share/ebbi/ebbi.db`. `--db` accepts a new or existing Ebbi database, and rejects legacy/unrecognized schemas. A process lock prevents two Ebbi instances from opening the same database; navigation to an existing instance is a later milestone. `--help` and `--version` do not open storage.

## Daily workflow

- Dashboard: habits left, tasks right; columns stack in narrow windows.
- `a` adds a habit; `i` always opens a new task title field beneath Tasks. Enter saves; Esc cancels. With a task selected, `Enter` shows only its title input inline, just like creation (existing notes and due date are preserved); `d` opens delete confirmation (Enter confirms, Esc cancels). `j/k` select a row, `Space` completes it, and `Enter` edits it. Drag a task card onto another card to reorder it; release outside the list to cancel. Arrow keys also reorder the selected task.
- `h/l` change date; `t` returns to today. Type a date and press Enter for a specific day.
- In Settings, Tab reaches path fields, Enter submits the current path, F5/F6 choose Habit Tracker/Repasito, and F8 confirms a visible import preview. Esc cancels a preview or returns to Dashboard.
- In forms, Tab/Shift-Tab move between fields, Enter in title/due saves, and Esc cancels. Notes support multiple lines. Text inputs retain their normal editing behavior.
- Archive/restore takes effect tomorrow. Past completions remain available. Future completions and dates before tracking began are disabled.
- Tasks appear in their saved order. Completed tasks disappear from the list six hours after completion; their records stay saved. Legacy tasks without a known completion timestamp remain visible.
- Closing the window ends the app. Nothing starts at login or remains as a notification service in this build.

Saves run on a dedicated database thread. Success appears only after the transaction commits. Storage failures remain visible and retain the form; a committed save followed by a refresh failure is identified separately and disables the stale dashboard until reload.

## Study

Open **Study**, press F2, or launch `--view=study`. The Repasito-style agenda shows one compact row per review, grouped into Today (including overdue), Tomorrow, later dates and Completed. A seven-day strip starts today, shows pending-count badges (today includes overdue), and scrolls its date range when H/L goes outside it. Click a day to filter; click it again or Show all to clear the filter. Search remains independent of the date filter.

Press `n` or + Add Topic to enter a title, Markdown notes and study date in a dialog over the agenda. Reviews are generated at +1/+3/+5 days. `/` focuses search; Esc first clears search, then clears a selected date. `h/l` changes the date, `j/k` selects reviews in visible order, Space completes/undoes, `i` or Enter edits a topic, and `d` opens deletion confirmation. Hover a review for +1d snooze, reschedule, expandable notes, edit, study-date and delete actions; right-click opens editing. `r` and `s` also reach review/study-date rescheduling. Completed titles are struck through and remain available for undo.

Complete or undo each review separately; all three must be complete for the topic to finish. Editing a title or notes preserves every review. Change study date previews the new dates before confirmation and moves only unfinished reviews. Reschedule changes one unfinished review. Both preserve original deadlines and record the change; overdue reviews stay visible until acted on. Delete requires confirmation. F4 previews Markdown. Markdown preview is local; clicking a link displays its URL without opening a browser.

## Numeric habits and analytics

Press `a`, choose Numeric (F4), and enter a unit and minimum, maximum or inclusive-range goal (F5 cycles the rule). Mouse users can create habits in Settings → New habit. Kind and unit are fixed for that habit; create a new habit to change them. Record a value with its dashboard button or select it and press Space. Values must be finite and nonnegative; blank clears the entry. Missing values never count as zero. Goal changes in Settings apply tomorrow or later; the pending goal is shown and earlier entries keep their original goal.

Open **Analytics**, press F3, or launch `--view=analytics`. Select a 7/30/90-day window ending on the dashboard date. Charts use calendar spacing and 0–100% for completion; hover or click for details, use inspection buttons or Left/Right for keyboard access; keys 1/2/3 choose the range. Numeric charts show actual values and effective target bounds; missing values leave gaps.

Daily completion and consistency divide successes by known eligible opportunities. Untracked, future and unknown schedule dates are excluded; no opportunities means N/A. The current day is provisional. A pending current day preserves yesterday’s streak; archive ends the active streak while keeping the historical longest. Weekly and monthly comparisons use the last two complete periods, with percentage-point change. Study reports count due/overdue/upcoming reviews, actual completion activity and on-time completion against elapsed original deadlines. Legacy original deadlines and local completion dates that were never stored remain unknown, with explicit counts rather than guessed rates.

F1 returns to Dashboard. PageUp/PageDown scroll long views. Closing the window exits the application.

## Import

In Settings choose Habit Tracker or Repasito, browse or type its SQLite path, then preview and confirm. Detected candidates include `habit_tracker/habit_tracker.db`, `habitodo/habitodo.db`, `Omacom/omacalendar/omacalendar.db`, and `Omacom/repasito/repasito.db` under the XDG data directory.

The importer opens sources read-only, uses SQLite’s backup API to include live WAL data, validates the snapshot, rechecks the source before commit and imports in a transaction. It preserves source identifiers and raw rows, notes, dates, ordering, completion flags and timestamps. SHA-256 of canonical source rows makes identical snapshots idempotent even when copied to another path; changed previously imported sources require explicit reconciliation. Invalid records cause a visible error and no partial import. Source snapshots and pre-import destination backups remain beside the destination database.

Unknown archive dates, original review deadlines and ambiguous timestamp provenance remain unknown. Exact habit progress excludes uncertain schedules. Legacy completed tasks with unknown completion time retain both facts. Habitodo SQL creation timestamps are UTC; offset-bearing timestamps are converted to the local date for eligibility; stored calendar entry/due dates are preserved unchanged.

Only synthetic data has been used for implementation verification. Check your imported histories before switching apps. Later import of an already changed source is deliberately blocked rather than automatically merging edits; choose one app to edit that history after importing.

## Backups and recovery

One rolling snapshot is refreshed after committed changes, while Ebbi is open, at `.ebbi-backups/<full-database-filename>/daily.db` beside the database. New snapshots are verified and synced before atomic replacement. Replacement requires the same database identity and refuses active or sidecar-bearing destinations. Permanent snapshots and “Back up now” never overwrite an existing file. Files are created private to the current user.

A backup error does not undo or hide a committed save; it is reported explicitly. Preview/import operations and backups have bounded SQLite waits. Backups are SQLite databases, not JSON exports. Schema v1 upgrades transactionally to v2 after a verified permanent `pre-v2` backup. If the backup or migration fails, startup reports the error; the migration ledger and schema roll back together. Newer unsupported schemas are refused. Keep permanent pre-upgrade backups: older Ebbi binaries cannot open v2.

To inspect a backup, open a **copy** using `--db /path/to/copy.db`. To restore, close Ebbi, keep the original directory as a recovery copy including any SQLite sidecars, and place a verified backup in a new directory as `ebbi.db`; launch it with `--db`. An in-app restore workflow is deferred to the desktop release milestone. Never replace a live `.db` file or pair a restored file with old WAL/shared-memory files.

## Development

The shared contract lives in `src/domain.rs`. The app and UI contain no SQL; `src/worker.rs` serializes storage work; `src/persistence/` owns schema, queries, snapshots and import. No new abstraction layer or server is required.

The baseline dependencies follow Habit Tracker: Iced, rusqlite, chrono, serde and dirs. Added features/dependencies: rusqlite’s backup API for consistent snapshots, SHA-256 via `sha2` for import identity, and `rfd` 0.17 for async portal file dialogs. `rfd` provides the native picker integration; it is used only on request and needs a functional session portal, with typed paths available as fallback. Iced’s canvas and Markdown features supply native charts and an offline Markdown parser/preview; their transitive dependencies are recorded in the lockfile. No browser renderer or network service is used.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
TZ=America/Lima cargo test --locked
cargo build --release --locked
```

Tests focus on storage integrity, imports/rollback/idempotency, backup recovery, eligibility and small app/CLI behavior checks. No pixel snapshots, widget-count tests or coverage target. The CI workflow is provided but its remote result has not been observed.

The GitHub Actions workflow runs formatting, Clippy, tests, and a release build on pushes and pull requests. Local implementation checks have also been run on Omarchy + Wayland using synthetic data; no personal history was used.
