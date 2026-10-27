use crate::app::{App, FormKind, GoalMode, Message, Screen, Selection};
use ebbi::domain::{Action, HabitKind, LegacyKind, NumericGoal};
use iced::widget::{
    button, checkbox, column, container, horizontal_rule, markdown, mouse_area, pick_list, row,
    scrollable, text, text_editor, text_input, Space,
};
use iced::{Alignment, Border, Color, Element, Length, Theme};

pub(crate) const MUTED: Color = Color::from_rgb(0.65, 0.70, 0.73);
pub(crate) const ACCENT: Color = Color::from_rgb(0.52, 0.87, 0.72);
const ERROR: Color = Color::from_rgb(1.0, 0.64, 0.61);

pub fn theme() -> Theme {
    Theme::custom(
        "Ebbi dark".into(),
        iced::theme::Palette {
            background: Color::from_rgb(0.055, 0.071, 0.082),
            text: Color::from_rgb(0.91, 0.94, 0.94),
            primary: ACCENT,
            success: ACCENT,
            danger: ERROR,
        },
    )
}

pub(crate) fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb(0.085, 0.105, 0.118).into()),
        border: Border {
            color: Color::from_rgb(0.16, 0.20, 0.22),
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}

pub(crate) fn action<'a>(
    label: impl Into<String>,
    message: Message,
    enabled: bool,
) -> iced::widget::Button<'a, Message> {
    button(text(label.into()).size(13))
        .style(button::secondary)
        .padding([7, 10])
        .on_press_maybe(enabled.then_some(message))
}

pub fn view(app: &App) -> Element<'_, Message> {
    let ready = app.data.is_some();
    let header = row![
        text("E B B I").size(20).color(ACCENT),
        Space::with_width(Length::Fill),
        action("Today", Message::Navigate(Screen::Dashboard), !app.busy),
        action("Study", Message::Navigate(Screen::Study), !app.busy),
        action("Analytics", Message::Navigate(Screen::Analytics), !app.busy),
        action("Settings", Message::Navigate(Screen::Settings), true),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let mut body = if app.screen == Screen::Study {
        column![]
    } else {
        column![header, horizontal_rule(1)]
    };
    body = body.spacing(12).padding([12, 24]).width(Length::Fill);
    if let Some(error) = &app.error {
        body = body.push(
            container(
                column![
                    text(error).color(ERROR).size(14),
                    row![
                        action("Retry / reconnect", Message::Retry, !app.busy),
                        action("Reload", Message::Refresh, !app.busy)
                    ]
                    .spacing(8),
                ]
                .spacing(10),
            )
            .padding(14)
            .style(panel)
            .width(Length::Fill),
        );
    }
    if let Some(notice) = &app.notice {
        body = body.push(
            row![
                text(notice).color(ACCENT).size(13).width(Length::Fill),
                action("Dismiss", Message::Dismiss, true)
            ]
            .spacing(8),
        );
    }
    let inline_task = app
        .form
        .as_ref()
        .is_some_and(|form| matches!(form.kind, FormKind::Task(_)));
    let delete_habit = app
        .form
        .as_ref()
        .is_some_and(|form| matches!(form.kind, FormKind::DeleteHabit(_)));
    if app.form.is_some() && !inline_task && !delete_habit && app.screen != Screen::Study {
        body = body.push(form(app));
    } else {
        body = body.push(match app.screen {
            Screen::Dashboard => dashboard(app, ready),
            Screen::Settings => settings(app, ready),
            Screen::Study => crate::study_ui::view(app),
            Screen::Analytics => crate::analytics_ui::view(app),
        });
    }
    let footer = if inline_task {
        "Enter save task · Esc cancel"
    } else if delete_habit {
        "Y yes · N no · Esc cancel"
    } else if app.form.is_some() {
        "Esc cancel · Tab move between fields"
    } else if app.screen == Screen::Study {
        "N add topic · H/L day · J/K navigate · I edit · D delete · Space complete · Esc clear"
    } else if app.screen == Screen::Analytics {
        "Left/Right inspect dates · 1/2/3 range · F1 dashboard · F2 study"
    } else if app.screen == Screen::Settings {
        "Tab fields · Enter preview/backup · F5 habits · F6 study · F8 confirm import · Esc back"
    } else {
        "j/k select within column · h habits / l tasks · Up/Down move task · i add task · Enter edit · d delete · Space complete · t today · a habit"
    };
    let hint = text(footer)
        .font(if app.screen == Screen::Study {
            iced::Font::MONOSPACE
        } else {
            iced::Font::DEFAULT
        })
        .size(10)
        .color(MUTED);
    let bottom: Element<'_, Message> = if app.screen == Screen::Study {
        let nav = row![
            button(text("F1 Today").font(iced::Font::MONOSPACE).size(9))
                .style(button::text)
                .on_press(Message::Navigate(Screen::Dashboard)),
            button(text("F3 Analytics").font(iced::Font::MONOSPACE).size(9))
                .style(button::text)
                .on_press(Message::Navigate(Screen::Analytics)),
            button(text("Settings").font(iced::Font::MONOSPACE).size(9))
                .style(button::text)
                .on_press(Message::Navigate(Screen::Settings)),
        ]
        .spacing(6);
        if app.width >= 900.0 {
            row![hint.width(Length::Fill), nav]
                .spacing(10)
                .align_y(Alignment::Center)
                .into()
        } else {
            column![hint, nav].spacing(4).into()
        }
    } else {
        hint.into()
    };
    let base: Element<'_, Message> = column![
        scrollable(body)
            .id(iced::widget::scrollable::Id::new("main-scroll"))
            .height(Length::Fill),
        container(bottom).padding([10, 24]).width(Length::Fill)
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into();
    if delete_habit || (app.screen == Screen::Study && app.form.is_some()) {
        let mut modal = column![].spacing(10);
        if let Some(error) = &app.error {
            modal = modal.push(text(error).color(ERROR).size(14));
        }
        modal = modal.push(form(app));
        let dialog = container(scrollable(modal))
            .max_width(if delete_habit { 460 } else { 680 })
            .height(if delete_habit {
                Length::Shrink
            } else {
                Length::Fill
            })
            .padding([24, 0]);
        let backdrop = container(dialog)
            .padding([0, 24])
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.65).into()),
                ..Default::default()
            });
        iced::widget::stack![base, iced::widget::opaque(backdrop)].into()
    } else {
        base
    }
}

fn dashboard(app: &App, ready: bool) -> Element<'_, Message> {
    let date = row![
        action("‹", Message::DateStep(-1), true),
        text(app.date.format("%A").to_string())
            .size(24)
            .color(ACCENT),
        text_input("YYYY-MM-DD", &app.date_input)
            .on_input(Message::DateInput)
            .on_submit(Message::GoDate)
            .size(13)
            .padding(6)
            .width(116),
        action("Go", Message::GoDate, true),
        action("›", Message::DateStep(1), true),
        Space::with_width(Length::Fill),
        action("Today", Message::Today, true),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut body = column![date].spacing(12);
    let Some(data) = &app.data else {
        return body
            .push(
                text(if app.busy {
                    "Opening your local workspace…"
                } else {
                    "Your database could not be opened. Retry above or check its path in Settings."
                })
                .color(MUTED),
            )
            .into();
    };
    if data.habits.iter().any(|h| !h.eligibility_known) {
        body=body.push(text("Some legacy schedules are unknown. Their records are shown, but excluded from exact progress.").size(13).color(MUTED));
    }
    if app.date > app.today {
        body = body.push(
            text("Future day — habit completion is disabled.")
                .size(13)
                .color(MUTED),
        );
    }
    let habits = habit_list(app, ready);
    let tasks = task_list(app, ready);
    if app.width >= 900.0 {
        body = body.push(row![habits, tasks].spacing(18));
    } else {
        body = body.push(column![habits, tasks].spacing(18));
    }
    body.into()
}

fn habit_list(app: &App, ready: bool) -> Element<'_, Message> {
    let mut list = column![text("Daily habits").size(20)].spacing(12);
    let data = app.data.as_ref().expect("dashboard checked data");
    let habits: Vec<_> = data
        .habits
        .iter()
        .filter(|h| {
            !h.archived
                || h.eligible
                || h.completed
                || (!h.eligibility_known && app.date <= app.today)
        })
        .collect();
    if habits.is_empty() {
        list = list.push(
            text("Start small. Add something you want to do every day.")
                .color(MUTED)
                .size(14),
        );
    }
    for habit in habits {
        let selected = app.selected == Some(Selection::Habit(habit.id));
        let check: Element<'_, Message> = if habit.kind == HabitKind::Numeric {
            let label = format!(
                "{} {} · {}",
                habit
                    .value
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".into()),
                habit.unit,
                if habit.completed { "met" } else { "pending" }
            );
            iced::widget::tooltip(
                action(
                    label,
                    Message::OpenForm(FormKind::HabitValue(habit.id)),
                    ready
                        && !app.busy
                        && habit.eligible
                        && habit.eligibility_known
                        && app.date <= app.today,
                ),
                text(format!(
                    "Goal: {} {}",
                    habit
                        .goal
                        .as_ref()
                        .map(goal_text)
                        .unwrap_or_else(|| "unknown".into()),
                    habit.unit
                )),
                iced::widget::tooltip::Position::Top,
            )
            .into()
        } else {
            checkbox("", habit.completed)
                .on_toggle_maybe(
                    (ready && habit.eligible && habit.eligibility_known && app.date <= app.today)
                        .then_some(move |completed| {
                            Message::Act(Action::SetHabitCompletion {
                                id: habit.id,
                                date: app.date,
                                completed,
                            })
                        }),
                )
                .into()
        };
        let item = row![
            check,
            button(
                text(format!(
                    "{}{}",
                    if selected { "› " } else { "" },
                    habit.name
                ))
                .size(17)
            )
            .on_press_maybe(ready.then_some(Message::Select(Selection::Habit(habit.id))))
            .padding([7, 10])
            .width(Length::Fill)
            .style(button::text)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        list = list.push(container(item).padding(12).style(panel).width(Length::Fill));
    }
    container(list)
        .padding(16)
        .style(panel)
        .width(Length::Fill)
        .into()
}

fn task_title_input<'a>(app: &App, title: &'a str) -> Element<'a, Message> {
    row![
        text("> ").color(ACCENT),
        text_input("Enter task title… (Enter to save, Esc to cancel)", title)
            .id("form-title")
            .on_input_maybe((!app.busy).then_some(Message::Title))
            .on_submit_maybe((!app.busy).then_some(Message::Save))
            .padding(12)
    ]
    .align_y(Alignment::Center)
    .spacing(8)
    .into()
}

fn task_list(app: &App, ready: bool) -> Element<'_, Message> {
    let mut list = column![text("Tasks").size(20)].spacing(12);
    if let Some(form) = app
        .form
        .as_ref()
        .filter(|form| matches!(form.kind, FormKind::Task(None)))
    {
        list = list.push(task_title_input(app, &form.title));
    }
    let tasks = app.tasks();
    if tasks.is_empty() {
        list = list.push(
            text("No tasks yet. Press i to add one.")
                .color(MUTED)
                .size(14),
        );
    }
    for task in tasks {
        if let Some(form) = app
            .form
            .as_ref()
            .filter(|form| matches!(form.kind, FormKind::Task(Some(id)) if id == task.id))
        {
            list = list.push(task_title_input(app, &form.title));
            continue;
        }
        let selected = app.selected == Some(Selection::Task(task.id));
        let status = if task.completed {
            None
        } else {
            task.due_date.map(|d| {
                if d < app.today {
                    format!("Overdue · {d}")
                } else if d == app.today {
                    "Due today".into()
                } else {
                    format!("Due {d}")
                }
            })
        };
        let check =
            checkbox("", task.completed).on_toggle_maybe(ready.then_some(move |completed| {
                Message::Act(Action::SetTaskCompleted {
                    id: task.id,
                    completed,
                })
            }));
        let mut item = column![row![
            check,
            text(format!(
                "{}{}",
                if selected { "› " } else { "" },
                task.title
            ))
            .size(17)
            .width(Length::Fill)
        ]
        .spacing(8)
        .align_y(Alignment::Center)]
        .spacing(7);
        if let Some(status) = status {
            item = item.push(text(status).size(12).color(MUTED));
        }
        if !task.notes.is_empty() {
            item = item.push(text(&task.notes).size(13).color(MUTED));
        }
        let card = container(item)
            .padding(12)
            .style(move |theme| {
                let mut style = panel(theme);
                if app.drag_task.is_some() && app.drop_task == Some(task.id) {
                    style.border.color = ACCENT;
                    style.border.width = 2.0;
                }
                style
            })
            .width(Length::Fill);
        let card = mouse_area(card)
            .on_press(Message::TaskDragStart(task.id))
            .on_enter(Message::TaskDragOver(task.id))
            .on_move(move |_| Message::TaskDragOver(task.id))
            .on_exit(Message::TaskDragLeave(task.id))
            .interaction(if app.drag_task.is_some() {
                iced::mouse::Interaction::Grabbing
            } else {
                iced::mouse::Interaction::Grab
            });
        list = list.push(card);
    }
    container(list)
        .padding(16)
        .style(panel)
        .width(Length::Fill)
        .into()
}

fn form(app: &App) -> Element<'_, Message> {
    let form = app.form.as_ref().expect("view checked form");
    let ready = !app.busy;
    if matches!(form.kind, FormKind::DeleteHabit(_)) {
        return container(
            column![
                text("Do you want to delete a habit?").size(20),
                text(&form.title),
                text("Its tracking history will also be deleted.")
                    .size(13)
                    .color(MUTED),
                row![
                    action("Yes", Message::Save, ready),
                    action("No", Message::Cancel, ready),
                ]
                .spacing(10),
            ]
            .spacing(12),
        )
        .padding(20)
        .style(panel)
        .width(Length::Fill)
        .into();
    }
    if matches!(
        form.kind,
        FormKind::DeleteTask(_) | FormKind::DeleteStudy(_)
    ) {
        return container(
            column![
                text(if matches!(form.kind, FormKind::DeleteTask(_)) {
                    "Delete this task?"
                } else {
                    "Delete this topic and its review history?"
                })
                .size(24),
                text(&form.title),
                text("Press Enter to delete, or Esc to cancel.").color(MUTED),
                row![
                    action("Delete", Message::Save, ready),
                    action("Cancel", Message::Cancel, ready)
                ]
                .spacing(10)
            ]
            .spacing(18),
        )
        .padding(24)
        .style(panel)
        .width(Length::Fill)
        .into();
    }
    let title = match form.kind {
        FormKind::Habit(None) => "New daily habit",
        FormKind::Habit(Some(_)) => "Rename habit",
        FormKind::NumericHabit => "New numeric habit",
        FormKind::HabitValue(_) => "Record value",
        FormKind::HabitGoal(_) => "Change goal",
        FormKind::Study(None) => "New study topic",
        FormKind::Study(Some(_)) => "Edit study topic",
        FormKind::StudyDate(_) => "Change study date",
        FormKind::ReviewDate(_) => "Reschedule unfinished review",
        _ => "Edit task",
    };
    let mut content = column![text(title).size(25)].spacing(12);
    if matches!(form.kind, FormKind::Habit(None) | FormKind::NumericHabit) {
        content = content.push(
            row![
                action("Binary", Message::OpenForm(FormKind::Habit(None)), ready),
                action(
                    "Numeric [F4]",
                    Message::OpenForm(FormKind::NumericHabit),
                    ready
                )
            ]
            .spacing(8),
        );
    }
    if matches!(
        form.kind,
        FormKind::Habit(_) | FormKind::NumericHabit | FormKind::Study(_) | FormKind::Task(_)
    ) {
        content = content.push(
            text_input("Title", &form.title)
                .id("form-title")
                .on_input_maybe(ready.then_some(Message::Title))
                .on_submit_maybe(ready.then_some(Message::Save))
                .padding(12),
        );
    } else if !form.title.is_empty() {
        content = content.push(text(&form.title).size(17));
    }
    if matches!(form.kind, FormKind::NumericHabit) {
        content = content.push(
            text_input("Unit (minutes, pages, kcal…)", &form.unit)
                .on_input_maybe(ready.then_some(Message::Unit))
                .padding(12),
        );
    }
    if matches!(form.kind, FormKind::NumericHabit | FormKind::HabitGoal(_)) {
        content = content.push(
            text("Goal rule · F5 cycles minimum / maximum / range")
                .size(12)
                .color(MUTED),
        );
        content = content.push(pick_list(
            [GoalMode::Minimum, GoalMode::Maximum, GoalMode::Range],
            Some(form.goal_mode),
            Message::GoalMode,
        ));
    }
    if matches!(
        form.kind,
        FormKind::NumericHabit | FormKind::HabitGoal(_) | FormKind::HabitValue(_)
    ) {
        content = content.push(
            text_input(
                if matches!(form.kind, FormKind::HabitValue(_)) {
                    "Value (blank clears it)"
                } else {
                    "Target / lower bound"
                },
                &form.bound,
            )
            .id("numeric-value")
            .on_input_maybe(ready.then_some(Message::Bound))
            .on_submit_maybe(ready.then_some(Message::Save))
            .padding(12),
        );
        if !matches!(form.kind, FormKind::HabitValue(_)) && form.goal_mode == GoalMode::Range {
            content = content.push(
                text_input("Upper bound", &form.upper)
                    .on_input_maybe(ready.then_some(Message::Upper))
                    .on_submit_maybe(ready.then_some(Message::Save))
                    .padding(12),
            );
        }
        if matches!(form.kind, FormKind::HabitValue(_)) {
            content = content.push(
                text(format!(
                    "{} · {} · Missing is not zero",
                    form.entry_date, form.unit
                ))
                .color(MUTED),
            );
        }
    }
    if matches!(form.kind, FormKind::Study(_)) {
        content = content
            .push(text("Markdown notes").color(MUTED))
            .push(
                text_editor(&form.notes)
                    .key_binding(|press| {
                        if press.key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab)
                        {
                            None
                        } else {
                            iced::widget::text_editor::Binding::from_key_press(press)
                        }
                    })
                    .placeholder("Optional notes")
                    .on_action(Message::Notes)
                    .height(180)
                    .padding(12),
            )
            .push(action(
                "Preview Markdown [F4]",
                Message::PreviewNotes,
                ready,
            ));
        if !app.markdown.is_empty() {
            content = content.push(
                markdown::view(
                    &app.markdown,
                    markdown::Settings::default(),
                    markdown::Style::from_palette(theme().palette()),
                )
                .map(|url| Message::Link(url.to_string())),
            );
        }
    }
    if matches!(
        form.kind,
        FormKind::Study(None)
            | FormKind::StudyDate(_)
            | FormKind::ReviewDate(_)
            | FormKind::HabitGoal(_)
    ) {
        content = content
            .push(
                text(if matches!(form.kind, FormKind::HabitGoal(_)) {
                    "Effective from (tomorrow or later)"
                } else {
                    "Date (YYYY-MM-DD)"
                })
                .color(MUTED),
            )
            .push(
                text_input("YYYY-MM-DD", &form.due)
                    .id("form-date")
                    .on_input_maybe(ready.then_some(Message::Due))
                    .on_submit_maybe(ready.then_some(Message::Save))
                    .padding(12),
            );
    }
    if let FormKind::StudyDate(id) = form.kind {
        content = content.push(text("Only unfinished reviews move. Original deadlines and completed history are preserved.").size(13).color(MUTED));
        if form.confirmed {
            if let Ok(date) = form.due.trim().parse() {
                if let Ok(dates) = ebbi::analytics::fixed_review_dates(date) {
                    for review in app
                        .topic_reviews(id)
                        .into_iter()
                        .filter(|r| r.completed_at.is_none())
                    {
                        content = content.push(text(format!(
                            "R{}: {} to {}",
                            review.stage,
                            review.scheduled_date,
                            dates[usize::from(review.stage) - 1]
                        )));
                    }
                }
            }
        }
    }
    content = content.push(
        row![
            action(
                if matches!(form.kind, FormKind::StudyDate(_)) {
                    if form.confirmed {
                        "Confirm reschedule"
                    } else {
                        "Preview changes"
                    }
                } else {
                    "Save"
                },
                Message::Save,
                ready
            ),
            action("Cancel", Message::Cancel, ready)
        ]
        .spacing(10),
    );
    container(content)
        .padding(24)
        .style(panel)
        .width(Length::Fill)
        .into()
}

pub(crate) fn goal_text(goal: &NumericGoal) -> String {
    match goal {
        NumericGoal::AtLeast(v) => format!(">= {v}"),
        NumericGoal::AtMost(v) => format!("<= {v}"),
        NumericGoal::Between { min, max } => format!("{min}–{max}"),
    }
}

fn settings(app: &App, ready: bool) -> Element<'_, Message> {
    let db_path = app
        .db_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| e.clone());
    let mut content=column![text("Your workspace").size(29),text("Local storage").size(18),text(db_path.clone()).size(13).color(MUTED),text("One rolling daily backup after changes. Migration and import snapshots are retained.").size(13).color(MUTED),
        text_input("Backup destination (.db)",&app.backup_path).on_input(Message::BackupPath).on_submit(Message::Backup).padding(10),row![action("Choose destination…",Message::BrowseBackup,!app.busy),action("Back up now",Message::Backup,ready && !app.backup_path.trim().is_empty())].spacing(8),horizontal_rule(1),
        text("Restore an Ebbi backup").size(20),
        text("Restore an earlier snapshot of this workspace. Your current data is preserved in a permanent recovery backup before replacement.").size(13).color(MUTED),
        text_input("Path to Ebbi backup", &app.restore_path).on_input(Message::RestorePath).on_submit(Message::Restore).padding(10),
        row![action("Choose backup…", Message::BrowseRestore, !app.busy), action("Restore backup…", Message::Restore, ready && !app.busy && !app.restore_path.trim().is_empty())].spacing(8)].spacing(14);
    if app.restore_confirm {
        content = content.push(container(column![
            text("Replace this workspace with the selected backup?").size(18).color(ERROR),
            text(format!("Backup: {}", app.restore_path.trim())).size(13),
            text(format!("Workspace: {db_path}")).size(13),
            text("Changes made after this backup will be removed from the workspace. A verified recovery copy of the current data will be retained.").size(13),
            row![action("Confirm restore [F8]", Message::ConfirmRestore, ready && !app.busy), action("Cancel", Message::CancelRestore, !app.busy)].spacing(8)
        ].spacing(10)).padding(16).style(panel));
    }
    content = content.push(horizontal_rule(1)).push(column![text("Import legacy history").size(20),text("Choose a source and inspect a verified snapshot before importing. Existing apps keep their own databases.").size(13).color(MUTED),
        row![action("Habit Tracker [F5]",Message::SourceKind(LegacyKind::Habitodo),!app.busy),action("Repasito [F6]",Message::SourceKind(LegacyKind::Repasito),!app.busy)].spacing(8),
        text(format!("Source: {}",app.source_kind)).size(13),text_input("Path to legacy SQLite database",&app.source_path).on_input(Message::SourcePath).on_submit(Message::Preview).padding(10),row![action("Browse…",Message::BrowseSource,!app.busy),action("Preview import",Message::Preview,ready && !app.source_path.trim().is_empty())].spacing(8)].spacing(14));
    if let Some(preview) = &app.preview {
        let mut report = column![
            text("Import preview").size(18),
            text(format!("Source: {}", preview.source_path.display())).size(12),
            text(format!(
                "Verified snapshot: {}",
                preview.snapshot_path.display()
            ))
            .size(12)
            .color(MUTED)
        ]
        .spacing(8);
        for (kind, count) in &preview.counts {
            report = report.push(text(format!("{kind}: {count}")).size(14));
        }
        for warning in &preview.warnings {
            report = report.push(text(warning).size(13).color(MUTED));
        }
        report = report.push(
            row![
                action("Confirm import [F8]", Message::Import, ready),
                action("Cancel", Message::Cancel, !app.busy)
            ]
            .spacing(8),
        );
        content = content.push(container(report).padding(16).style(panel));
    }
    content = content
        .push(horizontal_rule(1))
        .push(
            row![
                text("Habit management").size(20).width(Length::Fill),
                action(
                    "New habit",
                    Message::OpenForm(FormKind::Habit(None)),
                    ready && !app.busy
                )
            ]
            .spacing(8),
        )
        .push(
            text("Archive and restore take effect tomorrow, preserving today and earlier records.")
                .size(13)
                .color(MUTED),
        );
    if let Some(data) = &app.data {
        for habit in &data.habits {
            if let Some((date, goal)) = &habit.next_goal {
                content = content.push(
                    text(format!(
                        "{} · pending goal {} {} from {}",
                        habit.name,
                        goal_text(goal),
                        habit.unit,
                        date
                    ))
                    .size(12)
                    .color(ACCENT),
                );
            }
            if habit.kind == HabitKind::Numeric {
                content = content.push(
                    row![
                        text(format!(
                            "{} · {} {}",
                            habit.name,
                            habit.goal.as_ref().map(goal_text).unwrap_or_default(),
                            habit.unit
                        ))
                        .width(Length::Fill),
                        action(
                            "Change future goal",
                            Message::OpenForm(FormKind::HabitGoal(habit.id)),
                            ready && !app.busy
                        )
                    ]
                    .spacing(8),
                );
            }
            content = content.push(
                row![
                    text(&habit.name).size(14).width(Length::Fill),
                    text(if habit.archived { "Archived" } else { "Active" })
                        .size(12)
                        .color(MUTED),
                    action(
                        if habit.archived { "Restore" } else { "Archive" },
                        Message::Act(Action::SetHabitArchived {
                            id: habit.id,
                            archived: !habit.archived
                        }),
                        ready
                    )
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
    }
    content=content.push(horizontal_rule(1)).push(text("Release scope").size(18)).push(text("This build includes the daily dashboard, study, habit analytics, single-instance view routing and backup restore.").size(13).color(MUTED));
    content.into()
}
