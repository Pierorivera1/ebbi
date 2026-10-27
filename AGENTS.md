# Ebbi working instructions

Read `productivity_codex/PROJECT.md`, `productivity_codex/AGENTS.md`, and `IMPLEMENTATION_STATUS.md` before editing. The owner authorized milestones 1–4 on 2026-10-15; the original first-assignment planning gate has been satisfied for that scope.

Keep UI/application code separate from SQLite persistence and run database work off the UI thread. Preserve the original Habit Tracker/Repasito repositories and databases. Use temporary/synthetic data for checks. No silent ephemeral fallback or successful-save message before commit. Keep tests focused on data integrity and meaningful behavior.

When delegating, agree on shared interfaces and disjoint file ownership before edits. Explicitly hand files back before integration fixes. Back's domain, persistence and analytics work has been handed back to Main.

Update the LLM-wiki Ebbi notes and log when milestone status changes; immutable raw sources stay unchanged. Study and numeric habits/analytics are authorized. The owner also authorized single-instance view routing and graphical backup restore on 2026-10-16; remaining desktop release work is release automation and benchmarks.
