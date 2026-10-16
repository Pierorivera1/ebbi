ALTER TABLE habits ADD COLUMN kind TEXT NOT NULL DEFAULT 'binary' CHECK(kind IN ('binary','numeric'));
ALTER TABLE habits ADD COLUMN unit TEXT NOT NULL DEFAULT '';
CREATE TABLE habit_goals(habit_id INTEGER NOT NULL REFERENCES habits(id), effective_from TEXT NOT NULL, goal TEXT NOT NULL, PRIMARY KEY(habit_id,effective_from));
CREATE TABLE habit_values(habit_id INTEGER NOT NULL REFERENCES habits(id), date TEXT NOT NULL, value REAL NOT NULL CHECK(value >= 0 AND value <= 1.7976931348623157e308), PRIMARY KEY(habit_id,date));
ALTER TABLE study_reviews ADD COLUMN completed_date TEXT;
INSERT INTO schema_migrations(version) VALUES(2);
PRAGMA user_version=2;
