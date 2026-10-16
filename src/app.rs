use crate::worker::{Operation, Reply, Worker};
use chrono::{Local, NaiveDate};
use ebbi::domain::{
    Action, AnalyticsReport, HabitKind, ImportPreview, LegacyKind, NumericGoal, Snapshot,
    StudyReview, StudyTopic, Todo,
};
use iced::{event, keyboard, window, Event, Subscription, Task};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Settings,
    Study,
    Analytics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Habit(i64),
    Task(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalMode {
    Minimum,
    Maximum,
    Range,
}
impl std::fmt::Display for GoalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Minimum => "At least",
            Self::Maximum => "At most",
            Self::Range => "Between (inclusive)",
        })
    }
}
#[derive(Debug, Clone)]
pub enum FormKind {
    Habit(Option<i64>),
    Task(Option<i64>),
    DeleteTask(i64),
    NumericHabit,
    HabitValue(i64),
    HabitGoal(i64),
    Study(Option<i64>),
    StudyDate(i64),
    ReviewDate(i64),
    DeleteStudy(i64),
}
#[derive(Debug)]
pub struct Form {
    pub kind: FormKind,
    pub title: String,
    pub notes: iced::widget::text_editor::Content,
    notes_original: String,
    notes_dirty: bool,
    pub due: String,
    pub unit: String,
    pub goal_mode: GoalMode,
    pub bound: String,
    pub upper: String,
    pub confirmed: bool,
    pub entry_date: NaiveDate,
}

impl Form {
    fn notes_text(&self) -> String {
        if !self.notes_dirty {
            return self.notes_original.clone();
        }
        // Iced Content::text adds a final newline even when the user did not enter one.
        self.notes
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<Box<Reply>, String>),
    OpenForm(FormKind),
    Unit(String),
    GoalMode(GoalMode),
    Bound(String),
    Upper(String),
    StudySearch(String),
    StudyDay(Option<NaiveDate>),
    HoverReview(Option<i64>),
    ToggleReviewNotes(i64),
    SnoozeReview(i64),
    SelectReview(i64),
    PreviewNotes,
    Link(String),
    AnalyticsDays(u32),
    InspectDay(usize),
    Navigate(Screen),
    DateStep(i64),
    Today,
    DateInput(String),
    GoDate,
    Resize(f32),
    Tick,
    Key(keyboard::Key, keyboard::Modifiers),
    Select(Selection),
    TaskDragStart(i64),
    TaskDragOver(i64),
    TaskDragLeave(i64),
    TaskDrop,
    TaskDragCancel,
    Title(String),
    Notes(iced::widget::text_editor::Action),
    Due(String),
    Save,
    Cancel,
    Act(Action),
    Retry,
    Refresh,
    SourcePath(String),
    SourceKind(LegacyKind),
    Preview,
    Import,
    BackupPath(String),
    Backup,
    BrowseSource,
    BrowseBackup,
    PickedPath(bool, Option<PathBuf>),
    Dismiss,
}

pub struct App {
    pub screen: Screen,
    pub study_search: String,
    pub study_date: Option<NaiveDate>,
    pub study_hovered: Option<i64>,
    pub study_notes: Option<i64>,
    pub study_selected: Option<i64>,
    pub markdown: Vec<iced::widget::markdown::Item>,
    pub analytics: Option<AnalyticsReport>,
    pub analytics_days: u32,
    pub inspected_day: usize,
    pub date: NaiveDate,
    pub today: NaiveDate,
    pub date_input: String,
    pub width: f32,
    pub data: Option<Snapshot>,
    pub busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub form: Option<Form>,
    pub selected: Option<Selection>,
    pub drag_task: Option<i64>,
    pub drop_task: Option<i64>,
    pub db_path: Result<PathBuf, String>,
    pub source_path: String,
    pub source_kind: LegacyKind,
    pub preview: Option<ImportPreview>,
    pub backup_path: String,
    worker: Option<Worker>,
    close_form_after_save: bool,
}

impl App {
    pub fn new(path: Result<PathBuf, String>, screen: Screen) -> (Self, Task<Message>) {
        let today = Local::now().date_naive();
        let backup_path = path
            .as_ref()
            .ok()
            .and_then(|p| p.parent())
            .map(|p| {
                p.join(format!(
                    "backup-{}.db",
                    Local::now().format("%Y%m%d-%H%M%S")
                ))
                .to_string_lossy()
                .into_owned()
            })
            .unwrap_or_default();
        let worker = path.as_ref().ok().map(|p| Worker::start(p.clone()));
        let error = path.as_ref().err().cloned();
        let mut app = Self {
            screen,
            study_search: String::new(),
            study_date: None,
            study_hovered: None,
            study_notes: None,
            study_selected: None,
            markdown: Vec::new(),
            analytics: None,
            analytics_days: 30,
            inspected_day: 0,
            date: today,
            today,
            date_input: today.to_string(),
            width: 1100.0,
            data: None,
            busy: false,
            error,
            notice: None,
            form: None,
            selected: None,
            drag_task: None,
            drop_task: None,
            source_path: Self::legacy_path(LegacyKind::Habitodo),
            source_kind: LegacyKind::Habitodo,
            preview: None,
            backup_path,
            db_path: path,
            worker,
            close_form_after_save: false,
        };
        let task = app.run(if screen == Screen::Analytics {
            Operation::Analytics(30)
        } else {
            Operation::Refresh
        });
        (app, task)
    }

    fn legacy_path(kind: LegacyKind) -> String {
        dirs::data_local_dir()
            .map(|base| {
                let names: &[&str] = match kind {
                    LegacyKind::Habitodo => {
                        &["habit_tracker/habit_tracker.db", "habitodo/habitodo.db"]
                    }
                    LegacyKind::Repasito => &[
                        "Omacom/omacalendar/omacalendar.db",
                        "Omacom/repasito/repasito.db",
                    ],
                };
                names
                    .iter()
                    .map(|p| base.join(p))
                    .find(|p| p.is_file())
                    .unwrap_or_else(|| base.join(names[0]))
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default()
    }

    fn run(&mut self, operation: Operation) -> Task<Message> {
        if self.busy {
            return Task::none();
        }
        let Some(worker) = &self.worker else {
            return Task::none();
        };
        self.busy = true;
        self.error = None;
        Task::perform(worker.request(operation, self.date, self.today), |reply| {
            Message::Loaded(reply.map(Box::new))
        })
    }

    pub fn tasks(&self) -> Vec<&Todo> {
        self.tasks_at(chrono::Utc::now())
    }

    fn tasks_at(&self, now: chrono::DateTime<chrono::Utc>) -> Vec<&Todo> {
        self.data
            .as_ref()
            .map(|data| {
                data.tasks
                    .iter()
                    .filter(|task| {
                        !task.completed
                            || task
                                .completed_at
                                .as_deref()
                                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                                .is_none_or(|completed| {
                                    now.signed_duration_since(completed)
                                        < chrono::Duration::hours(6)
                                })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Loaded(result) => {
                self.busy = false;
                match result {
                    Ok(reply) => {
                        let is_analytics = reply.analytics.is_some();
                        if let Some(report) = reply.analytics {
                            match report {
                                Ok(report) => {
                                    self.inspected_day = self
                                        .inspected_day
                                        .min(report.daily.len().saturating_sub(1));
                                    self.analytics = Some(report);
                                }
                                Err(error) => {
                                    self.analytics = None;
                                    self.error = Some(error);
                                }
                            }
                        }
                        let snapshot = reply.snapshot;
                        self.preview = reply.preview.or_else(|| self.preview.take());
                        self.notice = reply.notice.or_else(|| self.notice.take());
                        let closed_form = self.close_form_after_save;
                        if self.close_form_after_save {
                            if self
                                .form
                                .as_ref()
                                .is_some_and(|f| matches!(f.kind, FormKind::Study(None)))
                            {
                                self.study_date = None;
                            }
                            self.form = None;
                            self.preview = None;
                        }
                        self.close_form_after_save = false;
                        match snapshot {
                            Ok(data) => {
                                self.data = Some(data);
                                if !is_analytics || self.analytics.is_some() {
                                    self.error = None;
                                }
                            }
                            Err(error) => {
                                self.data = None;
                                self.error = Some(error);
                            }
                        }
                        if self
                            .study_selected
                            .is_none_or(|id| !self.study_reviews().iter().any(|r| r.id == id))
                        {
                            self.study_selected = self.study_reviews().first().map(|r| r.id);
                        }
                        if self
                            .selected
                            .is_some_and(|s| !self.selections().contains(&s))
                        {
                            self.selected = None;
                        }
                        if self.screen == Screen::Analytics && !is_analytics {
                            return self.run(Operation::Analytics(self.analytics_days));
                        }
                        if closed_form {
                            return iced::widget::text_input::focus("no-input");
                        }
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.close_form_after_save = false;
                    }
                }
            }
            Message::PickedPath(backup, path) => {
                if let Some(path) = path.filter(|_| !self.busy) {
                    let path = path.to_string_lossy().into_owned();
                    if backup {
                        self.backup_path = path;
                    } else {
                        self.source_path = path;
                        self.preview = None;
                    }
                }
            }
            Message::Resize(width) => self.width = width,
            Message::Dismiss => {
                self.error = None;
                self.notice = None;
            }
            Message::Title(value) => {
                if !self.busy {
                    if let Some(form) = &mut self.form {
                        form.title = value;
                    }
                }
            }
            Message::Notes(value) => {
                if !self.busy {
                    if value.is_edit() {
                        self.markdown.clear();
                    }
                    if let Some(form) = &mut self.form {
                        form.notes_dirty |= value.is_edit();
                        form.notes.perform(value);
                    }
                }
            }
            Message::Due(value) => {
                if !self.busy {
                    if let Some(form) = &mut self.form {
                        form.due = value;
                        form.confirmed = false;
                    }
                }
            }
            Message::StudySearch(value) => {
                self.study_search = value;
                self.study_selected = self.study_reviews().first().map(|r| r.id);
            }
            Message::StudyDay(date) => {
                self.study_date = date;
                self.study_selected = self.study_reviews().first().map(|r| r.id);
            }
            Message::HoverReview(id) => self.study_hovered = id,
            Message::SelectReview(id) => self.study_selected = Some(id),
            Message::InspectDay(index) => self.inspected_day = index,
            Message::Link(url) => self.notice = Some(format!("Link: {url}")),
            Message::Unit(value) => {
                if !self.busy {
                    if let Some(form) = &mut self.form {
                        form.unit = value;
                    }
                }
            }
            Message::GoalMode(value) => {
                if !self.busy {
                    if let Some(form) = &mut self.form {
                        form.goal_mode = value;
                    }
                }
            }
            Message::Bound(value) => {
                if !self.busy {
                    if let Some(form) = &mut self.form {
                        form.bound = value;
                    }
                }
            }
            Message::Upper(value) => {
                if !self.busy {
                    if let Some(form) = &mut self.form {
                        form.upper = value;
                    }
                }
            }
            Message::SourcePath(value) => {
                if !self.busy {
                    self.source_path = value;
                    self.preview = None;
                }
            }
            Message::BackupPath(value) => self.backup_path = value,
            Message::DateInput(value) => self.date_input = value,
            Message::Tick => {
                if self
                    .selected
                    .is_some_and(|selection| !self.selections().contains(&selection))
                {
                    self.selected = None;
                    self.drag_task = None;
                    self.drop_task = None;
                }
                let today = Local::now().date_naive();
                if today != self.today && !self.busy {
                    if self.date == self.today && self.form.is_none() {
                        self.date = today;
                        self.date_input = today.to_string();
                    }
                    self.today = today;
                    return self.run(Operation::Refresh);
                }
            }
            Message::TaskDrop => {
                let source = self.drag_task.take();
                let target = self.drop_task.take();
                if !self.busy {
                    if let (Some(id), Some(target)) = (source, target) {
                        if let Some(action) = self.task_move_action(id, target) {
                            return self.run(Operation::Apply(action));
                        }
                    }
                }
            }
            Message::TaskDragCancel => {
                self.drag_task = None;
                self.drop_task = None;
            }
            Message::Key(key, modifiers) => return self.key(key, modifiers),
            _ if self.busy => {}
            Message::SnoozeReview(id) => {
                if let Some(review) = self.data.as_ref().and_then(|d| {
                    d.study_reviews
                        .iter()
                        .find(|r| r.id == id && r.completed_at.is_none())
                }) {
                    if let Some(date) = review.scheduled_date.succ_opt() {
                        return self.run(Operation::Apply(Action::RescheduleReview { id, date }));
                    }
                    self.error = Some("Review date is out of range.".into());
                }
            }
            Message::ToggleReviewNotes(id) => {
                if self.study_notes == Some(id) {
                    self.study_notes = None;
                } else if let Some(topic) = self.data.as_ref().and_then(|d| {
                    d.study_reviews
                        .iter()
                        .find(|r| r.id == id)
                        .and_then(|r| d.study_topics.iter().find(|t| t.id == r.topic_id))
                }) {
                    self.markdown = iced::widget::markdown::parse(&topic.notes).collect();
                    self.study_notes = Some(id);
                }
            }
            Message::OpenForm(kind) => return self.open_form(kind),
            Message::PreviewNotes => {
                if let Some(form) = &self.form {
                    self.markdown = iced::widget::markdown::parse(&form.notes_text()).collect();
                }
            }
            Message::AnalyticsDays(days) => {
                self.analytics_days = days;
                self.analytics = None;
                return self.run(Operation::Analytics(days));
            }
            Message::Navigate(screen) => {
                self.screen = screen;
                self.form = None;
                self.selected = None;
                self.drag_task = None;
                self.drop_task = None;
                let task = if screen == Screen::Analytics {
                    self.analytics = None;
                    self.run(Operation::Analytics(self.analytics_days))
                } else {
                    Task::none()
                };
                return Task::batch([
                    task,
                    iced::widget::scrollable::snap_to(
                        iced::widget::scrollable::Id::new("main-scroll"),
                        iced::widget::scrollable::RelativeOffset::START,
                    ),
                    iced::widget::text_input::focus("no-input"),
                ]);
            }
            Message::DateStep(step) => {
                if let Some(date) = self.date.checked_add_signed(chrono::Duration::days(step)) {
                    self.date = date;
                    self.date_input = date.to_string();
                    return self.run(Operation::Refresh);
                }
            }
            Message::Today => {
                self.date = self.today;
                self.date_input = self.today.to_string();
                return self.run(Operation::Refresh);
            }
            Message::GoDate => match NaiveDate::parse_from_str(&self.date_input, "%Y-%m-%d") {
                Ok(date) => {
                    self.date = date;
                    return self.run(Operation::Refresh);
                }
                Err(_) => self.error = Some("Enter a calendar date as YYYY-MM-DD.".into()),
            },
            Message::Select(selection) => self.selected = Some(selection),
            Message::TaskDragStart(id) => {
                if self.form.is_none() && self.screen == Screen::Dashboard {
                    self.selected = Some(Selection::Task(id));
                    self.drag_task = Some(id);
                    self.drop_task = Some(id);
                }
            }
            Message::TaskDragOver(id) => {
                if self.drag_task.is_some() {
                    self.drop_task = Some(id);
                }
            }
            Message::TaskDragLeave(id) => {
                if self.drop_task == Some(id) {
                    self.drop_task = None;
                }
            }
            Message::Cancel => {
                self.drag_task = None;
                self.drop_task = None;
                self.form = None;
                self.preview = None;
                self.error = None;
                return iced::widget::text_input::focus("no-input");
            }
            Message::Save => return self.save(),
            Message::Act(action) => return self.run(Operation::Apply(action)),
            Message::Retry => return self.run(Operation::Retry),
            Message::Refresh => return self.run(Operation::Refresh),
            Message::SourceKind(kind) => {
                self.source_kind = kind;
                self.source_path = Self::legacy_path(kind);
                self.preview = None;
            }
            Message::Preview => {
                return self.run(Operation::Preview(
                    self.source_kind,
                    PathBuf::from(self.source_path.trim()),
                ))
            }
            Message::Import => {
                if let Some(preview) = self.preview.clone() {
                    self.close_form_after_save = true;
                    return self.run(Operation::Import(preview));
                }
            }
            Message::BrowseSource | Message::BrowseBackup => {
                let backup = matches!(message, Message::BrowseBackup);
                return Task::perform(
                    async move {
                        let dialog = rfd::AsyncFileDialog::new()
                            .add_filter("SQLite database", &["db", "sqlite", "sqlite3"]);
                        let file = if backup {
                            dialog
                                .set_title("Save an Ebbi backup")
                                .set_file_name("ebbi-backup.db")
                                .save_file()
                                .await
                        } else {
                            dialog
                                .set_title("Select a legacy database")
                                .pick_file()
                                .await
                        };
                        file.map(|f| f.path().to_path_buf())
                    },
                    move |path| Message::PickedPath(backup, path),
                );
            }
            Message::Backup => {
                return self.run(Operation::Backup(PathBuf::from(self.backup_path.trim())))
            }
        }
        Task::none()
    }

    fn open_form(&mut self, kind: FormKind) -> Task<Message> {
        self.drag_task = None;
        self.drop_task = None;
        self.study_notes = None;
        let mut form = Form {
            kind: kind.clone(),
            title: String::new(),
            notes: iced::widget::text_editor::Content::new(),
            notes_original: String::new(),
            notes_dirty: false,
            due: String::new(),
            unit: String::new(),
            goal_mode: GoalMode::Minimum,
            bound: String::new(),
            upper: String::new(),
            confirmed: false,
            entry_date: self.date,
        };
        match kind {
            FormKind::Habit(Some(id)) => {
                if let Some(habit) = self
                    .data
                    .as_ref()
                    .and_then(|d| d.habits.iter().find(|h| h.id == id))
                {
                    form.title = habit.name.clone();
                }
            }
            FormKind::Task(Some(id)) | FormKind::DeleteTask(id) => {
                if let Some(task) = self
                    .data
                    .as_ref()
                    .and_then(|d| d.tasks.iter().find(|t| t.id == id))
                {
                    form.title = task.title.clone();
                    form.notes_original = task.notes.clone();
                    // Buffer initialization drops one final newline; preserve it before editing.
                    form.notes =
                        iced::widget::text_editor::Content::with_text(&format!("{}\n", task.notes));
                    form.due = task.due_date.map(|d| d.to_string()).unwrap_or_default();
                }
            }
            FormKind::Study(id) => {
                form.due = self.today.to_string();
                if let Some(topic) =
                    id.and_then(|id| self.data.as_ref()?.study_topics.iter().find(|t| t.id == id))
                {
                    form.title = topic.title.clone();
                    form.notes_original = topic.notes.clone();
                    form.notes = iced::widget::text_editor::Content::with_text(&format!(
                        "{}\n",
                        topic.notes
                    ));
                    form.due = topic
                        .initial_date
                        .map(|d| d.to_string())
                        .unwrap_or_default();
                }
            }
            FormKind::StudyDate(id) | FormKind::DeleteStudy(id) => {
                if let Some(topic) = self
                    .data
                    .as_ref()
                    .and_then(|d| d.study_topics.iter().find(|t| t.id == id))
                {
                    form.title = topic.title.clone();
                    form.due = topic.initial_date.unwrap_or(self.today).to_string();
                }
            }
            FormKind::ReviewDate(id) => {
                if let Some(review) = self
                    .data
                    .as_ref()
                    .and_then(|d| d.study_reviews.iter().find(|r| r.id == id))
                {
                    form.due = review.scheduled_date.to_string();
                }
            }
            FormKind::HabitValue(id) | FormKind::HabitGoal(id) => {
                if let Some(habit) = self
                    .data
                    .as_ref()
                    .and_then(|d| d.habits.iter().find(|h| h.id == id))
                {
                    form.title = habit.name.clone();
                    form.unit = habit.unit.clone();
                    form.bound = habit.value.map(|v| v.to_string()).unwrap_or_default();
                    if matches!(kind, FormKind::HabitGoal(_)) {
                        match &habit.goal {
                            Some(NumericGoal::AtLeast(v)) => {
                                form.goal_mode = GoalMode::Minimum;
                                form.bound = v.to_string();
                            }
                            Some(NumericGoal::AtMost(v)) => {
                                form.goal_mode = GoalMode::Maximum;
                                form.bound = v.to_string();
                            }
                            Some(NumericGoal::Between { min, max }) => {
                                form.goal_mode = GoalMode::Range;
                                form.bound = min.to_string();
                                form.upper = max.to_string();
                            }
                            None => {}
                        }
                        form.due = self.today.succ_opt().unwrap_or(self.today).to_string();
                    }
                }
            }
            _ => {}
        }
        self.markdown.clear();
        self.form = Some(form);
        self.error = None;
        let focus = match self.form.as_ref().map(|f| &f.kind) {
            Some(FormKind::HabitValue(_) | FormKind::HabitGoal(_)) => "numeric-value",
            Some(FormKind::StudyDate(_) | FormKind::ReviewDate(_)) => "form-date",
            _ => "form-title",
        };
        Task::batch([
            iced::widget::text_input::focus(focus),
            iced::widget::scrollable::snap_to(
                iced::widget::scrollable::Id::new("main-scroll"),
                iced::widget::scrollable::RelativeOffset::START,
            ),
        ])
    }

    fn save(&mut self) -> Task<Message> {
        let Some(form) = &self.form else {
            return Task::none();
        };
        let date = || {
            NaiveDate::parse_from_str(form.due.trim(), "%Y-%m-%d")
                .map_err(|_| "Enter a calendar date as YYYY-MM-DD.".to_string())
        };
        let number = |value: &str| -> Result<f64, String> {
            let value: f64 = value
                .trim()
                .parse()
                .map_err(|_| "Enter a nonnegative number.".to_string())?;
            if !value.is_finite() || value < 0.0 {
                return Err("Enter a finite, nonnegative number.".into());
            }
            Ok(value)
        };
        let goal = || -> Result<NumericGoal, String> {
            let lower = number(&form.bound)?;
            Ok(match form.goal_mode {
                GoalMode::Minimum => NumericGoal::AtLeast(lower),
                GoalMode::Maximum => NumericGoal::AtMost(lower),
                GoalMode::Range => {
                    let max = number(&form.upper)?;
                    if max < lower {
                        return Err("Upper bound must be at least the lower bound.".into());
                    }
                    NumericGoal::Between { min: lower, max }
                }
            })
        };
        let action = (|| -> Result<Action, String> {
            match form.kind {
                FormKind::DeleteTask(id) => Ok(Action::DeleteTask(id)),
                FormKind::DeleteStudy(id) => Ok(Action::DeleteStudyTopic(id)),
                FormKind::HabitValue(id) => Ok(Action::SetHabitValue {
                    id,
                    date: form.entry_date,
                    value: if form.bound.trim().is_empty() {
                        None
                    } else {
                        Some(number(&form.bound)?)
                    },
                }),
                FormKind::HabitGoal(id) => Ok(Action::SetNumericGoal {
                    id,
                    effective_from: date()?,
                    goal: goal()?,
                }),
                FormKind::StudyDate(id) => {
                    let initial_date = date()?;
                    ebbi::analytics::fixed_review_dates(initial_date)?;
                    Ok(Action::RescheduleStudyTopic { id, initial_date })
                }
                FormKind::ReviewDate(id) => Ok(Action::RescheduleReview { id, date: date()? }),
                _ => {
                    if form.title.trim().is_empty() {
                        return Err("A title is required.".into());
                    }
                    let title = form.title.trim().to_string();
                    Ok(match form.kind {
                        FormKind::Habit(None) => Action::AddHabit(title),
                        FormKind::Habit(Some(id)) => Action::RenameHabit { id, name: title },
                        FormKind::NumericHabit => Action::AddNumericHabit {
                            name: title,
                            unit: form.unit.trim().into(),
                            goal: goal()?,
                        },
                        FormKind::Task(id) => {
                            let notes = form.notes_text();
                            let due_date = if form.due.trim().is_empty() {
                                None
                            } else {
                                Some(date()?)
                            };
                            match id {
                                Some(id) => Action::UpdateTask {
                                    id,
                                    title,
                                    notes,
                                    due_date,
                                },
                                None => Action::AddTask {
                                    title,
                                    notes,
                                    due_date,
                                },
                            }
                        }
                        FormKind::Study(id) => match id {
                            Some(id) => Action::UpdateStudyTopic {
                                id,
                                title,
                                notes: form.notes_text(),
                            },
                            None => Action::AddStudyTopic {
                                title,
                                notes: form.notes_text(),
                                initial_date: date()?,
                            },
                        },
                        _ => unreachable!(),
                    })
                }
            }
        })();
        let action = match action {
            Ok(action) => action,
            Err(error) => {
                self.error = Some(error);
                return Task::none();
            }
        };
        if matches!(form.kind, FormKind::StudyDate(_)) && !form.confirmed {
            if let Some(form) = &mut self.form {
                form.confirmed = true;
            }
            return Task::none();
        }
        self.close_form_after_save = true;
        self.run(Operation::Apply(action))
    }

    pub fn study_topics(&self) -> Vec<&StudyTopic> {
        let query = self.study_search.trim().to_lowercase();
        self.data
            .as_ref()
            .map(|data| {
                data.study_topics
                    .iter()
                    .filter(|topic| {
                        query.is_empty()
                            || topic.title.to_lowercase().contains(&query)
                            || topic.notes.to_lowercase().contains(&query)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn study_reviews(&self) -> Vec<&StudyReview> {
        let matching: std::collections::BTreeSet<_> =
            self.study_topics().iter().map(|t| t.id).collect();
        let mut reviews: Vec<_> = self
            .data
            .as_ref()
            .map(|data| {
                data.study_reviews
                    .iter()
                    .filter(|review| {
                        matching.contains(&review.topic_id)
                            && match self.study_date {
                                None => true,
                                Some(date) if date == self.today => {
                                    review.scheduled_date == date
                                        || (review.completed_at.is_none()
                                            && review.scheduled_date < date)
                                        || review.completed_date == Some(date)
                                }
                                Some(date) => review.scheduled_date == date,
                            }
                    })
                    .collect()
            })
            .unwrap_or_default();
        reviews.sort_by_key(|r| (r.completed_at.is_some(), r.scheduled_date, r.id));
        reviews
    }

    pub fn topic_reviews(&self, topic_id: i64) -> Vec<&StudyReview> {
        self.data
            .as_ref()
            .map(|data| {
                data.study_reviews
                    .iter()
                    .filter(|r| r.topic_id == topic_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn task_move_action(&self, id: i64, target: i64) -> Option<Action> {
        if id == target {
            return None;
        }
        let tasks = &self.data.as_ref()?.tasks;
        let from = tasks.iter().position(|task| task.id == id)?;
        let to = tasks.iter().position(|task| task.id == target)?;
        Some(Action::MoveTask {
            id,
            delta: to as i64 - from as i64,
        })
    }

    fn selections(&self) -> Vec<Selection> {
        let mut items: Vec<_> = self
            .data
            .as_ref()
            .map(|data| {
                data.habits
                    .iter()
                    .filter(|h| {
                        !h.archived
                            || h.eligible
                            || h.completed
                            || (!h.eligibility_known && self.date <= self.today)
                    })
                    .map(|h| Selection::Habit(h.id))
                    .collect()
            })
            .unwrap_or_default();
        items.extend(self.tasks().iter().map(|t| Selection::Task(t.id)));
        items
    }

    fn key(&mut self, key: keyboard::Key, modifiers: keyboard::Modifiers) -> Task<Message> {
        use keyboard::{key::Named, Key};
        if matches!(key, Key::Named(Named::PageDown | Named::PageUp)) {
            return iced::widget::scrollable::scroll_by(
                iced::widget::scrollable::Id::new("main-scroll"),
                iced::widget::scrollable::AbsoluteOffset {
                    x: 0.0,
                    y: if key == Key::Named(Named::PageDown) {
                        420.0
                    } else {
                        -420.0
                    },
                },
            );
        }
        if self.busy {
            return Task::none();
        }
        if key == Key::Named(Named::Escape) {
            if self.screen == Screen::Study && self.form.is_none() {
                if !self.study_search.is_empty() {
                    self.study_search.clear();
                } else {
                    self.study_date = None;
                }
                self.study_selected = self.study_reviews().first().map(|r| r.id);
                return iced::widget::text_input::focus("no-input");
            }
            return self.update(
                if self.screen == Screen::Settings && self.preview.is_none() && self.form.is_none()
                {
                    Message::Navigate(Screen::Dashboard)
                } else {
                    Message::Cancel
                },
            );
        }
        if key == Key::Named(Named::Tab) {
            return if modifiers.shift() {
                iced::widget::focus_previous()
            } else {
                iced::widget::focus_next()
            };
        }
        if self.form.is_none() {
            match key.as_ref() {
                Key::Named(Named::F1) => return self.update(Message::Navigate(Screen::Dashboard)),
                Key::Named(Named::F2) => return self.update(Message::Navigate(Screen::Study)),
                Key::Named(Named::F3) => return self.update(Message::Navigate(Screen::Analytics)),
                _ => {}
            }
        }
        if self.screen == Screen::Settings && self.form.is_none() {
            return match key.as_ref() {
                Key::Named(Named::F5) => self.update(Message::SourceKind(LegacyKind::Habitodo)),
                Key::Named(Named::F6) => self.update(Message::SourceKind(LegacyKind::Repasito)),
                Key::Named(Named::F8) => self.update(Message::Import),
                _ => Task::none(),
            };
        }
        if let Some(form) = &self.form {
            if key == Key::Named(Named::F5)
                && matches!(form.kind, FormKind::NumericHabit | FormKind::HabitGoal(_))
            {
                let mode = match form.goal_mode {
                    GoalMode::Minimum => GoalMode::Maximum,
                    GoalMode::Maximum => GoalMode::Range,
                    GoalMode::Range => GoalMode::Minimum,
                };
                return self.update(Message::GoalMode(mode));
            }
            if key == Key::Named(Named::F4) {
                return match form.kind {
                    FormKind::Habit(None) => self.open_form(FormKind::NumericHabit),
                    FormKind::NumericHabit => self.open_form(FormKind::Habit(None)),
                    FormKind::Study(_) => self.update(Message::PreviewNotes),
                    _ => Task::none(),
                };
            }
            return if matches!(
                form.kind,
                FormKind::DeleteTask(_) | FormKind::DeleteStudy(_)
            ) && key == Key::Named(Named::Enter)
            {
                self.save()
            } else {
                Task::none()
            };
        }
        if self.screen == Screen::Study {
            if modifiers.control() || modifiers.alt() || modifiers.logo() {
                return Task::none();
            }
            let reviews = self.study_reviews();
            let selected = self
                .study_selected
                .and_then(|id| reviews.iter().find(|r| r.id == id).copied());
            return match key.as_ref() {
                Key::Character("n") => self.open_form(FormKind::Study(None)),
                Key::Character("/") => iced::widget::text_input::focus("study-search"),
                Key::Character("j") | Key::Character("k") => {
                    let current =
                        selected.and_then(|r| reviews.iter().position(|entry| entry.id == r.id));
                    let down = matches!(key.as_ref(), Key::Character("j"));
                    let index = current
                        .map(|i| {
                            if down {
                                (i + 1).min(reviews.len().saturating_sub(1))
                            } else {
                                i.saturating_sub(1)
                            }
                        })
                        .unwrap_or(0);
                    if let Some(review) = reviews.get(index) {
                        self.study_selected = Some(review.id);
                    }
                    Task::none()
                }
                Key::Named(Named::Space) => {
                    let action = selected.map(|r| Action::SetReviewCompleted {
                        id: r.id,
                        completed: r.completed_at.is_none(),
                    });
                    if let Some(action) = action {
                        self.run(Operation::Apply(action))
                    } else {
                        Task::none()
                    }
                }
                Key::Named(Named::Enter) | Key::Character("i") => {
                    let topic = selected.map(|r| r.topic_id);
                    if let Some(id) = topic {
                        self.open_form(FormKind::Study(Some(id)))
                    } else {
                        Task::none()
                    }
                }
                Key::Character("r") => {
                    let id = selected.filter(|r| r.completed_at.is_none()).map(|r| r.id);
                    if let Some(id) = id {
                        self.open_form(FormKind::ReviewDate(id))
                    } else {
                        Task::none()
                    }
                }
                Key::Character("s") => {
                    let id = selected.map(|r| r.topic_id);
                    if let Some(id) = id {
                        self.open_form(FormKind::StudyDate(id))
                    } else {
                        Task::none()
                    }
                }
                Key::Character("d") => {
                    let id = selected.map(|r| r.topic_id);
                    if let Some(id) = id {
                        self.open_form(FormKind::DeleteStudy(id))
                    } else {
                        Task::none()
                    }
                }
                Key::Character("h") | Key::Character("l") => {
                    let step = if matches!(key.as_ref(), Key::Character("l")) {
                        1
                    } else {
                        -1
                    };
                    if let Some(date) = self
                        .study_date
                        .unwrap_or(self.today)
                        .checked_add_signed(chrono::Duration::days(step))
                    {
                        self.update(Message::StudyDay(Some(date)))
                    } else {
                        Task::none()
                    }
                }
                Key::Character("t") => self.update(Message::StudyDay(Some(self.today))),
                _ => Task::none(),
            };
        }
        if self.screen == Screen::Analytics {
            let length = self.analytics.as_ref().map(|a| a.daily.len()).unwrap_or(0);
            match key.as_ref() {
                Key::Character("1") => return self.update(Message::AnalyticsDays(7)),
                Key::Character("2") => return self.update(Message::AnalyticsDays(30)),
                Key::Character("3") => return self.update(Message::AnalyticsDays(90)),
                Key::Named(Named::ArrowLeft) => {
                    self.inspected_day = self.inspected_day.saturating_sub(1)
                }
                Key::Named(Named::ArrowRight) => {
                    self.inspected_day = (self.inspected_day + 1).min(length.saturating_sub(1))
                }
                _ => {}
            }
            return Task::none();
        }
        if self.screen != Screen::Dashboard
            || modifiers.control()
            || modifiers.alt()
            || modifiers.logo()
        {
            return Task::none();
        }
        match key.as_ref() {
            Key::Named(Named::ArrowUp) | Key::Named(Named::ArrowDown) => {
                let Some(Selection::Task(id)) = self.selected else {
                    return Task::none();
                };
                let visible = self.tasks();
                let Some(index) = visible.iter().position(|task| task.id == id) else {
                    return Task::none();
                };
                let down = key == Key::Named(Named::ArrowDown);
                let Some(neighbor) = visible.get(if down {
                    index + 1
                } else {
                    index.saturating_sub(1)
                }) else {
                    return Task::none();
                };
                if index == 0 && !down {
                    return Task::none();
                }
                if let Some(action) = self.task_move_action(id, neighbor.id) {
                    return self.run(Operation::Apply(action));
                }
            }
            Key::Character("j") | Key::Character("k") => {
                let items = self.selections();
                if items.is_empty() {
                    return Task::none();
                }
                let down = matches!(key.as_ref(), Key::Character("j"));
                let index = self
                    .selected
                    .and_then(|s| items.iter().position(|i| *i == s));
                let index = index
                    .map(|i| {
                        if down {
                            (i + 1).min(items.len() - 1)
                        } else {
                            i.saturating_sub(1)
                        }
                    })
                    .unwrap_or(0);
                self.selected = Some(items[index]);
            }
            Key::Character("h") => return self.update(Message::DateStep(-1)),
            Key::Character("l") => return self.update(Message::DateStep(1)),
            Key::Character("t") => return self.update(Message::Today),
            Key::Character("i") => return self.open_form(FormKind::Task(None)),
            Key::Character("d") => {
                if let Some(Selection::Task(id)) = self.selected {
                    return self.open_form(FormKind::DeleteTask(id));
                }
            }
            Key::Character("a") => return self.open_form(FormKind::Habit(None)),
            Key::Named(Named::Enter) => match self.selected {
                Some(Selection::Habit(id)) => return self.open_form(FormKind::Habit(Some(id))),
                Some(Selection::Task(id)) => return self.open_form(FormKind::Task(Some(id))),
                None => {}
            },
            Key::Named(Named::Space) => {
                if let Some(Selection::Habit(id)) = self.selected {
                    if self.data.as_ref().is_some_and(|d| {
                        d.habits
                            .iter()
                            .any(|h| h.id == id && h.kind == HabitKind::Numeric)
                    }) {
                        return self.open_form(FormKind::HabitValue(id));
                    }
                }
                let action = match self.selected {
                    Some(Selection::Habit(id)) => self
                        .data
                        .as_ref()
                        .and_then(|d| d.habits.iter().find(|h| h.id == id))
                        .filter(|h| h.eligible && h.eligibility_known && self.date <= self.today)
                        .map(|h| Action::SetHabitCompletion {
                            id,
                            date: self.date,
                            completed: !h.completed,
                        }),
                    Some(Selection::Task(id)) => self
                        .data
                        .as_ref()
                        .and_then(|d| d.tasks.iter().find(|t| t.id == id))
                        .map(|t| Action::SetTaskCompleted {
                            id,
                            completed: !t.completed,
                        }),
                    None => None,
                };
                if let Some(action) = action {
                    return self.run(Operation::Apply(action));
                }
            }
            _ => {}
        }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let events = event::listen_with(|event, status, _| match event {
            Event::Window(window::Event::Resized(size)) => Some(Message::Resize(size.width)),
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::TaskDrop)
            }
            Event::Mouse(iced::mouse::Event::CursorLeft)
            | Event::Window(window::Event::Unfocused) => Some(Message::TaskDragCancel),
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if status == event::Status::Ignored
                    || matches!(
                        key,
                        keyboard::Key::Named(
                            keyboard::key::Named::Escape
                                | keyboard::key::Named::F4
                                | keyboard::key::Named::F5
                                | keyboard::key::Named::F6
                                | keyboard::key::Named::F8
                        )
                    ) =>
            {
                Some(Message::Key(key, modifiers))
            }
            _ => None,
        });
        Subscription::batch([events, Subscription::run(clock)])
    }
}

fn clock() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(1, |mut output| async move {
        use iced::futures::{SinkExt, StreamExt};
        let (mut tx, mut rx) = iced::futures::channel::mpsc::channel(1);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            if tx.try_send(()).is_err() {
                break;
            }
        });
        while rx.next().await.is_some() {
            if output.send(Message::Tick).await.is_err() {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ebbi::domain::Todo;

    #[test]
    fn completed_tasks_hide_at_six_hours_without_hiding_unknown_or_open_tasks() {
        let (mut app, _) = App::new(Err("test".into()), Screen::Dashboard);
        let completed = chrono::DateTime::parse_from_rfc3339("2026-10-15T10:00:00-05:00")
            .unwrap()
            .to_utc();
        let task = |id, done, timestamp| Todo {
            id,
            title: format!("Task {id}"),
            notes: String::new(),
            due_date: None,
            completed: done,
            completed_at: timestamp,
            position: id,
        };
        app.data = Some(Snapshot {
            tasks: vec![
                task(1, true, Some(completed.to_rfc3339())),
                task(2, false, Some(completed.to_rfc3339())),
                task(3, true, None),
                task(4, true, Some("unknown".into())),
            ],
            ..Default::default()
        });
        assert_eq!(
            app.tasks_at(completed + chrono::Duration::hours(6) - chrono::Duration::seconds(1))
                .len(),
            4
        );
        assert_eq!(
            app.tasks_at(completed + chrono::Duration::hours(6))
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(app.data.as_ref().unwrap().tasks.len(), 4);
    }

    #[test]
    fn task_drag_targets_use_full_order_and_cancel_outside_the_list() {
        let (mut app, _) = App::new(Err("test".into()), Screen::Dashboard);
        app.data = Some(Snapshot {
            tasks: [10, 20, 30]
                .into_iter()
                .map(|id| Todo {
                    id,
                    title: format!("Task {id}"),
                    notes: String::new(),
                    due_date: None,
                    completed: id == 20,
                    completed_at: None,
                    position: id,
                })
                .collect(),
            ..Default::default()
        });
        assert!(matches!(
            app.task_move_action(10, 30),
            Some(Action::MoveTask { id: 10, delta: 2 })
        ));
        assert!(matches!(
            app.task_move_action(30, 10),
            Some(Action::MoveTask { id: 30, delta: -2 })
        ));
        assert!(app.task_move_action(10, 10).is_none());
        let _ = app.update(Message::TaskDragStart(10));
        let _ = app.update(Message::TaskDragOver(30));
        assert_eq!((app.drag_task, app.drop_task), (Some(10), Some(30)));
        let _ = app.update(Message::TaskDragLeave(30));
        let _ = app.update(Message::TaskDrop);
        assert_eq!((app.drag_task, app.drop_task), (None, None));
        assert!(!app.busy);
        let _ = app.update(Message::TaskDragStart(10));
        let _ = app.update(Message::TaskDragCancel);
        assert_eq!((app.drag_task, app.drop_task), (None, None));
    }

    #[test]
    fn tasks_show_all_dates_and_completion_states_in_saved_order() {
        let (mut app, _) = App::new(Err("test".into()), Screen::Dashboard);
        let task = |id, completed, due_date| Todo {
            id,
            title: "Task".into(),
            notes: String::new(),
            due_date,
            completed,
            completed_at: None,
            position: id,
        };
        app.data = Some(Snapshot {
            tasks: vec![
                task(1, false, None),
                task(2, false, Some(app.date)),
                task(3, false, app.date.succ_opt()),
                task(4, true, None),
            ],
            ..Default::default()
        });
        assert_eq!(
            app.tasks().iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn task_creation_and_editing_have_distinct_keys() {
        let (mut app, _) = App::new(Err("test".into()), Screen::Dashboard);
        app.data = Some(Snapshot {
            tasks: vec![Todo {
                id: 1,
                title: "Existing task".into(),
                notes: "Keep these notes\n".into(),
                due_date: Some(app.date),
                completed: false,
                completed_at: None,
                position: 0,
            }],
            ..Default::default()
        });
        app.selected = Some(Selection::Task(1));
        let _ = app.key(
            keyboard::Key::Character("i".into()),
            keyboard::Modifiers::empty(),
        );
        let form = app.form.as_ref().unwrap();
        assert!(matches!(form.kind, FormKind::Task(None)));
        assert!(form.title.is_empty());
        let _ = app.update(Message::Cancel);
        let _ = app.key(
            keyboard::Key::Named(keyboard::key::Named::Enter),
            keyboard::Modifiers::empty(),
        );
        let form = app.form.as_ref().unwrap();
        assert!(matches!(form.kind, FormKind::Task(Some(1))));
        assert_eq!(form.title, "Existing task");
        assert_eq!(form.notes_text(), "Keep these notes\n");
        assert_eq!(form.due, app.date.to_string());
        let _ = app.update(Message::Cancel);
        assert_eq!(app.data.as_ref().unwrap().tasks[0].title, "Existing task");
    }

    #[test]
    fn typing_modes_and_failed_saves_preserve_the_form() {
        let (mut app, _) = App::new(Err("test".into()), Screen::Dashboard);
        let _ = app.open_form(FormKind::Task(None));
        let _ = app.update(Message::Title("j keeps typing".into()));
        let _ = app.key(
            keyboard::Key::Character("h".into()),
            keyboard::Modifiers::empty(),
        );
        assert_eq!(app.date, app.today);
        let _ = app.update(Message::Loaded(Err("Disk full".into())));
        assert_eq!(app.form.as_ref().unwrap().title, "j keeps typing");
        assert!(app.error.as_ref().unwrap().contains("Disk full"));
        for notes in ["", "One line", "First\nSecond", "Intentional newline\n"] {
            let form = app.form.as_mut().unwrap();
            form.notes_original = notes.into();
            form.notes = iced::widget::text_editor::Content::with_text(&format!("{notes}\n"));
            form.notes_dirty = false;
            assert_eq!(form.notes_text(), notes);
            form.notes_dirty = true;
            assert_eq!(form.notes_text(), notes);
        }
        let _ = app.update(Message::Cancel);
        assert!(app.form.is_none());
        let _ = app.key(
            keyboard::Key::Named(keyboard::key::Named::ArrowDown),
            keyboard::Modifiers::empty(),
        );
        assert!(app.selected.is_none());
    }
    #[test]
    fn study_search_and_reschedule_require_an_unchanged_preview() {
        let (mut app, _) = App::new(Err("test".into()), Screen::Study);
        app.data = Some(Snapshot {
            study_topics: vec![StudyTopic {
                id: 1,
                title: "Rust".into(),
                notes: "**Ownership**".into(),
                initial_date: Some(app.today),
                completed: false,
            }],
            study_reviews: vec![StudyReview {
                id: 1,
                topic_id: 1,
                stage: 1,
                original_due_date: Some(app.today),
                scheduled_date: app.today,
                completed_at: None,
                completed_date: None,
            }],
            ..Default::default()
        });
        let _ = app.update(Message::StudySearch("ownership".into()));
        assert_eq!(app.study_topics().len(), 1);
        let _ = app.key(
            keyboard::Key::Character("j".into()),
            keyboard::Modifiers::empty(),
        );
        assert_eq!(app.study_selected, Some(1));
        let _ = app.key(
            keyboard::Key::Character("r".into()),
            keyboard::Modifiers::empty(),
        );
        assert!(matches!(
            app.form.as_ref().unwrap().kind,
            FormKind::ReviewDate(1)
        ));
        let _ = app.update(Message::Cancel);
        let _ = app.open_form(FormKind::StudyDate(1));
        let _ = app.update(Message::Due(format!(" {} ", app.today)));
        let _ = app.save();
        assert!(app.form.as_ref().unwrap().confirmed);
        assert!(!app.close_form_after_save);
        let _ = app.update(Message::Due(app.today.succ_opt().unwrap().to_string()));
        assert!(!app.form.as_ref().unwrap().confirmed);
        let _ = app.update(Message::Cancel);
        assert_eq!(
            app.data.as_ref().unwrap().study_reviews[0].scheduled_date,
            app.today
        );
        let _ = app.open_form(FormKind::HabitValue(1));
        let selected_date = app.date;
        app.date = app.date.succ_opt().unwrap();
        assert_eq!(app.form.as_ref().unwrap().entry_date, selected_date);
        let _ = app.open_form(FormKind::NumericHabit);
        let _ = app.update(Message::Title("Reading".into()));
        let _ = app.update(Message::Bound("NaN".into()));
        let _ = app.save();
        assert!(app.error.as_ref().unwrap().contains("finite"));
        assert!(app.form.is_some());
    }
    #[test]
    fn study_agenda_orders_reviews_and_filters_independently_of_search() {
        let (mut app, _) = App::new(Err("test".into()), Screen::Study);
        let today = app.today;
        let tomorrow = today.succ_opt().unwrap();
        let review = |id, date, done| StudyReview {
            id,
            topic_id: 1,
            stage: 1,
            original_due_date: Some(date),
            scheduled_date: date,
            completed_at: if done { Some("legacy".into()) } else { None },
            completed_date: None,
        };
        app.data = Some(Snapshot {
            study_topics: vec![StudyTopic {
                id: 1,
                title: "Module 5".into(),
                notes: "Networking".into(),
                initial_date: Some(today),
                completed: false,
            }],
            study_reviews: vec![
                review(3, tomorrow, false),
                review(4, today.pred_opt().unwrap(), true),
                review(2, today, false),
                review(1, today.pred_opt().unwrap(), false),
            ],
            ..Default::default()
        });
        assert_eq!(
            app.study_reviews().iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        let _ = app.update(Message::StudyDay(Some(today)));
        assert_eq!(
            app.study_reviews().iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        let _ = app.update(Message::StudyDay(Some(tomorrow)));
        assert_eq!(
            app.study_reviews().iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![3]
        );
        let _ = app.update(Message::StudySearch("network".into()));
        assert_eq!(app.study_reviews().len(), 1);
        let _ = app.key(
            keyboard::Key::Named(keyboard::key::Named::Escape),
            keyboard::Modifiers::empty(),
        );
        assert!(app.study_search.is_empty());
        assert_eq!(app.study_date, Some(tomorrow));
        let _ = app.key(
            keyboard::Key::Named(keyboard::key::Named::Escape),
            keyboard::Modifiers::empty(),
        );
        assert!(app.study_date.is_none());
        assert_eq!(app.study_reviews().len(), 4);
    }
}
