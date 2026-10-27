use crate::app::{App, Message};
use crate::charts::Plot;
use crate::ui::{action, goal_text, panel, ACCENT, MUTED};
use ebbi::domain::{Counts, NumericGoal};
use iced::widget::{canvas, column, container, progress_bar, row, text, Space};
use iced::{Color, Element, Length};

fn counts(counts: &Counts) -> String {
    match counts.percentage() {
        Some(percent) => format!("{}/{} · {percent:.1}%", counts.successful, counts.eligible),
        None => "N/A · no known opportunities".into(),
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let mut body = column![
        row![
            text("Analytics").size(26),
            Space::with_width(Length::Fill),
            action("‹ day", Message::DateStep(-1), !app.busy),
            action("Today", Message::Today, !app.busy),
            action("day ›", Message::DateStep(1), !app.busy)
        ]
        .spacing(8),
        row![
            action("7 days", Message::AnalyticsDays(7), !app.busy),
            action("30 days", Message::AnalyticsDays(30), !app.busy),
            action("90 days", Message::AnalyticsDays(90), !app.busy)
        ]
        .spacing(8),
    ]
    .spacing(14);
    let Some(report) = &app.analytics else {
        return body
            .push(
                text(if app.busy {
                    "Calculating local history…"
                } else {
                    "No report available. Reload to retry."
                })
                .color(MUTED),
            )
            .into();
    };
    body = body.push(
        text(format!(
            "{} — {} · {} · Today is provisional",
            report.start,
            report.end,
            counts(&report.total)
        ))
        .color(ACCENT),
    );
    let labels = report
        .daily
        .iter()
        .map(|d| {
            format!(
                "{} · {} · {} unknown",
                d.date,
                counts(&d.counts),
                d.unknown_habits
            )
        })
        .collect();
    body = body.push(
        canvas(Plot {
            series: vec![(
                ACCENT,
                report.daily.iter().map(|d| d.counts.percentage()).collect(),
            )],
            labels,
            maximum: 100.0,
        })
        .width(Length::Fill)
        .height(200),
    );
    let index = app.inspected_day.min(report.daily.len().saturating_sub(1));
    if let Some(day) = report.daily.get(index) {
        body = body.push(
            row![
                action(
                    "‹ inspect",
                    Message::InspectDay(index.saturating_sub(1)),
                    true
                ),
                text(format!(
                    "{} · {} · {:?} · {} unknown",
                    day.date,
                    counts(&day.counts),
                    day.state,
                    day.unknown_habits
                ))
                .size(13)
                .width(Length::Fill),
                action(
                    "inspect ›",
                    Message::InspectDay((index + 1).min(report.daily.len() - 1)),
                    true
                )
            ]
            .spacing(8),
        );
    }
    body = body.push(text("Habit consistency and streaks").size(20));
    for habit in &report.habits {
        let mut card = column![
            text(format!("{} · {}", habit.name, counts(&habit.consistency))).size(16),
            progress_bar(
                0.0..=100.0,
                habit.consistency.percentage().unwrap_or(0.0) as f32
            ),
            text(format!(
                "Current streak: {} · longest: {} known eligible days",
                habit.current_streak, habit.longest_streak
            ))
            .size(12)
            .color(MUTED)
        ]
        .spacing(8);
        if !habit.numeric.is_empty() {
            let values = habit.numeric.iter().map(|p| p.value).collect::<Vec<_>>();
            let lower = habit
                .numeric
                .iter()
                .map(|p| match p.goal {
                    Some(NumericGoal::AtLeast(v)) | Some(NumericGoal::AtMost(v)) => Some(v),
                    Some(NumericGoal::Between { min, .. }) => Some(min),
                    None => None,
                })
                .collect::<Vec<_>>();
            let upper = habit
                .numeric
                .iter()
                .map(|p| match p.goal {
                    Some(NumericGoal::Between { max, .. }) => Some(max),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let maximum = values
                .iter()
                .chain(lower.iter())
                .chain(upper.iter())
                .flatten()
                .copied()
                .fold(1.0, f64::max)
                * 1.1;
            let maximum = maximum.min(f64::MAX);
            let labels = habit
                .numeric
                .iter()
                .map(|p| {
                    format!(
                        "{} · {} {} · goal {}",
                        p.date,
                        p.value
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "missing".into()),
                        habit.unit,
                        p.goal
                            .as_ref()
                            .map(goal_text)
                            .unwrap_or_else(|| "N/A".into())
                    )
                })
                .collect();
            card = card
                .push(
                    text(format!(
                        "{} · mint actual · blue target · gold upper bound",
                        habit.unit
                    ))
                    .size(12)
                    .color(MUTED),
                )
                .push(
                    canvas(Plot {
                        series: vec![
                            (ACCENT, values),
                            (Color::from_rgb(0.4, 0.65, 1.0), lower),
                            (Color::from_rgb(0.9, 0.7, 0.3), upper),
                        ],
                        labels,
                        maximum,
                    })
                    .width(Length::Fill)
                    .height(170),
                );
            if let Some(point) = habit.numeric.get(index) {
                card = card.push(
                    text(format!(
                        "{} · value {} {} · target {} · {}",
                        point.date,
                        point
                            .value
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "missing".into()),
                        habit.unit,
                        point
                            .goal
                            .as_ref()
                            .map(goal_text)
                            .unwrap_or_else(|| "N/A".into()),
                        if point.successful {
                            "goal met"
                        } else {
                            "not met / N/A"
                        }
                    ))
                    .size(12),
                );
            }
        }
        body = body.push(container(card).padding(14).style(panel).width(Length::Fill));
    }
    body.into()
}
