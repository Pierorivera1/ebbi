use crate::app::{App, FormKind, Message};
use ebbi::domain::{Action, StudyReview, StudyTopic};
use iced::widget::{
    button, checkbox, column, container, markdown, mouse_area, rich_text, row, span, text, tooltip,
    Space,
};
use iced::{Alignment, Border, Color, Element, Font, Length, Theme};

const INK: Color = Color::from_rgb(0.76, 0.80, 0.82);
const MUTED: Color = Color::from_rgb(0.40, 0.45, 0.49);
const EDGE: Color = Color::from_rgb(0.16, 0.19, 0.21);
const RED: Color = Color::from_rgb(0.96, 0.40, 0.39);

fn frame(selected: bool) -> container::Style {
    container::Style {
        background: selected.then_some(Color::from_rgb(0.20, 0.23, 0.25).into()),
        border: Border {
            color: if selected {
                Color::from_rgb(0.47, 0.51, 0.53)
            } else {
                EDGE
            },
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}
fn control(label: &str, message: Message, enabled: bool) -> iced::widget::Button<'_, Message> {
    button(text(label).font(Font::MONOSPACE).size(11))
        .padding([5, 8])
        .style(button::text)
        .on_press_maybe(enabled.then_some(message))
}
fn badge(label: String, overdue: bool) -> Element<'static, Message> {
    container(
        text(label)
            .font(Font::MONOSPACE)
            .size(10)
            .color(if overdue { RED } else { MUTED }),
    )
    .padding([3, 5])
    .style(move |_| container::Style {
        background: Some(
            if overdue {
                Color::from_rgb(0.24, 0.07, 0.07)
            } else {
                Color::from_rgb(0.12, 0.14, 0.15)
            }
            .into(),
        ),
        border: Border {
            color: if overdue {
                Color::from_rgb(0.46, 0.10, 0.10)
            } else {
                EDGE
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    })
    .into()
}
fn section(label: String) -> Element<'static, Message> {
    container(text(label).size(18).color(crate::ui::ACCENT))
        .padding([8, 0])
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let ready = app.data.is_some() && !app.busy;
    let heading = column![
        text("Study").font(Font::MONOSPACE).size(22).color(INK),
        text(app.today.format("%A, %B %-d").to_string())
            .font(Font::MONOSPACE)
            .size(11)
            .color(MUTED)
    ]
    .spacing(4);
    let add = button(text("+ Add Topic").font(Font::MONOSPACE).size(13))
        .padding([7, 18])
        .on_press_maybe(ready.then_some(Message::OpenForm(FormKind::Study(None))))
        .style(|_, status| button::Style {
            background: Some(
                if status == button::Status::Hovered {
                    Color::from_rgb(0.58, 0.62, 0.64)
                } else {
                    Color::from_rgb(0.46, 0.50, 0.52)
                }
                .into(),
            ),
            text_color: Color::from_rgb(0.04, 0.06, 0.07),
            border: Border {
                radius: 18.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
    let mut body = column![].spacing(6);
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
            .font(Font::MONOSPACE)
            .size(16)
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
                    .font(Font::MONOSPACE)
                    .size(9)
                    .color(Color::from_rgb(0.08, 0.10, 0.11)),
                )
                .padding([1, 4])
                .style(|_| container::Style {
                    background: Some(INK.into()),
                    border: Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            );
        }
        let day = column![
            text(date.format("%a").to_string().to_uppercase())
                .font(Font::MONOSPACE)
                .size(10)
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
                .style(move |_, status| button::Style {
                    background: Some(
                        if selected {
                            Color::from_rgb(0.14, 0.17, 0.19)
                        } else if status == button::Status::Hovered {
                            Color::from_rgb(0.10, 0.12, 0.14)
                        } else {
                            Color::TRANSPARENT
                        }
                        .into(),
                    ),
                    border: Border {
                        color: if selected {
                            Color::from_rgb(0.27, 0.31, 0.34)
                        } else {
                            Color::TRANSPARENT
                        },
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }),
        );
    }
    body = body.push(
        container(days)
            .padding(6)
            .style(|_| frame(false))
            .width(Length::Fill),
    );
    if let Some(date) = app.study_date {
        body = body.push(
            row![
                text(format!(
                    "Viewing {}{}",
                    date.format("%A, %b %-d"),
                    if date == app.today {
                        " and overdue"
                    } else {
                        ""
                    }
                ))
                .font(Font::MONOSPACE)
                .size(11)
                .color(MUTED),
                control("Show all", Message::StudyDay(None), true)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }
    let reviews = app.study_reviews();
    if reviews.is_empty() {
        body = body.push(
            text("No reviews here. Press N to add a topic, or choose another day.")
                .font(Font::MONOSPACE)
                .size(13)
                .color(MUTED),
        );
    }
    let topics: std::collections::BTreeMap<_, _> = app
        .data
        .as_ref()
        .map(|d| d.study_topics.iter().map(|t| (t.id, t)).collect())
        .unwrap_or_default();
    let mut previous = String::new();
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
            body = body.push(section(label.clone()));
            previous = label;
        }
        if let Some(topic) = topics.get(&review.topic_id) {
            body = body.push(review_row(app, review, topic, ready));
        }
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
    let hovered = app.study_hovered == Some(review.id);
    let check = checkbox("", done)
        .size(19)
        .spacing(0)
        .on_toggle_maybe(ready.then_some(move |completed| {
            Message::Act(Action::SetReviewCompleted {
                id: review.id,
                completed,
            })
        }))
        .style(move |theme, status| {
            let mut style = checkbox::primary(theme, status);
            style.icon_color = Color::from_rgb(0.06, 0.08, 0.09);
            style.background = if done { INK } else { Color::TRANSPARENT }.into();
            style.border.color = if done {
                INK
            } else {
                Color::from_rgb(0.33, 0.37, 0.40)
            };
            style
        });
    let title =
        rich_text([span(&topic.title)
            .strikethrough(done)
            .color(if done { MUTED } else { INK })])
        .font(Font::MONOSPACE)
        .size(16)
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
            .font(Font::MONOSPACE)
            .size(11)
            .color(MUTED),
    );
    let mut content = column![].spacing(8);
    if app.width >= 900.0 {
        content = content.push(
            row![check, title, meta]
                .spacing(12)
                .align_y(Alignment::Center),
        );
    } else {
        content = content
            .push(row![check, title].spacing(12).align_y(Alignment::Center))
            .push(meta);
    }
    if hovered || app.study_notes == Some(review.id) {
        let mut tools = row![].spacing(4);
        if !done {
            tools = tools
                .push(control("+1d", Message::SnoozeReview(review.id), ready))
                .push(control(
                    "Reschedule",
                    Message::OpenForm(FormKind::ReviewDate(review.id)),
                    ready,
                ));
        }
        if !topic.notes.trim().is_empty() {
            tools = tools.push(control(
                if app.study_notes == Some(review.id) {
                    "Hide notes"
                } else {
                    "Notes"
                },
                Message::ToggleReviewNotes(review.id),
                ready,
            ));
        }
        tools = tools
            .push(control(
                "Edit [I]",
                Message::OpenForm(FormKind::Study(Some(topic.id))),
                ready,
            ))
            .push(control(
                "Study date",
                Message::OpenForm(FormKind::StudyDate(topic.id)),
                ready,
            ))
            .push(control(
                "Delete [D]",
                Message::OpenForm(FormKind::DeleteStudy(topic.id)),
                ready,
            ));
        content = content.push(tools);
    }
    if app.study_notes == Some(review.id) {
        content = content.push(
            markdown::view(
                &app.markdown,
                markdown::Settings::default(),
                markdown::Style::from_palette(Theme::Dark.palette()),
            )
            .map(|url| Message::Link(url.to_string())),
        );
    }
    let detail = format!(
        "Study date: {} · Original deadline: {} · Completed: {}",
        topic
            .initial_date
            .map(|d| d.to_string())
            .unwrap_or_else(|| "unknown".into()),
        review
            .original_due_date
            .map(|d| d.to_string())
            .unwrap_or_else(|| "unknown (legacy)".into()),
        review
            .completed_date
            .map(|d| d.to_string())
            .unwrap_or_else(|| if done {
                "local date unknown".into()
            } else {
                "pending".into()
            })
    );
    let card = container(content)
        .padding([12, 10])
        .width(Length::Fill)
        .style(move |_| frame(selected));
    tooltip(
        mouse_area(card)
            .on_press(Message::SelectReview(review.id))
            .on_enter(Message::HoverReview(Some(review.id)))
            .on_exit(Message::HoverReview(None))
            .on_right_press(Message::OpenForm(FormKind::Study(Some(topic.id)))),
        container(text(detail).font(Font::MONOSPACE).size(11))
            .padding(8)
            .style(|_| frame(true)),
        tooltip::Position::Bottom,
    )
    .into()
}
