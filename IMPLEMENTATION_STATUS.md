# Ebbi implementation status

## Confirmed habit deletion — 2026-10-19

On Dashboard, `d` now opens a compact modal for the selected habit: “Do you want to delete a habit?” with its name and Yes/No buttons. Yes or `y` deletes the habit and its binary/numeric history; No, `n`, or Escape cancels. Repeated `d` and unfocused Enter do not confirm. Task deletion keeps its existing confirmation. All habit child records and the habit are deleted in one transaction through the database worker; write failures retain the form and report the error.

Formatting, strict all-target Clippy, and all 33 tests passed offline. Added regressions cover confirmation/cancellation, task shortcut routing, persisted binary/numeric deletion, unrelated data preservation, foreign keys and rollback when deletion fails. Native visual interaction was not checked in this session.

Built and installed the local Arch package from `/home/pierooo/.cache/ebbi-habit-delete.NkgvcXi2/`; package checks passed. The running app must be reopened to load the updated executable.

Updated: 2026-10-16. The owner authorized milestones **1–4**. Storage, the daily dashboard, Study, and numeric habits/analytics are implemented. The owner authorized release materials and local Arch/Omarchy packaging on 2026-10-16; milestone 5 is in progress. The design reference is `productivity_codex/IMPLEMENTATION_PLAN.md` and the Ebbi notes in LLM-wiki.

## Implemented

- Native Rust/Iced app with Dashboard, Study, Analytics and Settings, dark palette and responsive dashboard. CLI accepts all four views. Database work runs on one owned worker; closing ends the process.
- Daily binary habits with historical eligible check-ins, rename/reorder and archive/restore effective tomorrow. The compact dashboard retains the owner's requested title-only task creation/editing, saved task order, drag/arrow reordering and confirmed deletion. Completed tasks hide after six hours; unknown legacy completion times remain visible. Dashboard add buttons and task dropdown remain removed; Settings offers mouse access to habit creation.
- Numeric habits with immutable kind/unit, finite nonnegative values and minimum/maximum/inclusive-range goals. Clearing a value preserves the difference between missing and zero. Goal changes apply after today and retain earlier goal definitions; Settings shows the pending goal. Open value forms capture their entry date; writes use the actual local date when executed.
- Study topic CRUD, title/notes search, Repasito-style grouped review rows, independent search/day filtering and seven-day strip. Fixed +1/+3/+5 reviews, complete/undo, single-review rescheduling and confirmed unfinished-review rescheduling after study-date changes. Metadata edits preserve review dates; completion requires all three stages. Original deadlines, completed rows and rescheduling audit records are preserved. Markdown editor and offline preview use Iced's built-in feature.
- Analytics: 7/30/90-day daily completion trends with calendar spacing and N/A gaps; per-habit consistency bars; current/all-time longest streaks; numeric actual/target charts; complete-week/month comparisons; study due/overdue/upcoming, actual completion activity and on-time rates. Hover details, explicit day inspection and keyboard shortcuts expose counts and units. Reports are computed off the UI thread. Streaks scan recorded dates instead of every historical calendar day.
- SQLite v2 with transactional v1 upgrade, permanent verified pre-upgrade backup, migration ledger and identity checks. Fresh databases create v1 then upgrade within ordered transactions. Private new files, foreign keys, WAL, FULL synchronization, bounded waits and exclusive process lock remain. Retry releases the existing database before reopening.
- Read-only legacy snapshots/imports, raw provenance, copied-source alias deduplication, changed-source refusal, transactional validation and verified permanent/manual/rolling backups. Unknown legacy schedules/deadlines/local completion dates remain unknown. Post-commit refresh and backup errors explicitly distinguish saved changes from failed reads/backups.

## Coordination

Back was idle before this work. The workspace is not a Git repository; a worktree was unnecessary. Agreed ownership before edits: Back owned domain types, pure analytics, persistence and storage tests; Main owned app/UI/worker/main, Cargo, documentation and wiki. Back handed all files back before Main integrated fixes and then reviewed read-only. No overlapping file edits were required.

Review fixes cover midnight entry/goal dates, reopening the process's own database lock, reschedule preview validation/whitespace, mouse access through Settings and bounded streak calculation. Native checking also exposed Iced retaining input focus after a form disappears; successful save/cancel now releases input focus. PageUp/PageDown scroll the main view, and navigation/forms start at the top.

## Verification

Environment: Omarchy 4.0.4 / Wayland, Cargo 1.98.1; synthetic temporary data only.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked --offline -- -D warnings
TZ=America/Lima cargo test --locked --offline
cargo build --release --locked --offline
```

**20 tests passed across the suites:** seven storage cases, four study/numeric/analytics/migration cases, nine app/worker/CLI cases. The latest UI change reran the nine app cases; the unchanged storage suites passed in the milestone checks. Strict Clippy and release build passed. The new suite concentrates on comparator boundaries/missing-vs-zero, leap/year dates, completed review preservation, reschedule rollback/audit, on-time unknowns, hand-calculated rates/streaks/complete-period comparisons, extreme historical dates, and v1 backup/upgrade rollback. Existing tests cover imports, aliases, WAL snapshots, backups, errors and task interactions. App checks cover preview confirmation/input retention, search, frozen entry dates and Retry.

Native Wayland checks in `/tmp/ebbi-m34-01x8f71d/`:

- Upgraded a synthetic v1 database, preserving its existing history and creating a permanent v1 snapshot.
- Created a topic with multiline Markdown through the UI; verified +1/+3/+5 dates and exact notes in SQLite. Inspected rendered Markdown preview.
- Completed reviews using keyboard selection; rescheduled one unfinished review, then previewed/confirmed a changed study date with whitespace. Verified completed dates and original deadlines unchanged, only unfinished dates moved, and both audit rows persisted.
- Created a numeric habit and entered a successful value through the UI; checked its goal/value in SQLite and its dashboard completion count. Keyboard navigation after save worked.
- Opened Analytics and inspected the calendar-aligned percentage chart, normalized monthly heatmap, consistency/streak bars, numeric actual/target plot and complete-period comparisons. Verified PageDown scrolling. Closed/reopened the app and retained synthetic study history.

Prior native smoke checks covered habit/task CRUD, typed-path legacy preview/import, NULL completion-time preservation, stacked/two-column layouts and process exit. No personal history has been imported or inspected.

## Remaining limits

- Milestone 5: release automation and formal performance validation remain. Local packaging, single-instance view routing and graphical backup restore are implemented; see the latest execution record below.
- No production migration/import, remote CI run, real disk-full/power-loss hardware injection, native portal picker interaction or formal startup/memory/latency benchmark was performed.
- Legacy deadlines/archive history and local completion dates absent from the source cannot be reconstructed. Reports disclose unknowns. Changed imported sources require explicit reconciliation; subsequent automatic merging is not implemented.
- Markdown preview supports Iced's built-in subset (headings, emphasis, lists and code); rich document editing and embedded images remain outside scope. Non-daily habits, adaptive repetition and cloud sync remain deferred.

## Study agenda redesign — 2026-10-15

F2 now follows the owner's Repasito screenshot: monospace header/date/search, neutral seven-day strip, compact checkbox/title/stage/date rows, red overdue badges, grouped Today/Tomorrow/later/Completed sections and muted struck-through completed titles. Day clicks toggle filtering independently of search. Hover reveals actions and expandable Markdown notes; add/edit/delete forms open over the agenda. I and Enter edit, N creates, H/L changes day, J/K navigates the same sorted rows, Space completes/undoes, D confirms deletion and Esc clears search/date filtering. Original deadlines and history behavior use the existing persistence API.

Nine app tests, strict all-target Clippy and release build passed. A focused agenda regression checks pending/completed ordering, today including overdue, exact future-day filtering and independent search/Escape behavior. Native synthetic Wayland checks compared the wide layout with the reference and exercised completion/undo, date navigation and edit-dialog opening. Data fixture/screenshots: `/tmp/ebbi-repasito-e2wlo4kh/`. Production data and the original Repasito files were not modified.

The owner requested removing the Analytics monthly heatmap. Its calendar, legend, inspection shortcuts and extra worker report have been removed.

## Release materials and local packaging — 2026-10-16

The owner authorized the first two desktop-release items: complete release materials and implement local Arch/Omarchy packaging. The existing README and three synthetic screenshots were retained, with focused installation/sample/license links and release-status updates. Added standard MIT/Apache license files matching Cargo's existing declaration.

`examples/sample_data.rs` creates a private new directory and seeds an Ebbi database through the normal persistence API. It refuses every existing destination, including symlinks. The sample contains 90 days of binary/numeric habit history (including missing and zero values), three tasks, four Markdown topics and twelve completed/due/overdue/upcoming reviews. `docs/sample-data.md` documents generation and isolated launches. Its focused test checks persisted records, report opportunities, missing/zero values and overwrite refusal. CI now includes it through `cargo test --all-targets --locked`.

Back accepted ownership of `packaging/**` and `docs/installation.md` before edits; Main owned the sample generator, other documentation and wiki/status. Shared interface: existing version 0.1.0 binary and CLI, with Dashboard default and Study/Analytics desktop actions. Back explicitly handed the files back after validation, and reviewed the sample read-only; Main reviewed packaging and performed native checks.

Verification:

- Formatting, strict all-target Clippy and all **21 tests** passed offline with America/Lima timezone (20 existing tests plus the sample generator). README/documentation links and PNG files were checked; screenshots were not regenerated.
- A clean, locked, offline release build and `makepkg` passed from an allowlisted source snapshot. The refreshed final snapshot also built successfully. Source checksum verification, modified-source rejection, desktop validation, package metadata/file-list inspection, extraction and dynamic-library resolution passed.
- The package contains only `/usr/bin/ebbi`, its desktop entry and two license files. It does not overwrite application data. Source archives retain README/screenshots and sample sources while excluding Git metadata, build artifacts and databases.
- Extracted package launches for Dashboard, Study and Analytics mapped native Wayland windows using `/tmp/ebbi-release-demo-20261016/ebbi.db`. Each window closed through Omarchy's current Lua dispatcher and each process exited with code 0. The initial smoke harness used obsolete dispatcher syntax; after correcting that harness, all three checks passed.

Artifacts and build logs: `/tmp/ebbi-package.83dwNr3k/`; package: `ebbi-0.1.0-1-x86_64.pkg.tar.zst`. Native logs: `/tmp/ebbi-package-smoke-lydpivja/`. Installation/build/upgrade/removal instructions are in `docs/installation.md`.

This slice is complete. Milestone 5 still needs single-instance routing, graphical restore, release automation and formal performance validation. This is an unsigned local x86_64 package, checked by extraction on the current system; no clean chroot, privileged system installation, remote CI run or publication was performed. Personal and legacy databases were not inspected or modified.

## System installation — 2026-10-16

At the owner's request, installed the verified local `ebbi 0.1.0-1` package through `pkexec pacman -U`. Pacman completed successfully and updated the desktop entry cache. The earlier extraction-only verification record above describes the pre-installation slice; installation has now been performed. Publication and a clean chroot remain unverified.

Post-install verification: `pacman -Q ebbi` reports `0.1.0-1`; `pacman -Qk ebbi` reports zero missing files; `/usr/bin/ebbi --version` returns `0.1.0`; the installed desktop entry validates.

## Custom application icon — 2026-10-16

At the owner's request, packaged the supplied 512×512 bird PNG unchanged as `packaging/ebbi.png`, installed at `/usr/share/icons/hicolor/512x512/apps/ebbi.png`. The desktop entry uses `Icon=ebbi`; Iced's Linux `application_id` now matches the `ebbi.desktop` basename so Wayland associates running windows with the application. Updated the source-archive instructions and package revision to `0.1.0-2`, rebuilt offline with makepkg and installed the upgrade through pacman. The icon/theme and desktop caches were updated by package hooks.

Formatting, strict Clippy, desktop validation, package build, installed file integrity and exact original/installed image-byte comparisons passed. A synthetic native launch confirmed window class `ebbi` and clean exit after window close; logs: `/tmp/ebbi-icon-check-0liyzqvq/`. No application data or automatic startup settings changed.

## Single-instance routing and graphical restore — 2026-10-16

The owner authorized these two remaining roadmap tasks and asked Main and Back to collaborate through Herdr. Back owned persistence, worker and storage tests; Main owned routing/main, application/UI, documentation and wiki. Ownership stayed disjoint in the existing dirty Git checkout; no worktree or merge was needed. Back explicitly handed files back before Main integration and performed read-only reviews.

Repeated launches for the same canonical database path send a bounded view request over a user-private Unix socket, receive a queue acknowledgement and exit. A process lock prevents competing listeners, startup/shutdown races retry within a bounded window, and stale sockets are replaced only while holding that lock. Separate `--db` paths remain independent. Forms, import previews and in-flight database work defer navigation. No forced Wayland activation is claimed. The already locked rustix 1.1.5 dependency is now direct with its net feature for safe nonblocking connect; the lockfile adds no new package versions.

Settings now offers a typed-path/native-picker restore flow with explicit source/destination confirmation, F8 confirmation and Escape cancellation. Restore accepts a closed supported backup from the same workspace, validates and migrates a disposable copy, preserves a verified permanent pre-restore recovery snapshot, checkpoints/closes SQLite, atomically replaces the destination and reopens the worker. Stable path and transferred inode locks protect replacement/reopen, including older Ebbi processes that only use the inode lock. Candidate files are removed, selected source bytes remain unchanged, and unknown sidecars are never deleted. Success reports the recovery path; errors clear stale UI data/selections and require Reload/Retry. Graphical restore requires a readable current database; startup corruption recovery still uses a copied backup and `--db`.

Verification: formatting, strict all-target Clippy, all 31 tests (16 binary/app/worker/IPC, 10 storage, four study/analytics/migration, one sample generator) and a locked offline release build pass. New checks cover view routing/isolation/symlink aliases, stale sockets/private directories/invalid protocol, shutdown timing and saturated connection backlog, deferred navigation, restore confirmation/error state, recovery contents and source immutability, invalid/live/unrelated/newer/sidecar sources, external-reader refusal, private v1 migration, reopening and continued writes.

Native Wayland evidence: `/tmp/ebbi-routing-restore-6ykz6kpw/`. A typed-path restore brought back an older synthetic task title, while the permanent recovery snapshot retained the newer title. A Study launch during confirmation queued and switched the same window after F8 restore. Analytics and Dashboard launches reused the same process; closing the targeted demo exited it. Screenshots and logs are retained there. An initial keyboard harness sent text before focus settled; it was corrected with explicit delays. A close command initially targeted a terminal; subsequent input/close actions verified the exact demo PID. No personal or legacy database was opened or changed.

Remaining: release automation and formal performance benchmarks. The installed system package was not rebuilt/upgraded in this slice. Native portal picker interaction, production imports, remote CI, hardware power-loss/full-disk injection and writers bypassing advisory locks were not validated. Post-replacement filesystem/reopen failure can require Retry or manual recovery from the retained snapshot; automatic rollback is not claimed.
