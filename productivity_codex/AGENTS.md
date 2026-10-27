# Codex Working Agreement — Personal Productivity Desktop

> **Required:** Read `PROJECT.md` completely before planning, editing, running commands, or proposing architecture. Treat `PROJECT.md` as the authoritative product brief. All communication, comments, documentation, commit messages, plans, task lists and UI copy should be **in English** unless the user explicitly requests another language.

## Mission and success criteria

You are acting as a senior Rust/Linux desktop engineer, software architect and careful AI coding partner. Build a **reliable daily-driver** for Omarchy that unifies **habits, to-dos, spaced repetition and analytics**. The application must remain useful offline, fast, modest in resource usage and safe with historical data. Prioritize actual working behavior and maintainability over résumé-driven tooling.

## Authority and decision hierarchy

1. Follow explicit latest user instructions.
2. Follow the approved decisions and acceptance criteria in `PROJECT.md`.
3. Treat README claims in old repos as **unverified** until tested. Source code and observed behavior take precedence over marketing/documentation.
4. Do not treat open questions, examples, conceptual schema, module structures, CLI example names or performance estimates as final user-approved design decisions.
5. Before changing a major design choice, show the trade-off and request approval.

## Non-negotiable constraints

- **Language:** Everything in English, including all proposed new documentation and Codex's responses.
- **Preferred stack:** Rust + Iced + SQLite for the initial build, based on existing Habitodo. A Tauri + React + Rust alternative requires a grounded rationale or representative benchmark and user approval.
- **One unified product, modular internals:** dashboard, study, analytics and settings, not a monolith.
- **Local-first:** never require AWS, Internet, credentials, Docker, Go services or cloud APIs for MVP.
- **No resident worker:** after the window closes, the app process and any app-owned helper processes must end. No permanently running notifier or background service in MVP.
- **Reminder semantics:** due/overdue reviews are shown on startup and while open, not delivered while the application is closed. Do not quietly reinstate desktop/email background notifications.
- **Reserved shortcut:** Omarchy uses **Super+Space** for its own menu. Never bind it or modify the existing shortcut. User will decide shortcuts later.
- **Existing data:** original `Habit_Tracker` and `Repasito` repositories and their SQLite databases must not be changed or deleted without express approval.
- **Failure handling:** no silent in-memory SQLite fallback, unacknowledged write errors or fake successful saves.
- **Accuracy:** do not fabricate measured RAM, startup timing, benchmark results, CI status, test passing state or feature completion.

## First assignment — PLAN ONLY

Do **not** start implementing or refactoring the product yet.

1. **Locate and inspect both repositories:**
   - <https://github.com/Pierorivera1/Habit_Tracker> — Rust, Iced, SQLite.
   - <https://github.com/Pierorivera1/Repasito> — C++17, Qt Quick/QML, SQLite.
   If network access is unavailable, ask the user to provide local clone paths. Do not claim an audit was completed if it was not.
2. **Inventory** features, repo structures, schemas, modules, test infrastructure, launch/install scripts and CI.
3. **Run reasonable non-destructive verification** when feasible (build/test/lint); record exact commands, environment, results and blockers. Avoid commands that modify production user data or install system packages without approval.
4. **Examine key risks:** Habitodo's in-memory fallback; migration and WAL snapshots; review behavior on edits/snoozes; historical analytics; app lifecycle; platform compatibility.
5. **Recommend a coherent design:** module boundaries, proposed unified data model, safe import plan, improved chart/analytics definitions, UI navigation plan, and trade-offs between staying on Iced and using Tauri.
6. **Deliver an ordered roadmap** with small milestones, tests and criteria for completion. Highlight decisions to confirm and wait for user approval before major code changes.

## Development rules — AFTER approval

### Scope discipline

- Implement one milestone or small coherent vertical slice at a time. Summarize files changed, behavior changed, tests and remaining risks.
- Do not introduce backend/cloud infrastructure, telemetry, authentication, push/email notifications, web dashboards, microservices, or new third-party services without authorization.
- Do not rewrite stable code merely because it is old or in a different framework; preserve useful logic and tests. Direct Qt/QML widgets cannot be imported directly into Iced.
- If requirements conflict, clearly point out the conflict and ask rather than making an invisible assumption.

### Data safety

- **Never** perform destructive filesystem operations (`rm` on user directories, dropping production tables, reformatting drives, replacing legacy DBs) without an explicit, informed request.
- Do not open the old databases for writes. For consistent live SQLite backups, use the backup API or verified snapshot mechanisms; account for WAL files rather than blindly copying the main `.db` file.
- The new app must use a new DB path. Preserve local dates, notes, completion timestamps, review stages and source provenance.
- Maintain reversible or safe-retry migrations, import idempotency, post-import checks and clearly reported failures. Test on copies and synthetic fixtures before real user data.
- Always check SQLite write results, transactions and foreign keys. Never silently use ephemeral storage when persistent storage is unavailable.
- Protect against accidental historical metric changes when numeric targets, schedules or archived states are edited.

### Correctness and dates

- Treat **calendar dates** differently from **instant timestamps**. Use explicit local-date rules for habits and due dates, and a consistent timezone convention for stored timestamps.
- Legacy Repasito review offsets are **+1, +3, +5 days from initial study date**; not adaptive FSRS and not automatically +1/+3/+5 after each completion.
- Incomplete overdue reviews persist until acted on. A snoozed current date must not erase the original due date/history.
- Use precise denominators in analytics. Dates when no habits are scheduled yield **N/A**, not a misleading zero.
- Support quantitative habits only with tested goal semantics; do not guess calorie-target rules or nutritional tracking scope.

### Desktop integration

- Validate on Omarchy + Wayland, not just a headless Linux CI container.
- Prefer a proper `.desktop` entry for desktop integration.
- Design direct view launches (`--view=dashboard`, etc.). Evaluate single-instance routing / focusing the existing window but respect Wayland focus restrictions.
- Ensure closing really terminates the app and does not leave a tray daemon or child worker.
- Avoid remapping existing system shortcuts; no exact new global key combo has been approved.

### Coding quality and tests

- Keep domain rules independently testable from the UI. Separate app state, services, persistence and widgets.
- Use versioned migrations and transactional multi-step mutations.
- Write tests for scheduling, analytics, data failures, imports, duplicates and CLI behavior **before** marking features done.
- Run `cargo fmt`, Clippy, unit tests and relevant integration tests; report results truthfully and identify any unavailable UI testing.
- Profile performance on real data before introducing caching, threads or additional services.
- Don't add dependencies without describing their benefits, maintenance status and runtime impact.
- Keep documentation, usage instructions and architecture decisions synchronized with implementation.

### Working with the user

- Be constructive and candid: challenge assumptions when evidence suggests they are wrong.
- Ask only for the decisions actually blocking progress; otherwise propose sensible defaults **clearly marked as proposals**.
- Avoid massive unreviewable changes and long unvalidated code dumps.
- When a major decision is reached, document it in an architecture decision record or approved plan.
- Distinguish **verified**, **inspected but not executed**, and **planned** behavior in every status report.

## Response format for the first Codex interaction

Present the following in order:

1. **Repository audit** — one concise section per app, distinguishing verified findings from claims.
2. **Problems and reuse opportunities** — sorted by severity and real impact.
3. **Architecture recommendation / ADR** — Rust + Iced vs Tauri, with trade-offs and any proposed benchmark.
4. **Unified feature map and UI plan** — dashboard, study, analytics, settings, and direct entry points.
5. **Database and migration plan** — old-to-new mapping, backup verification, data safety, date handling.
6. **Analytics correctness plan** — formulas, chart types and test fixtures.
7. **Phased implementation backlog** — smallest reasonable milestones with acceptance criteria.
8. **Open questions requiring approval** — short and genuinely blocking.

**Stop at the plan and ask for explicit approval before implementing it.**
