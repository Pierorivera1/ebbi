use crate::domain::NumericGoal;
use chrono::{Days, NaiveDate};

pub fn fixed_review_dates(initial_date: NaiveDate) -> Result<[NaiveDate; 3], String> {
    let add = |days| {
        initial_date
            .checked_add_days(Days::new(days))
            .ok_or_else(|| "Study date is out of range".to_string())
    };
    Ok([add(1)?, add(3)?, add(5)?])
}

pub fn numeric_success(goal: &NumericGoal, value: Option<f64>) -> bool {
    let Some(value) = value.filter(|v| v.is_finite() && *v >= 0.0) else {
        return false;
    };
    match *goal {
        NumericGoal::AtLeast(min) => value >= min,
        NumericGoal::AtMost(max) => value <= max,
        NumericGoal::Between { min, max } => value >= min && value <= max,
    }
}

use crate::domain::*;
use chrono::{Datelike, Duration};
use std::collections::BTreeMap;

pub(crate) struct HabitHistory {
    pub id: i64,
    pub name: String,
    pub kind: HabitKind,
    pub unit: String,
    pub tracked_from: NaiveDate,
    pub unknown_before: Option<NaiveDate>,
    pub rules: BTreeMap<NaiveDate, bool>,
    pub completions: BTreeMap<NaiveDate, bool>,
    pub values: BTreeMap<NaiveDate, f64>,
    pub goals: BTreeMap<NaiveDate, NumericGoal>,
}
impl HabitHistory {
    fn state(&self, date: NaiveDate, today: NaiveDate) -> DayState {
        if date > today {
            return DayState::Future;
        }
        if self.unknown_before.is_some_and(|before| date < before) {
            return DayState::Unknown;
        }
        if date < self.tracked_from
            || !self
                .rules
                .range(..=date)
                .next_back()
                .is_some_and(|(_, active)| *active)
        {
            return DayState::Untracked;
        }
        DayState::Tracked
    }
    fn goal(&self, date: NaiveDate) -> Option<NumericGoal> {
        self.goals.range(..=date).next_back().map(|(_, goal)| *goal)
    }
    fn successful(&self, date: NaiveDate) -> bool {
        match self.kind {
            HabitKind::Binary => self.completions.get(&date).copied().unwrap_or(false),
            HabitKind::Numeric => self
                .goal(date)
                .is_some_and(|goal| numeric_success(&goal, self.values.get(&date).copied())),
        }
    }
    fn streaks(&self, end: NaiveDate, today: NaiveDate) -> (usize, usize) {
        let dates: Box<dyn Iterator<Item = &NaiveDate> + '_> = match self.kind {
            HabitKind::Binary => Box::new(self.completions.keys()),
            HabitKind::Numeric => Box::new(self.values.keys()),
        };
        let mut run = 0;
        let mut longest = 0;
        let mut last: Option<NaiveDate> = None;
        for &date in dates.take_while(|date| **date <= end) {
            if self.state(date, today) != DayState::Tracked || !self.successful(date) {
                continue;
            }
            run = if last.and_then(|previous| previous.succ_opt()) == Some(date) {
                run + 1
            } else {
                1
            };
            longest = longest.max(run);
            last = Some(date);
        }
        let current = if self.state(end, today) == DayState::Tracked
            && (last == Some(end) || (end == today && last == end.pred_opt()))
        {
            run
        } else {
            0
        };
        (current, longest)
    }
}
fn counts_at(histories: &[HabitHistory], date: NaiveDate, today: NaiveDate) -> Counts {
    let mut counts = Counts::default();
    for habit in histories {
        if habit.state(date, today) == DayState::Tracked {
            counts.eligible += 1;
            counts.successful += usize::from(habit.successful(date));
        }
    }
    counts
}
fn period(
    histories: &[HabitHistory],
    start: NaiveDate,
    end: NaiveDate,
    today: NaiveDate,
) -> PeriodSummary {
    let mut counts = Counts::default();
    for date in start.iter_days().take_while(|date| *date <= end) {
        let day = counts_at(histories, date, today);
        counts.successful += day.successful;
        counts.eligible += day.eligible;
    }
    PeriodSummary {
        start,
        end,
        counts,
        provisional: end >= today,
    }
}
fn comparison(current: PeriodSummary, previous: PeriodSummary) -> PeriodComparison {
    let percentage_point_change = current
        .counts
        .percentage()
        .zip(previous.counts.percentage())
        .map(|(a, b)| a - b);
    PeriodComparison {
        current,
        previous,
        percentage_point_change,
    }
}
fn before(date: NaiveDate, days: i64) -> Result<NaiveDate, String> {
    date.checked_sub_signed(Duration::days(days))
        .ok_or_else(|| "Analytics date is out of range".into())
}
pub(crate) fn report(
    histories: Vec<HabitHistory>,
    reviews: Vec<StudyReview>,
    end: NaiveDate,
    days: u32,
    today: NaiveDate,
) -> Result<AnalyticsReport, String> {
    if !(1..=366).contains(&days) {
        return Err("Analytics range must be 1–366 days".into());
    }
    let start = before(end, i64::from(days) - 1)?;
    let anchor = end.min(today);
    let week_start = before(anchor, i64::from(anchor.weekday().num_days_from_monday()))?;
    let week_end = before(week_start, 1)?;
    let previous_week_end = before(week_start, 8)?;
    let month_start = anchor.with_day(1).ok_or("Analytics date is out of range")?;
    let month_end = before(month_start, 1)?;
    let last_month_start = month_end
        .with_day(1)
        .ok_or("Analytics date is out of range")?;
    let previous_month_end = before(last_month_start, 1)?;
    let previous_month_start = previous_month_end
        .with_day(1)
        .ok_or("Analytics date is out of range")?;
    let weekly = comparison(
        period(&histories, before(week_start, 7)?, week_end, today),
        period(
            &histories,
            before(week_start, 14)?,
            previous_week_end,
            today,
        ),
    );
    let monthly = comparison(
        period(&histories, last_month_start, month_end, today),
        period(&histories, previous_month_start, previous_month_end, today),
    );
    let mut total = Counts::default();
    let daily: Vec<_> = start
        .iter_days()
        .take(days as usize)
        .map(|date| {
            let counts = counts_at(&histories, date, today);
            total.successful += counts.successful;
            total.eligible += counts.eligible;
            let unknown_habits = histories
                .iter()
                .filter(|habit| habit.state(date, today) == DayState::Unknown)
                .count();
            let state = if date > today {
                DayState::Future
            } else if counts.eligible > 0 {
                DayState::Tracked
            } else if unknown_habits > 0 {
                DayState::Unknown
            } else {
                DayState::Untracked
            };
            DailyProgress {
                date,
                counts,
                unknown_habits,
                state,
            }
        })
        .collect();
    let habits = histories
        .iter()
        .map(|habit| {
            let mut consistency = Counts::default();
            let mut numeric = Vec::new();
            for date in start.iter_days().take(days as usize) {
                let state = habit.state(date, today);
                let eligible = state == DayState::Tracked;
                let successful = eligible && habit.successful(date);
                consistency.eligible += usize::from(eligible);
                consistency.successful += usize::from(successful);
                if habit.kind == HabitKind::Numeric {
                    numeric.push(NumericPoint {
                        date,
                        value: habit.values.get(&date).copied(),
                        goal: habit.goal(date),
                        successful,
                        eligible,
                        state,
                    });
                }
            }
            let (current_streak, longest_streak) = habit.streaks(anchor, today);
            HabitAnalytics {
                id: habit.id,
                name: habit.name.clone(),
                kind: habit.kind,
                unit: habit.unit.clone(),
                consistency,
                current_streak,
                longest_streak,
                numeric,
            }
        })
        .collect();
    let mut study = StudyAnalytics {
        due: 0,
        overdue: 0,
        upcoming: 0,
        completed: 0,
        on_time: Counts::default(),
        unknown_deadlines: 0,
        unknown_completion_dates: 0,
        activity: start
            .iter_days()
            .take(days as usize)
            .map(|date| StudyDay { date, completed: 0 })
            .collect(),
    };
    for review in reviews {
        if review.completed_at.is_none() {
            match review.scheduled_date.cmp(&today) {
                std::cmp::Ordering::Equal => study.due += 1,
                std::cmp::Ordering::Less => study.overdue += 1,
                std::cmp::Ordering::Greater => study.upcoming += 1,
            }
        } else if let Some(date) = review.completed_date {
            if date >= start && date <= end && date <= today {
                study.completed += 1;
                study.activity[(date - start).num_days() as usize].completed += 1;
            }
        } else {
            study.unknown_completion_dates += 1;
        }
        if let Some(deadline) = review.original_due_date {
            if deadline >= start && deadline <= end && deadline < today {
                study.on_time.eligible += 1;
                study.on_time.successful += usize::from(
                    review.completed_at.is_some()
                        && review.completed_date.is_some_and(|date| date <= deadline),
                );
            }
        } else {
            study.unknown_deadlines += 1;
        }
    }
    Ok(AnalyticsReport {
        start,
        end,
        today,
        daily,
        habits,
        total,
        weekly,
        monthly,
        study,
    })
}
