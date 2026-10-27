# Personal Productivity Desktop — Product & Engineering Specification

**Status:** Milestones 1–4 authorized on 2026-10-15; release materials and local Arch/Omarchy packaging authorized on 2026-10-16; see ../IMPLEMENTATION_STATUS.md for current implementation and verification  
**Document language:** English  
**Target platform:** Omarchy (Arch Linux–based), Hyprland, Wayland  
**Primary audience:** The application's owner and an AI coding agent (Codex)  
**Source projects:** [Habit Tracker / Habitodo](https://github.com/Pierorivera1/Habit_Tracker) and [Repasito](https://github.com/Pierorivera1/Repasito)

> **Decision policy:** This specification records the user's current preferences, not proof that an implementation is correct. Codex must inspect the actual code, reproduce bugs, and propose a plan before making major changes. Explicitly marked open questions remain undecided.

## 1. Product vision

Build **one fast, lightweight, native Linux desktop application** that brings three complementary productivity workflows into a coherent experience:

1. **Daily habits:** recurring actions the user wants to track consistently (reading, meditation, stretching, workouts, or a daily calorie target).
2. **Daily tasks / to-dos:** ad-hoc work and personal tasks added manually (deploy an app, clean a room, study a topic).
3. **Spaced-repetition study planning:** register what was studied, calculate upcoming reviews, check them off, and see overdue reviews (e.g., AWS Skill Builder Module 6).

The application should also deliver **genuinely useful habit and study analytics**, with reliable historical data and understandable charts—not merely a list of checkboxes. It is primarily meant for **daily personal use**, and may later grow into a polished, demonstrable Cloud/DevOps portfolio project.

### Core priorities, in order

1. **No data loss:** dependable SQLite persistence, backups and safe migration from both older apps.
2. **Fast and lightweight:** responsive UI, low memory/CPU usage, quick launch, and no idle daemon after exit.
3. **Excellent everyday UX:** uncluttered, modern, keyboard-friendly, dark-mode-first, Omarchy-compatible.
4. **Correct behavior:** deterministic review scheduling and analytics; rigorous tests for edge cases.
5. **Maintainable engineering:** clear module boundaries, documentation, CI and reproducible builds.
6. **Optional résumé value:** cloud sync, AWS, Terraform, Go, observability and containerization only if they solve a defined problem later.

**Do not over-engineer the MVP just to include trendy technologies.** A dependable daily-driver product is the first milestone.

## 2. Decisions already made

| Area | Current decision | Implications |
| --- | --- | --- |
| Number of apps | **One unified application** with separately organized feature modules and navigable views | Do not force habits, to-dos and study into the same crowded screen. |
| Primary framework | **Rust + Iced** is the preferred starting point | Existing Habitodo already uses Iced. Reevaluate only after an honest comparison with Tauri. |
| Alternative framework | Tauri + React/TypeScript + Rust remains an option | Better access to web UI/chart components; WebKitGTK overhead on Linux must be measured, not assumed. |
| Persistence | **SQLite, local-first** | Entire core product works without Internet, account, API, Docker or cloud. |
| Exit | Closing the app **terminates the process** | Do not silently minimize to a tray or keep a permanent resident worker. |
| Notifications | **Background notifications and email are removed from MVP** | Due/overdue reviews are visible when the app opens or while it is running; no alerts while closed. |
| Shortcuts | Defer choosing global shortcuts | **Super+Space already opens the Omarchy menu. Do not hijack it.** |
| Views | Support launching or navigating directly to dashboard, study and analytics | CLI deep links should be possible; exact keybindings determined later. |
| Cloud | Optional future phase only | No compulsory server, AWS resources, microservices or Terraform in v1. |
| Existing data | Preserve both apps' histories | Safe import with original databases unchanged and verifiable backups. |
| UI | Modern minimalist dark UI, inspired by Omarchy / productivity tools | Inspiration is not a request to clone Notion or a proprietary design. |

### Important behavioral distinction

- **Hidden is not closed.** The close button should actually exit. A minimized/hidden window may continue consuming RAM.
- **Offline means offline.** Habits, to-dos, reviews and charts must not depend on a server response.
- **Performance is measured.** No unsupported promises that Iced always uses less RAM or starts faster than Tauri in all cases.

## 3. Existing application audit — evidence and limits

These findings come from repository files and project documentation, **not** from an executed build/test session. Codex must verify them in the target environment.

### 3.1 Habit Tracker / Habitodo

- Repository: <https://github.com/Pierorivera1/Habit_Tracker> (default branch `main` at time of review).
- Technology: Rust 2021, **Iced 0.13.1**, `rusqlite` 0.32 (bundled SQLite), `chrono`, `serde` / JSON.
- Relevant structure: `src/domain/`, `src/db/`, `src/app/`, `src/keybindings/`, `src/ui/`, `src/main.rs` and `tests/`.
- Existing UX: daily habits and to-dos side by side; 14/30-day trend chart; monthly heatmap; consistency statistics; dark styling; Vim-like navigation; settings.
- Existing persistence: `habits`, `habit_completions`, `todos`, `settings`. WAL and foreign-key configuration are present in `src/db/schema.rs`.
- Existing persistent DB path (per repository): `~/.local/share/habitodo/habitodo.db`.
- Existing portability: JSON import/export, plus release/build scripts and desktop integration.
- Existing automation: `.github/workflows/ci.yml` contains formatting checks, Clippy, tests, and release build steps. **Do not equate the existence of a workflow with a verified green CI run.**
- Existing quality: tests, CLI flags (`--help`, `--version`, `--verify`) and keyboard-focus handling are present in code/docs; run and validate.

**Confirmed code-level concern:** `src/main.rs` attempts `Database::new_in_memory()` if opening the on-disk database fails. This can create the **appearance of saving** even though new edits disappear when the process exits. The unified product must **never silently fall back to ephemeral storage**. Offer explicit recovery, retry, or read-only/error mode instead.

**Additional migration caveat:** current habits are boolean and the present unique-name constraint (`habits.name UNIQUE`) may not fit the future design (e.g., renamed habits, same name in distinct categories). Propose deliberate schema evolution.

### 3.2 Repasito

- Repository: <https://github.com/Pierorivera1/Repasito> (default branch `master` at time of review).
- Technology: **C++17**, Qt 6 Quick/QML, Qt SQL with SQLite.
- Relevant files: `src/main.cpp`, `src/backend.cpp/.h`, `src/database.cpp/.h`, `src/Main.qml`, UI components, `src/systemtheme.cpp` and QtTest suite under `tests/`.
- Existing product features (documented): topics with Markdown notes, seven-day date strip, search, overdue review handling, deferrals, keyboard shortcuts, dark-mode integration and packaging for Arch-based Linux.
- Existing schema (source): `topics` and `reviews`, with `reviews.topic_id` foreign key; initial review dates are **+1, +3 and +5 calendar days from `initial_date`**, as coded in `Database::addTopic`.
- Existing review model: review rows include scheduled dates, review stages and completion timestamps; it is an **agenda of checkpoints**, not yet an adaptive memory model like FSRS.
- Database location: README documents `~/.local/share/Omacom/repasito/repasito.db`, while runtime location is obtained through `QStandardPaths::AppDataLocation`. **Confirm actual runtime path on the user's machine before attempting import**.
- Qt/QML widgets are not directly reusable inside Iced. Reuse product behavior, schema knowledge, algorithm tests, and design ideas; port the implementation as needed.
- The README describes integration with Omarchy themes; inspect the runtime implementation before claiming automatic palette synchronization is complete.

### 3.3 Why unify, not simply keep two apps?

Benefits of a single program:

- One place to review everything relevant today.
- A consistent look, search pattern, keyboard interaction, app launcher and data backup strategy.
- One set of cross-module analytics and fewer context switches.
- Direct entry into a particular view preserves the efficiency of two separate launcher actions.

Tradeoffs to manage:

- A single app must stay **modular**; do not turn it into a giant UI/state file.
- A larger release may take more development time and compilation time than either original.
- Regression risk is real; protect previously working features with parity tests and migration tests.
- An application-wide crash affects both workflows; prioritize error isolation and defensive programming.

## 4. MVP feature specification

### 4.1 Main daily dashboard

**Desktop composition:** habits in the **left column**, tasks/to-dos in the **right column**. A compact banner/card shows **due and overdue study reviews**. When the window is narrow, stack columns or allow a switchable layout instead of clipping content.

#### Habits

- Create, rename, reorder, archive/unarchive habits; archived habits remain in historical reports.
- Check/uncheck a habit for a selected date; changing an existing day must update reports correctly.
- Clearly distinguish **scheduled**, **completed**, **not completed**, **not applicable**, **future**, and **not yet tracked** states.
- Support two habit types:
  - **Binary:** a checkbox (read, meditate, stretch).
  - **Quantitative:** record a numeric value in a named unit (minutes, pages, kcal, etc.), compare to goal, and graph trends.
- For quantitative habits, define goal rules explicitly (at least `>=`, at most `<=`, between inclusive bounds, or manual achievement). The exact UX/defaults are **open decisions**, not assumptions; a calorie goal might have a different meaning from a reading-minutes goal.
- Reject invalid data (negative values for inherently nonnegative measures, NaN, infinite values, empty names where prohibited). Handle correction/editing.
- MVP defaults to daily scheduling, matching existing Habitodo. The design must be extensible to weekday/frequency schedules; decide whether non-daily schedules enter MVP before committing schema/UI.
- Make current-day progress legible at a glance without forcing chart navigation.

#### To-dos

- Manually create a task; edit title and notes; mark complete/incomplete; remove or archive with an appropriate confirmation/undo path.
- Optional due date; visible distinction among due today, overdue, future and unscheduled.
- Ordering/reordering, and filters (today, upcoming, all, completed), with reasonable defaults.
- Store creation and completion times; present dates in the user's local timezone.
- Do not force every task to have a date or to repeat automatically; **tasks and habits are separate concepts**.
- Use accessible controls with clear focus indicators and keyboard operation.

#### Startup overview

On manual launch, clearly surface:

- Habits scheduled today and number completed.
- Tasks due today (plus overdue tasks).
- Reviews due today (plus overdue reviews).
- A quick way to jump directly to a module without navigating from the dashboard.

**Sample use case:** The user opens Ebbi from the launcher, sees 3/5 habits, 2 overdue tasks and 1 AWS module review, performs quick actions, and closes the app. All changes are already saved locally.

### 4.2 Study / spaced-repetition view

#### Topic creation and review schedule

- Create topics with title, study date and optional Markdown notes (e.g., "AWS Skill Builder — Module 6").
- Reproduce the legacy **fixed three-checkpoint schedule** by default: R1 = study date + 1 day, R2 = +3 days, R3 = +5 days. These are **absolute offsets from the study date**, not intervals from the prior completed review.
- Each review has a scheduled local date and a nullable completion timestamp. Show stage, due date and status.
- Display today's due reviews and all overdue incomplete reviews. Late reviews must **remain visible**, not disappear or silently change their original due dates.
- Support completing/uncompleting a review, intentional snoozing/rescheduling, viewing details, editing topic title/notes, and deleting with confirmation.
- Preserve completed history, especially when the user edits the parent topic. **Never silently wipe previously completed reviews.** Define explicit reschedule rules and tests.
- Do not equate clicking "Complete" with proof of knowledge retention. It's a study scheduler, not a scientifically validated adaptive learning system.
- Once the final checkpoint is complete, categorize the topic as completed while retaining its full review history.

#### Study navigation

- Today / overdue list, chronological agenda and 7-day or month calendar.
- Title/notes search and date filtering; ideally preserve independent filters as in Repasito.
- Simple indicators for "Due today", "Overdue", "Upcoming", "Completed", and "Snoozed".
- Option to view scheduled vs actual completion dates.

#### Future adaptation, **not MVP**

- An FSRS-like adaptive scheduler, ratings (again/hard/good/easy), and repeat cycles are possible future work.
- Make schedule generation a separable domain service; do not add FSRS prematurely or pretend the existing +1/+3/+5 design is adaptive.
- Algorithm changes require explicit versioning/migration for previously created records.

### 4.3 Analytics and charts

Charts must be accurate, readable, fast, and useful enough for daily decisions. They should never be fabricated, generated from AI guesses, or confused with the task planner's scheduled data.

**Requested analytics:**

1. **Habit completion line chart** with 7-, 30-, and 90-day views; daily percentage of eligible scheduled habits completed.
2. **Per-habit consistency bars** for a configurable time window, sorted or filtered by habit.
3. **Monthly calendar heatmap** showing daily progress, with a legend and accessible tooltip/detail.
4. **Current and longest streak** for each eligible recurring habit.
5. **Quantitative trends:** actual numeric values vs target over time, unit labels, and success/failure per day.
6. **Weekly/monthly summaries** with comparisons to previous periods.
7. **Study summaries:** due, overdue and completed review counts; on-time completion rate and review activity over time.

#### Analytics definitions to implement and test

- A date is **eligible** for a habit only when that habit was active and scheduled on that date. Dates before creation, after effective archival, and unscheduled dates must not count as failures.
- **Daily completion rate** = eligible scheduled habits satisfying their completion rules / total eligible scheduled habits × 100.
- If the denominator is 0, render **N/A / no habits scheduled**, not a misleading 0% failure or 100% success.
- **Per-habit consistency** = successful eligible scheduled dates / eligible scheduled dates within the chosen range. Display numerator and denominator in tooltip/detail.
- **Streak** = consecutive eligible dates meeting goals under a documented rule; pending current day should not prematurely break a streak. Define how non-daily schedules affect continuity.
- **Heatmap** uses normalized completion proportion or an explicitly explained count; do not bias a calendar cell solely because more habits existed that month.
- **Quantitative completion** follows the saved goal comparator and goal value applicable to the historical date; editing a goal later must not silently falsify old outcomes. Decide how goal versioning is recorded.
- **Study on-time rate** = reviews completed on/before the original deadline / eligible review instances in the reporting window, with a documented policy for due-but-incomplete and not-yet-due reviews.
- Don't silently alter history when topics or habits are renamed, archived, reactivated, or rescheduled.

#### Chart UX and performance

- Chart tooltips: exact date, numeric value and numerator/denominator; units for quantitative habits.
- Explicit empty states for no data, no scheduled habits, or new users.
- Accessible labels / non-color indicators, clear y-axis, understandable percentages, no misleading truncated axes.
- Smooth scrolling, keyboard-accessible date selectors, graceful narrow-window behavior.
- Start from the existing Iced canvas chart and heatmap; either enhance them or evaluate mature Iced-compatible chart solutions. Avoid replacing the entire framework just because a chart is challenging.
- Avoid recomputing all history on every frame; use database aggregates, caching or background tasks as justified by profiling.

## 5. User interface and interaction principles

### Primary views

| View | Main content | Fast actions |
| --- | --- | --- |
| **Dashboard** | Habits left; to-dos right; compact due-reviews summary | Mark habit, input value, add/complete task, jump to reviews |
| **Study** | Review agenda, topic list, calendar, notes | Create topic, complete/snooze review, search |
| **Analytics** | Completion trends, heatmap, per-habit and study metrics | Pick date window, inspect dates, compare habits |
| **Settings** | App configuration, data import/export, preferences | Display options, backup, diagnostics |

### Interaction requirements

- Dark mode first; aesthetically consistent with a minimal Omarchy setup. An optional theme palette integration is a stretch goal, not a requirement for a working MVP.
- Keyboard-first with Vim-inspired navigation where practical (`j/k` list navigation, `Enter` action, `/` search, `Esc` cancel); **never steal keys while a text field/editor has focus**.
- Mouse support remains mandatory; do not force Vim conventions on new users.
- Preserve user focus and selection after updates; avoid disruptive full-screen rerenders.
- Render readable empty/loading/error states and undo or confirmation for destructive actions.
- The user likes a **small, fast desktop utility** rather than an always-open browser page. App does not have to mimic every Notion feature.
- Define window sizing and small-screen breakpoints based on empirical testing.

### Direct-view startup

```bash
ebbi --view=dashboard
ebbi --view=study
ebbi --view=analytics
```

The binary is `ebbi`. Repeated launches for the same canonical database path route the requested view through a private Unix socket to the existing process, then exit. Open forms and operations finish before navigation. Separate database paths remain independent. Wayland may require selecting the existing window manually.

### Startup and lifecycle in Omarchy

- Provide a standards-compliant XDG desktop entry.
- **Do not choose global hotkeys yet.** Super+Space is reserved for Omarchy's menu; never overwrite it without the user's explicit request.
- Open app -> local data loads -> due review summary is shown. No Internet required.
- Close window -> app process exits -> no app-owned background timer/worker remains.
- System or compositor services used by Linux itself are **not** app-owned daemons; avoid inaccurate "zero total system RAM" claims.
- If reminders are desired later, propose explicit alternatives (app-open alerts vs scheduled systemd user timer vs cloud/email), each with resource/privacy trade-offs. **Do not silently add a daemon to MVP.**

## 6. Architecture proposal

### Preferred MVP stack

| Layer | Preferred technology | Rationale |
| --- | --- | --- |
| UI | Rust + **Iced** (validate compatible version) | Build on existing Habitodo; native Rust domain and UI |
| Domain | Rust modules, pure business logic | Deterministic scheduling and analytics; testable without GUI |
| Storage | SQLite through `rusqlite` | Portable, fast, local, ACID with no server |
| Dates | `chrono` or other deliberate, tested Rust time library | Explicit calendar dates versus instants and timezones |
| Data interchange | Versioned JSON export/import (optional CSV exports) | Backup portability |
| Build/tests | Cargo, rustfmt, Clippy, unit/integration tests | Established Rust tooling and existing Habitodo workflow |
| Release | Binary + `.desktop` entry; optionally Arch PKGBUILD later | Comfortable installation and launcher integration |
| Cloud | **None in MVP** | No startup latency, account dependency or running costs |

### Logical components

```text
Desktop shell (window, keyboard/mouse, navigation, error UI)
    |
    +-- Dashboard view    -> Habits service + Tasks service
    +-- Study view        -> Study scheduler / review service
    +-- Analytics view    -> Analytics queries / aggregation service
    +-- Settings view     -> Preferences, import/export, diagnostics
    |
Application orchestration / state transitions / async operations
    |
Domain services (pure rules, validations, interfaces)
    |
Persistence repository / versioned SQLite migrations / backup / import
    |
Single local SQLite database
```

This is a **conceptual layout**, not a mandatory exact directory tree. Reuse existing Habitodo modules where they are sound and move rather than rewrite functionality whenever it reduces risk.

### Candidate repository layout (to be refined by Codex)

```text
productivity-desktop/
├── AGENTS.md
├── PROJECT.md
├── README.md
├── Cargo.toml                 # or a small Cargo workspace
├── src/
│   ├── main.rs
│   ├── app/                   # state, messages, routing, lifecycle
│   ├── domain/
│   │   ├── habits/
│   │   ├── tasks/
│   │   ├── study/
│   │   └── analytics/
│   ├── persistence/           # schema, migrations, repositories, importer
│   ├── ui/
│   │   ├── dashboard/
│   │   ├── study/
│   │   ├── analytics/
│   │   └── settings/
│   └── platform/              # desktop integration, optional instance IPC
├── migrations/
├── tests/
├── packaging/
└── .github/workflows/
```

### Persistent storage rules

- SQLite database under an appropriate XDG data directory, such as `${XDG_DATA_HOME:-$HOME/.local/share}/<app-name>/app.db` (final app name TBD).
- WAL and foreign-key enforcement where supported and appropriate. Enable busy timeout; use explicit transactions for multi-row operations.
- Do not silently continue in an **in-memory database** if durable storage initialization fails.
- Surface database busy/corruption/permission/full-disk errors and offer safe, comprehensible recovery paths.
- Startup migrations are transactional where possible, versioned and tested against backups; do not discard older data.
- Data changes should be persisted before the UI shows final success (or use clear optimistic-update rollback handling).
- Never assume backups can be made safely with a plain file copy while WAL writers are active; use SQLite's backup API / consistent snapshot procedures or close writers appropriately.
- Preserve local privacy: no telemetry or uploading notes/habits by default.

## 7. Proposed data model (conceptual, not final SQL)

Codex should derive a normalized schema from real feature requirements and old schemas, not blindly paste the following into production.

| Entity | Important fields | Notes |
| --- | --- | --- |
| `habits` | ID, name, type (binary/numeric), unit, creation timestamp, active/archived dates, position, schedule ID | Prefer stable IDs. Rename must preserve all entries. |
| `habit_schedules` | habit ID, schedule rule, effective-from/to date | Can be simplified in daily-only MVP; plan for future recurrence. |
| `habit_goals` | habit ID, comparator, target(s), effective-from/to date | Historical goals should be interpretable. |
| `habit_entries` | habit ID, **local date**, completed flag or numeric value, update timestamp | Unique `(habit_id, local_date)`; distinct from absence. |
| `tasks` | ID, title, notes, due local date, created/completed timestamps, display order, status | Tasks are not habits. |
| `study_topics` | ID, title, Markdown notes, initial study local date, created timestamp, status | Preserve original study date. |
| `study_reviews` | ID, topic ID, stage, **original due date**, current scheduled date, completion timestamp, schedule version | Supports auditability, overdue history and snoozing. |
| `settings` | Key/value or typed configuration | Display settings and data policies. |
| `schema_migrations` | Migration version and applied timestamp | Audit/upgrade safety. |
| `legacy_imports` | Source app, fingerprint/import ID, imported timestamp, counts | Prevent duplicate imports and support reporting. |

### Date/time rules

- Calendar items (a habit for "today", study dates, review due dates) use **local calendar dates**; timestamps for mutations/completions are instants stored with an explicit timezone convention (prefer UTC ISO-8601 / epoch) and displayed locally.
- Avoid treating a **calendar day** as exactly 24 hours; daylight-saving and timezone travel create exceptions.
- Midnight rollover: while the app stays open, update today's view without losing unsaved input.
- Import legacy date strings carefully; don't shift dates by UTC conversion errors.
- Explicitly test the current day, future dates, negative time differences, end of month/year, leap days and timezone changes.

## 8. Migration strategy — mandatory before replacing current apps

The old applications already store potentially valuable daily history. The unified app must not destroy it.

**Required sequence:**

1. **Inspect** both schema versions and inspect a safe sample of existing data, with the user's permission when local files are involved.
2. **Locate** actual DB paths; do not rely solely on README. Account for `XDG_DATA_HOME` and Qt application naming/path behavior.
3. **Back up** source databases using a consistent SQLite snapshot, including WAL considerations; obtain a checksum and record row counts.
4. **Create a new database** in a **new path**. Do not have the new app point writes at either legacy DB.
5. **Define mapping:** Habitodo `habits`, `habit_completions`, `todos` → new habit/task entities; Repasito `topics`, `reviews` → new study entities.
6. **Import transactionally**, preserving dates, original IDs as provenance when useful, notes, status, completion history and ordering.
7. **Prevent duplicates:** keep import provenance/fingerprints or mapping tables; rerunning import must not duplicate items.
8. **Validate** counts, key fields, uniqueness, foreign keys, representative histories and derived summary parity; explain intentional differences caused by corrected analytics rules.
9. **Failure safety:** rollback new import completely, leave source databases unchanged and show clear error messages.
10. **User verification:** preview imported habits/topics/tasks and receive approval before treating the new app as the daily replacement.

**Do not:** move/delete old databases, mutate original repos, overwrite legacy installation, or auto-delete backup files. A JSON export is an extra recovery mechanism, not a substitute for correct SQLite snapshots.

## 9. Performance validation: Iced versus Tauri

**Default:** Keep Iced because substantial Rust/Iced code already exists and it suits the requirement. Tauri is a **competitor to test**, not a promised speedup or forbidden technology.

### Why a comparison may be warranted

- Iced avoids using a WebView for its UI and makes reuse from Habitodo straightforward.
- Tauri + React may make complex dashboards, CSS theming and ready-made charts faster to develop, but on Linux uses WebKitGTK and introduces additional runtime/dependency behavior.
- Iced may incur GPU allocations for rendering; platform/backend versions affect resource usage. A simple screenshot or idle RSS measurement proves little.

### Fair benchmark protocol

- Use **the same laptop, Omarchy session, power profile, resolution and dataset** for each prototype.
- Match functionality: same number of habits and tasks; same 30/90-day chart workload; identical sample data and equivalent UI interaction.
- Run **cold start** and **warm start** multiple times; report median and variation.
- Record **PSS** where possible (captures shared-memory attribution) and **RSS** with caveats, idle CPU after UI settles, startup duration, navigation latency, chart redraw time and input responsiveness.
- Test a realistic dataset (e.g., 10–30 habits, 200 tasks, 100–1,000 study reviews, at least one year of daily history) and a larger stress dataset.
- Record Wayland/X11 backend, GTK/WebKitGTK versions where applicable, GPU backend and compiler build profile.
- Prefer a reproducible script/procedure over marketing claims. Do not fabricate numbers or impose unjustified absolute RAM thresholds.
- Make the framework decision using **both** measured UX/performance and long-term maintenance/design productivity.

If the existing Iced implementation is sound, prioritize improvements over an expensive full rewrite.

## 10. Testing and quality assurance

### Domain tests

- Toggle binary habit on/off; historical edits; archived dates; zero scheduled habits; overlapping schedules; changed goals.
- Numeric habits: at-least/at-most/between semantics, zero values, invalid numeric inputs, units and historical goal changes.
- Streak behavior for scheduled days, current-day pending status, holidays/unscheduled dates and month boundaries.
- Study scheduling: +1/+3/+5 offsets, leap years, year boundary, overdue review, snooze, completed review edit, undo and completion order.
- To-do creation, editing, due-date filters, deletion/undo, and completion timestamp consistency.
- Every analytics definition with small hand-calculated fixtures; separate query and presentation tests.

### Storage / migration tests

- First launch and schema creation; upgrade from multiple historical schema versions.
- Corrupt DB, locked DB, read-only directory, missing directory and simulated disk-full failure.
- Import of both legacy schemas, idempotent retry, interrupted import, duplicate names, null/blank dates, Unicode, large notes and foreign-key integrity.
- Round-trip backup/export/import, restored row counts and chart parity.
- Verify the app **never silently creates an ephemeral working state** after persistent DB failure.

### Desktop and UX tests

- Wayland launch and close; window resizing; navigation with mouse and keyboard; input focus isolation.
- Manual launcher startup; closed-process termination; no duplicate instances after direct-view requests.
- Launch `--view=dashboard`, `--view=study`, `--view=analytics` after the implementation exists; validate invalid arguments.
- Clear overdue indicators and correct handling of midnight/day rollover.
- Human review of UI spacing, charts, tooltips, readable text, accessibility and light/dark fallback behavior.

### CI / release

- `cargo fmt --all -- --check`, strict `cargo clippy`, unit/integration tests, release build.
- Add reproducible packaging checks, installer smoke tests and GitHub release artifacts when practical.
- Document exact commands, OS dependencies and known platform limits. **A green CI job alone does not prove real Wayland behavior; test on Omarchy.**

## 11. Delivery milestones (with gates)

### Phase 0 — Repo audit and decisions (NO app rewrite yet)

**Deliver:** repository inventory, builds/tests status, confirmed runtime data paths, known bugs, reusable code map, Iced-versus-Tauri architecture decision record (ADR), UX wireframe and implementation task breakdown.

**Approval gate:** user reviews and approves architecture and data migration approach.

### Phase 1 — Stable data foundation

**Deliver:** fresh repository/workspace plan, domain model, SQLite v1 schema + versioned migrations, safe import prototype, explicit storage-error handling and backup/recovery tests.

**Done when:** legacy data round-trips in tests and failure never silently loses edits.

### Phase 2 — Unified daily dashboard

**Deliver:** habit CRUD/checkbox logic, to-do CRUD, modern responsive two-column interface, keyboard navigation, summary metrics, persistence.

**Done when:** it can replace Habitodo for daily usage with representative migrated data.

### Phase 3 — Study module

**Deliver:** topic CRUD, +1/+3/+5 review generation, overdue list, completion/snooze, notes, search, calendar, startup summary.

**Done when:** it can replace Repasito without losing study history.

### Phase 4 — Advanced habits and analytics

**Deliver:** quantitative goals, precise metric definitions, 7/30/90-day trend charts, consistency, streaks, heatmap, study metrics and tests.

**Done when:** all chart values are verified against hand-calculated fixtures and performance is acceptable.

### Phase 5 — Desktop polish and release

**Deliver:** direct-view CLI, practical single-instance behavior if feasible, Arch/Omarchy installation, README, screenshots, sample dataset, benchmark report and reliable CI/release workflow.

**Done when:** usable offline every day and the app exits completely when closed.

### Phase 6 — OPTIONAL cloud / DevOps portfolio extension (after stable local release)

Potential architecture: Rust desktop client + explicit sync layer + Go API + managed PostgreSQL + AWS infrastructure defined in Terraform; Docker for backend development/testing, GitHub Actions CI/CD, CloudWatch, authentication, observability and a well-tested sync protocol.

**Cloud design guardrails:** make sync optional, protect offline data, use authenticated transport, idempotent writes, conflict strategy, server-side schema migrations, cost budget/alerts and secure secrets. Compare serverless and container hosting rather than adopting microservices reflexively. Cloud should demonstrate engineering outcomes, not merely named services.

## 12. Acceptance criteria for MVP

- [ ] Works on the user's Omarchy / Wayland machine with no Internet.
- [ ] Unifies habits, to-dos and spaced-repetition study behind navigable views.
- [ ] Dashboard shows habits on the left and manually managed tasks on the right at normal width.
- [ ] Study reviews scheduled at +1/+3/+5 from the initial study date; due and overdue reviews never disappear silently.
- [ ] Users can open the app manually from the launcher and see today's summary.
- [ ] Closing the window terminates the app; no app-owned resident daemon or background notification process remains.
- [ ] Data persists after full restart and storage problems are explicit and recoverable.
- [ ] Existing Habitodo and Repasito data can be imported with verified integrity, safely and without altering the originals.
- [ ] Analytics definitions are documented and graphs reproduce correct fixture values.
- [ ] Keyboard-first UX works without interfering with text input.
- [ ] New binary provides direct navigation to main views; no Omarchy-reserved keybinding is overwritten.
- [ ] Tests pass locally and in CI; release instructions and current limitations are documented.

## 13. Open questions (do not silently choose without approval)

1. Final **application name** and visual design direction: evolve Habitodo's UI or substantially redesign while reusing its Rust domain logic?
2. Which quantitative goal semantics are needed initially: minimum, maximum, target range, exact number and/or custom rule? Are calories tracked as a simple numeric value or via a full food diary? **Do not build a nutrition database without asking.**
3. Do non-daily habit schedules belong in MVP or a later iteration?
5. For habits, should the user be able to mark yesterday complete or edit dates arbitrarily? Recommended yes, but confirm UX.
6. For the study system, should completed late reviews leave the next checkpoints on their original dates (legacy behavior) or shift them? **Keep original legacy behavior by default until explicitly decided.**
7. Resolved on 2026-10-16: implement private Unix-socket view routing; respect Wayland focus restrictions.
8. Whether study notes need full rich Markdown rendering or a simple Markdown editor/preview first.
9. Whether automatic backups, export/import preferences should be visible in the first release settings screen.
10. Later: whether optional sync should cover multiple Linux computers or other platforms, and whether any email notification feature is still desired.

## 14. Initial instructions to Codex

**First response must be a review and implementation plan, not a code dump.**

1. Read this document and `AGENTS.md` completely.
2. Audit both upstream repos, with a clear distinction between documented behavior, inspected code and successfully executed tests.
3. Identify immediate blockers, data-risk bugs, feature parity gaps, performance questions, safe-to-reuse modules and what must be ported.
4. Produce a short ADR about Iced vs Tauri, a preliminary domain/schema proposal, import/rollback design, view/navigation plan and prioritized implementation roadmap with acceptance tests.
5. Explicitly list decisions that require the user's approval. Do not change source code, legacy databases or the user environment until approval.

**Last updated:** 2026-10-15
