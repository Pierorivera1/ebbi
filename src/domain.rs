use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub habits: Vec<Habit>,
    pub tasks: Vec<Todo>,
    pub study_topics: Vec<StudyTopic>,
    pub study_reviews: Vec<StudyReview>,
    pub reviews_due: usize,
    pub reviews_overdue: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Habit {
    pub id: i64,
    pub name: String,
    pub kind: HabitKind,
    pub unit: String,
    pub goal: Option<NumericGoal>,
    pub next_goal: Option<(NaiveDate, NumericGoal)>,
    pub value: Option<f64>,
    pub position: i64,
    pub archived: bool,
    pub eligible: bool,
    pub eligibility_known: bool,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub due_date: Option<NaiveDate>,
    pub completed: bool,
    pub completed_at: Option<String>,
    pub position: i64,
}

#[derive(Debug, Clone)]
pub enum Action {
    AddNumericHabit {
        name: String,
        unit: String,
        goal: NumericGoal,
    },
    SetNumericGoal {
        id: i64,
        effective_from: NaiveDate,
        goal: NumericGoal,
    },
    SetHabitValue {
        id: i64,
        date: NaiveDate,
        value: Option<f64>,
    },
    AddStudyTopic {
        title: String,
        notes: String,
        initial_date: NaiveDate,
    },
    UpdateStudyTopic {
        id: i64,
        title: String,
        notes: String,
    },
    RescheduleStudyTopic {
        id: i64,
        initial_date: NaiveDate,
    },
    DeleteStudyTopic(i64),
    SetReviewCompleted {
        id: i64,
        completed: bool,
    },
    RescheduleReview {
        id: i64,
        date: NaiveDate,
    },
    AddHabit(String),
    DeleteHabit(i64),
    RenameHabit {
        id: i64,
        name: String,
    },
    MoveHabit {
        id: i64,
        delta: i64,
    },
    SetHabitArchived {
        id: i64,
        archived: bool,
    },
    SetHabitCompletion {
        id: i64,
        date: NaiveDate,
        completed: bool,
    },
    AddTask {
        title: String,
        notes: String,
        due_date: Option<NaiveDate>,
    },
    UpdateTask {
        id: i64,
        title: String,
        notes: String,
        due_date: Option<NaiveDate>,
    },
    SetTaskCompleted {
        id: i64,
        completed: bool,
    },
    MoveTask {
        id: i64,
        delta: i64,
    },
    DeleteTask(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyKind {
    Habitodo,
    Repasito,
}

impl std::fmt::Display for LegacyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Habitodo => "Habit Tracker / Habitodo",
            Self::Repasito => "Repasito",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreview {
    pub source_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub kind: LegacyKind,
    pub fingerprint: String,
    pub counts: Vec<(String, usize)>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HabitKind {
    Binary,
    Numeric,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NumericGoal {
    AtLeast(f64),
    AtMost(f64),
    Between { min: f64, max: f64 },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyTopic {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub initial_date: Option<NaiveDate>,
    pub completed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyReview {
    pub id: i64,
    pub topic_id: i64,
    pub stage: u8,
    pub original_due_date: Option<NaiveDate>,
    pub scheduled_date: NaiveDate,
    pub completed_at: Option<String>,
    pub completed_date: Option<NaiveDate>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub successful: usize,
    pub eligible: usize,
}
impl Counts {
    pub fn percentage(&self) -> Option<f64> {
        (self.eligible > 0).then(|| self.successful as f64 / self.eligible as f64 * 100.0)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DayState {
    Tracked,
    Untracked,
    Unknown,
    Future,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyProgress {
    pub date: NaiveDate,
    pub counts: Counts,
    pub unknown_habits: usize,
    pub state: DayState,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericPoint {
    pub date: NaiveDate,
    pub value: Option<f64>,
    pub goal: Option<NumericGoal>,
    pub successful: bool,
    pub eligible: bool,
    pub state: DayState,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitAnalytics {
    pub id: i64,
    pub name: String,
    pub kind: HabitKind,
    pub unit: String,
    pub consistency: Counts,
    pub current_streak: usize,
    pub longest_streak: usize,
    pub numeric: Vec<NumericPoint>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodSummary {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub counts: Counts,
    pub provisional: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodComparison {
    pub current: PeriodSummary,
    pub previous: PeriodSummary,
    pub percentage_point_change: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyDay {
    pub date: NaiveDate,
    pub completed: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyAnalytics {
    pub due: usize,
    pub overdue: usize,
    pub upcoming: usize,
    pub completed: usize,
    pub on_time: Counts,
    pub unknown_deadlines: usize,
    pub unknown_completion_dates: usize,
    pub activity: Vec<StudyDay>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsReport {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub today: NaiveDate,
    pub daily: Vec<DailyProgress>,
    pub habits: Vec<HabitAnalytics>,
    pub total: Counts,
    pub weekly: PeriodComparison,
    pub monthly: PeriodComparison,
    pub study: StudyAnalytics,
}
