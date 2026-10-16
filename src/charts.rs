use crate::app::Message;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};

/// Calendar-aligned series; None leaves a visible gap instead of inventing a zero.
pub struct Plot {
    pub series: Vec<(Color, Vec<Option<f64>>)>,
    pub labels: Vec<String>,
    pub maximum: f64,
}

impl canvas::Program<Message> for Plot {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        if matches!(
            event,
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            if let Some(position) = cursor.position_in(bounds) {
                if !self.labels.is_empty() {
                    let index = (((position.x - 44.0) / (bounds.width - 60.0).max(1.0))
                        .clamp(0.0, 1.0)
                        * self.labels.len().saturating_sub(1) as f32)
                        .round() as usize;
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::InspectDay(index)),
                    );
                }
            }
        }
        (canvas::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let width = (bounds.width - 60.0).max(1.0);
        let height = (bounds.height - 42.0).max(1.0);
        let muted = Color::from_rgb(0.55, 0.62, 0.65);
        let maximum = self.maximum.max(1.0);
        for fraction in [0.0, 0.5, 1.0] {
            let y = 12.0 + height * (1.0 - fraction);
            frame.stroke(
                &Path::line(Point::new(44.0, y), Point::new(44.0 + width, y)),
                Stroke::default().with_color(Color::from_rgb(0.18, 0.23, 0.25)),
            );
            frame.fill_text(Text {
                content: format!("{:.0}", maximum * fraction as f64),
                position: Point::new(0.0, y - 7.0),
                color: muted,
                size: 11.0.into(),
                ..Text::default()
            });
        }
        for (color, values) in &self.series {
            let mut previous = None;
            for (index, value) in values.iter().enumerate() {
                let point = value.map(|value| {
                    Point::new(
                        44.0 + width * index as f32 / values.len().saturating_sub(1).max(1) as f32,
                        12.0 + height * (1.0 - (value / maximum).clamp(0.0, 1.0) as f32),
                    )
                });
                if let (Some(from), Some(to)) = (previous, point) {
                    frame.stroke(
                        &Path::line(from, to),
                        Stroke::default().with_width(2.0).with_color(*color),
                    );
                }
                if let Some(point) = point {
                    frame.fill(&Path::circle(point, 2.5), *color);
                }
                previous = point;
            }
        }
        if let (Some(first), Some(last)) = (self.labels.first(), self.labels.last()) {
            for (label, x) in [(first, 44.0), (last, (bounds.width - 90.0).max(44.0))] {
                frame.fill_text(Text {
                    content: label.split(" · ").next().unwrap_or(label).to_string(),
                    position: Point::new(x, bounds.height - 17.0),
                    color: muted,
                    size: 11.0.into(),
                    ..Text::default()
                });
            }
        }
        if let Some(position) = cursor.position_in(bounds) {
            if !self.labels.is_empty() {
                let index = (((position.x - 44.0) / width).clamp(0.0, 1.0)
                    * self.labels.len().saturating_sub(1) as f32)
                    .round() as usize;
                let x =
                    44.0 + width * index as f32 / self.labels.len().saturating_sub(1).max(1) as f32;
                frame.stroke(
                    &Path::line(Point::new(x, 12.0), Point::new(x, 12.0 + height)),
                    Stroke::default().with_color(muted),
                );
                frame.fill_text(Text {
                    content: self.labels[index].clone(),
                    position: Point::new(
                        (x - 70.0).clamp(44.0, (bounds.width - 230.0).max(44.0)),
                        0.0,
                    ),
                    color: Color::WHITE,
                    size: 12.0.into(),
                    ..Text::default()
                });
            }
        }
        vec![frame.into_geometry()]
    }
}
