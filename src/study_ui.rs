use crate::app::{App, FormKind, Message};
use crate::ui::{
    action, panel, ACCENT, CONTROL_SIZE, DATE_SIZE, ERROR, HEADING_SIZE, INK, META_SIZE, MUTED,
    ROW_SIZE,
};
use ebbi::domain::{Action, StudyReview, StudyTopic};
use iced::widget::{
    button, checkbox, column, container, mouse_area, rich_text, row, span, text, Space,
};
use iced::{Alignment, Border, Color, Element, Length, Theme};

fn frame(theme: &Theme, selected: bool) -> container::Style {
    let mut style = panel(theme);
    if selected {
        style.border.color = ACCENT;
    }
    style
}
fn badge(label: String, overdue: bool) -> Element<'static, Message> {
    container(
        text(label)
            .size(META_SIZE)
            .color(if overdue { ERROR } else { MUTED }),
    )
    .padding([3, 5])
    .style(move |theme| {
        let mut style = panel(theme);
        style.border.radius = 4.0.into();
        if overdue {
            style.border.color = ERROR;
        }
        style
    })
    .into()
}
fn section<'a>(content: iced::widget::Column<'a, Message>) -> Element<'a, Message> {
    container(content)
        .padding(16)
        .style(panel)
        .width(Length::Fill)
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let ready = app.data.is_some() && !app.busy;
    let heading = row![
        text(app.today.format("%A").to_string())
            .size(DATE_SIZE)
            .color(ACCENT),
        text(app.today.format("%Y-%m-%d").to_string())
            .size(CONTROL_SIZE)
            .color(MUTED)
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let add = action(
        "+ Add Topic",
        Message::OpenForm(FormKind::Study(None)),
        ready,
    );
    let mut body = column![].spacing(12);
    body = body.push(
        row![heading, Space::with_width(Length::Fill), add]
            .spacing(16)
            .align_y(Alignment::Center),
    );
    let mut start = app.today;
    if let Some(date) = app.study_date {
        if date < app.today {
            start = date;
        } else if date.signed_duration_since(app.today).num_days() >= 7 {
            start = date
                .checked_sub_signed(chrono::Duration::days(6))
                .unwrap_or(date);
        }
    }
    let mut days = row![].spacing(6);
    for offset in 0..7 {
        let Some(date) = start.checked_add_signed(chrono::Duration::days(offset)) else {
            break;
        };
        let count = app
            .data
            .as_ref()
            .map(|d| {
                d.study_reviews
                    .iter()
                    .filter(|r| {
                        r.completed_at.is_none()
                            && (r.scheduled_date == date
                                || (date == app.today && r.scheduled_date < app.today))
                    })
                    .count()
            })
            .unwrap_or(0);
        let selected = app.study_date.unwrap_or(app.today) == date;
        let mut number = row![text(date.format("%-d").to_string())
            .size(ROW_SIZE)
            .color(INK)]
        .spacing(5)
        .align_y(Alignment::Center);
        if count > 0 {
            number = number.push(
                container(
                    text(if count > 9 {
                        "9+".into()
                    } else {
                        count.to_string()
                    })
                    .size(META_SIZE)
                    .color(crate::ui::theme().palette().background),
                )
                .padding([1, 4])
                .style(|_| container::Style {
                    background: Some(ACCENT.into()),
                    border: Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            );
        }
        let day = column![
            text(date.format("%a").to_string())
                .size(CONTROL_SIZE)
                .color(MUTED),
            number
        ]
        .spacing(4)
        .align_x(Alignment::Center);
        days = days.push(
            button(container(day).center_x(Length::Fill).padding([10, 2]))
                .width(Length::Fill)
                .padding(0)
                .on_press(Message::StudyDay(if app.study_date == Some(date) {
                    None
                } else {
                    Some(date)
                }))
                .style(move |theme, status| button::Style {
                    background: if selected || status == button::Status::Hovered {
                        button::secondary(theme, status).background
                    } else {
                        None
                    },
                    text_color: INK,
                    border: Border {
                        color: if selected { ACCENT } else { Color::TRANSPARENT },
                        width: 1.0,
                        radius: 12.0.into(),
                    },
                    ..Default::default()
                }),
        );
    }
    body = body.push(container(days).padding(6).style(panel).width(Length::Fill));
    let reviews = app.study_reviews();
    if reviews.is_empty() {
        body = body.push(
            text("No reviews here. Press N to add a topic, or choose another day.")
                .size(CONTROL_SIZE)
                .color(MUTED),
        );
    }
    let topics: std::collections::BTreeMap<_, _> = app
        .data
        .as_ref()
        .map(|d| d.study_topics.iter().map(|t| (t.id, t)).collect())
        .unwrap_or_default();
    let mut previous = String::new();
    let mut group = column![].spacing(12);
    for review in reviews {
        let label = if review.completed_at.is_some() {
            "Completed".into()
        } else if review.scheduled_date == app.today
            || (review.scheduled_date < app.today
                && app.study_date.is_none_or(|date| date == app.today))
        {
            "Today".into()
        } else if app.today.succ_opt() == Some(review.scheduled_date) {
            "Tomorrow".into()
        } else {
            review.scheduled_date.format("%A, %b %-d").to_string()
        };
        if label != previous {
            if !previous.is_empty() {
                body = body.push(section(group));
            }
            group = column![text(label.clone()).size(HEADING_SIZE)].spacing(12);
            previous = label;
        }
        if let Some(topic) = topics.get(&review.topic_id) {
            group = group.push(review_row(app, review, topic, ready));
        }
    }
    if !previous.is_empty() {
        body = body.push(section(group));
    }
    body.into()
}

fn review_row<'a>(
    app: &'a App,
    review: &'a StudyReview,
    topic: &'a StudyTopic,
    ready: bool,
) -> Element<'a, Message> {
    let done = review.completed_at.is_some();
    let selected = app.study_selected == Some(review.id);
    let check = checkbox("", done).on_toggle_maybe(ready.then_some(move |completed| {
        Message::Act(Action::SetReviewCompleted {
            id: review.id,
            completed,
        })
    }));
    let title =
        rich_text([span(&topic.title)
            .strikethrough(done)
            .color(if done { MUTED } else { INK })])
        .size(ROW_SIZE)
        .width(Length::Fill);
    let offset = match review.stage {
        1 => 1,
        2 => 3,
        _ => 5,
    };
    let mut meta = row![badge(format!("R{} (+{}d)", review.stage, offset), false)]
        .spacing(8)
        .align_y(Alignment::Center);
    if !done && review.scheduled_date < app.today {
        meta = meta.push(badge("OVERDUE".into(), true));
    }
    meta = meta.push(
        text(review.scheduled_date.format("%b %-d").to_string())
            .size(META_SIZE)
            .color(MUTED),
    );
    let mut content = column![].spacing(8);
    if app.width >= 900.0 {
        content = content.push(
            row![check, title, meta]
                .spacing(8)
                .align_y(Alignment::Center),
        );
    } else {
        content = content
            .push(row![check, title].spacing(8).align_y(Alignment::Center))
            .push(meta);
    }
    let card = container(content)
        .padding(12)
        .width(Length::Fill)
        .style(move |theme| frame(theme, selected));
    mouse_area(card)
        .on_press(Message::SelectReview(review.id))
        .on_right_press(Message::OpenForm(FormKind::Study(Some(topic.id))))
        .into()
}
