# Ebbi: Unified Desktop Productivity Application

## Summary

Build Ebbi with Rust, Iced and SQLite, reusing useful Rust components from Habit Tracker and porting Repasito’s study behavior. Prioritize durable storage and correct historical data before expanding the interface.

Confirmed choices: redesigned dark UI; daily binary and numeric habits; minimum, maximum and inclusive-range goals; fixed review dates; topics complete only after all reviews; editable past habit entries; Dashboard as the default launch view; single-instance navigation; Markdown editor and preview; one rolling backup.

## Repository audit

Both repositories were inspected locally at commits matching upstream: Habit Tracker `85f756b`, Repasito `0e74f47`. Both working copies had pre-existing uncommitted changes, so checks cover those local copies.

| Repository | Architecture and reusable work | Verification |
|---|---|---|
| [Habit Tracker](https://github.com/Pierorivera1/Habit_Tracker) | Rust, Iced 0.13.1 and rusqlite. Separate domain, database, app state, keyboard and UI modules. Reuse CRUD patterns, keyboard mapping, widgets and canvas drawing foundations. | `cargo test --locked --offline`: 72 passed. Strict Clippy passed. Formatting check failed on one local formatting change. |
| [Repasito](https://github.com/Pierorivera1/Repasito) | C++17, Qt Quick/QML and SQLite. QML calls a backend combining UI state, scheduling and platform integration. Reuse workflow specifications and test scenarios; port behavior to Rust. | Temporary qmake build and offscreen QtTest: 22 passed, including setup and cleanup. Qt header compiler warnings occurred. |

Checks ran on Omarchy 4.0.4, Wayland, Cargo 1.98.1 and Qt 6.11.2 using isolated test data. Habit Tracker has a CI workflow; its remote status was not checked. Repasito has no CI workflow in the checked-out tree, and its Arch package references a missing `bin/build` script. The test suites do not establish migration safety or correct historical analytics.

## Problems and reuse opportunities

1. **Persistence failure:** Habit Tracker silently falls back to an in-memory database when disk initialization fails. Ebbi must show a recovery state and disable writes.
2. **Unsafe copying and import:** Habit Tracker copies database, WAL and shared-memory files independently and ignores errors. Its JSON import uses `INSERT OR REPLACE`; an isolated SQLite reproduction confirmed that replacing a habit cascades deletion to its completions.
3. **Incorrect analytics:** Habit Tracker uses recorded completion rows as the denominator. Two active habits with one recorded success show 100% instead of 50%. Its aggregate omits unrecorded dates, includes up to 31 days, and consistency always divides by 30.
4. **Mutable study history:** Repasito rewrites review dates when editing a topic; snoozing overwrites the only deadline. Several transaction and schema operation results are unchecked.
5. **Missing historical facts:** Habit Tracker stores an archive boolean without archive dates. Repasito does not preserve original deadlines or rescheduling history. Migration cannot reconstruct information the source never recorded.
6. **Responsiveness and maintenance:** Habit Tracker’s update handler is large; refreshes synchronously reload data and issue one consistency query per habit. Its chart gives observations equal spacing despite missing calendar dates.

Reuse Habit Tracker’s sound module boundaries and interaction patterns while replacing persistence and analytics behavior. Repasito’s Omarchy palette reader is useful reference material, not a first-release requirement.

## Architecture and public interfaces

**Decision: Rust + Iced + rusqlite in one Cargo package.** Keep clear modules for application state, domain rules, persistence, UI and platform integration. Avoid a plugin framework, generic repository abstraction, separate services and a multi-crate workspace.

- Domain code owns pure scheduling, eligibility, goal evaluation and analytics rules; callers pass dates explicitly.
- Persistence owns concrete SQL, migrations, imports and backups.
- Application code owns navigation, feature state, messages, pending operations and errors.
- UI renders state and emits messages; it performs no SQL.
- Platform code owns launch and single-instance routing.

Use one application-owned database worker to keep SQLite waits, imports and backups off the UI thread. Persist changes before showing final success and retain input on failure. Stop the worker when the app exits.

Iced aligns with the existing Rust architecture. Tauri offers web UI and chart tooling but adds a WebKitGTK runtime and replaces the current UI. No performance advantage is established for either option. Measure an Iced vertical slice first; consider a matched Tauri prototype only if a specific UX or performance problem justifies it.

Launch interface:

```text
ebbi --view=dashboard
ebbi --view=study
ebbi --view=analytics
ebbi --help
ebbi --version
```

Route requests to the existing process through a user-private Unix socket guarded by a process lock. A second process sends its requested view and exits. Respect Wayland restrictions on raising windows.

## UI plan

| View | First-release behavior |
|---|---|
| Dashboard | Today’s summary; habits left, tasks right; compact due/overdue study banner. Stack columns below 900 logical pixels. |
| Study | Due/overdue agenda, seven-day strip, independent search/date filters, topic details and Markdown editor/preview. |
| Analytics | 7/30/90-day trends, consistency, streaks, numeric targets and study summaries. |
| Settings | Import preview/report, backup/restore, export and storage diagnostics. |

Start at 1100×700 with 640×480 minimum; validate resizing and text scaling on Omarchy. Preserve selection by item ID. Provide keyboard-accessible chart details and text summaries.

Launch Ebbi from the application launcher or CLI. Closing terminates all Ebbi processes.

Habit entries may be corrected for eligible past dates and today; future completion is disabled. Numeric habits store one nonnegative finite value and unit per date. Calories are a single numeric entry. Changing type or unit after entries exist requires creating a new habit.

Study rules:

- Generate reviews at study date +1, +3 and +5 calendar days.
- Late completion leaves other review dates unchanged.
- Title and notes edits never reschedule reviews.
- Changing a study date previews rescheduling unfinished reviews only.
- Snoozing changes only the selected unfinished review.
- A topic completes only when all three reviews are complete; undo reopens it.

## SQLite and migration strategy

New database: `${XDG_DATA_HOME:-~/.local/share}/ebbi/ebbi.db`.

| Legacy records | Ebbi destination |
|---|---|
| Habits and completions | `habits`, effective-dated `habit_rules`, daily `habit_entries` |
| Todos | `tasks`, preserving notes, order, due dates and completion information |
| Topics and reviews | `study_topics`, `study_reviews`, review rescheduling history |
| Settings | Explicitly mapped compatible preferences |
| Original identifiers | Import batch and source-row provenance |

`habit_rules` records eligibility, active/archive intervals and numeric goals. Rules apply from a local date; changes default to tomorrow so existing outcomes remain stable. Entries are unique per habit/date. Names are labels, not identities. Reviews retain original due date where known, current due date, completion timestamp/date and schedule version `fixed-1-3-5-v1`.

Use foreign keys, constraints, WAL, a bounded busy timeout and `synchronous=FULL`. The extra durability suits small personal writes; `NORMAL` can lose committed transactions on power failure when using WAL. [SQLite durability documentation](https://www.sqlite.org/pragma.html#pragma_synchronous).

Import flow:

1. Offer detected sources and a file picker. Current candidate paths are `habit_tracker/habit_tracker.db` and `Omacom/omacalendar/omacalendar.db` under the user data directory. Personal database contents have not been inspected.
2. Read source schemas and preview counts; recognize schema/path variants instead of relying on filenames.
3. Create consistent SQLite Backup API snapshots; validate integrity, foreign keys, counts and checksums. [SQLite backup documentation](https://www.sqlite.org/backup.html).
4. Import approved snapshots in one destination transaction, preserving raw legacy values where history is uncertain.
5. Map source IDs to new IDs. Identical imports are no-ops; changed snapshots require explicit reconciliation rather than overwriting Ebbi edits.
6. Verify preserved fields and report discrepancies before commit. Unsupported or invalid records block import with an actionable report.
7. Show imported history for review before switching daily use.

Unknown archive dates and original deadlines remain unknown. Preserve records and counts, but exclude uncertain observations from exact rates with an explanation. Do not infer deadlines from current topic dates.

Use ordered transactional schema migrations and a migration ledger. Back up before upgrades and reject newer unsupported schemas. Restore into a separately validated database before replacing a closed destination.

**Backup policy:** one rolling daily snapshot, refreshed after changes while open and replaced only after validation; retain migration/import snapshots and offer “Back up now.” No background backup daemon.

## Analytics correctness

- **Daily rate:** successful eligible habits ÷ eligible habits. Include every calendar date; a zero denominator is N/A.
- **Consistency:** successful eligible dates ÷ eligible dates in the selected window.
- **Streaks:** consecutive successful eligible dates. A pending current day does not break yesterday’s streak; missed completed days do. Archiving ends the active run while preserving the historical longest run.
- **Numeric success:** evaluate the goal effective on that date. Missing values are not zero, especially for maximum goals.
- **Period comparisons:** total successes ÷ total opportunities; show percentage-point change. Compare complete periods by default and label current progress provisional.
- **Study on-time rate:** reviews completed by original local deadline ÷ reviews whose original deadline falls in the reporting period and has elapsed. Overdue incomplete reviews count as failures; future and unknown deadlines are excluded.

Queries fetch bounded date ranges and aggregate consistency together. Recalculate after relevant changes or date navigation, never during chart drawing. Charts use real calendar spacing, gaps for N/A, tooltips with counts and a fixed 0–100% percentage axis.

## Roadmap and focused tests

| Milestone | Deliverable | Acceptance gate |
|---|---|---|
| 1. Safe foundation | Ebbi package, typed domain, versioned schema, storage failure UI, snapshot/import pipeline | Synthetic legacy records survive import/reopen; failed imports roll back; retry adds no duplicates. |
| 2. Daily dashboard | Redesigned shell, binary habits, tasks, past-date editing, keyboard navigation | Daily operations persist across restart; storage waits do not freeze typing; narrow layout works. |
| 3. Study | Agenda, search, calendar strip, notes, completion and explicit rescheduling | Fixed cadence holds; overdue reviews stay visible; edits preserve completed history. |
| 4. Numeric habits and analytics | Effective-dated goals, requested charts and summaries | Hand-calculated fixtures match every metric, including unknown history. |
| 5. Desktop release | Direct launches, single instance, backups, packaging and docs | Offline Omarchy smoke test passes; exit leaves no Ebbi processes; restore succeeds. |

Concentrate automated tests in four areas: domain rules, SQLite integration, import/backup recovery, and a small app-state/CLI suite. Use table-driven dates and comparators; avoid repetitive tests, pixel snapshots and coverage quotas. Cover date boundaries, pending today, goal/archive changes, missing versus zero, out-of-order review completion, snooze/edit preservation, duplicate imports, transaction failure, corrupt/locked storage, restore and keyboard focus isolation.

CI runs formatting, strict Clippy, tests and a locked release build. Manually validate Wayland launch, keyboard focus, scaling and exit. Measure release startup, process-tree PSS/RSS, idle CPU and interaction latency with realistic and 10× stress datasets. Proposed responsiveness target: ordinary actions under 100 ms at the 95th percentile; publish measured startup/memory before setting budgets.

## Approval and limits

The owner authorized milestones 1–4 on 2026-10-15. See ../IMPLEMENTATION_STATUS.md for implemented behavior and verification; milestones 3–4 are implemented; milestone 5 is in progress: release materials and local Arch/Omarchy packaging were authorized on 2026-10-16. Single-instance routing and graphical restore were authorized and implemented on 2026-10-16. Release automation and formal benchmarks remain pending; see ../IMPLEMENTATION_STATUS.md for evidence and recovery limits.

Deferred: non-daily schedules, adaptive learning, rich document editing, automatic palette synchronization, cloud sync and background notifications.

Not checked: production database contents, release performance and interactive Wayland UX. Main risk: legacy apps omitted historical facts the migration cannot reconstruct.
