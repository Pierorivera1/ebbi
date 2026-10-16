use super::*;

pub(super) fn goal_at(conn: &Connection, id: i64, date: NaiveDate) -> Result<Option<NumericGoal>> {
    let json: Option<String> = conn.query_row("SELECT goal FROM habit_goals WHERE habit_id=?1 AND effective_from<=?2 ORDER BY effective_from DESC LIMIT 1", params![id,date], |r|r.get(0)).optional().map_err(err)?;
    json.map(|json| serde_json::from_str(&json).map_err(err))
        .transpose()
}
fn valid_goal(goal: &NumericGoal) -> Result<()> {
    let valid = |v: f64| v.is_finite() && v >= 0.0;
    if match *goal {
        NumericGoal::AtLeast(v) | NumericGoal::AtMost(v) => valid(v),
        NumericGoal::Between { min, max } => valid(min) && valid(max) && min <= max,
    } {
        Ok(())
    } else {
        Err("Goals must be finite, nonnegative, and ordered".into())
    }
}
fn numeric(conn: &Connection, id: i64) -> Result<()> {
    let kind: String = conn
        .query_row("SELECT kind FROM habits WHERE id=?1", [id], |r| r.get(0))
        .map_err(err)?;
    if kind == "numeric" {
        Ok(())
    } else {
        Err("This habit does not accept numeric values or goals".into())
    }
}
pub(super) fn study_snapshot(conn: &Connection) -> Result<(Vec<StudyTopic>, Vec<StudyReview>)> {
    let topics = conn.prepare("SELECT id,title,notes,initial_date,(SELECT count(*)=3 AND count(completed_at)=3 FROM study_reviews WHERE topic_id=study_topics.id) FROM study_topics ORDER BY id").map_err(err)?.query_map([], |r| Ok(StudyTopic { id:r.get(0)?,title:r.get(1)?,notes:r.get(2)?,initial_date:r.get(3)?,completed:r.get(4)? })).map_err(err)?.collect::<rusqlite::Result<Vec<_>>>().map_err(err)?;
    let reviews = conn.prepare("SELECT id,topic_id,review_stage,original_due_date,scheduled_date,completed_at,completed_date FROM study_reviews ORDER BY scheduled_date,topic_id,review_stage").map_err(err)?.query_map([], |r| Ok(StudyReview { id:r.get(0)?,topic_id:r.get(1)?,stage:r.get(2)?,original_due_date:r.get(3)?,scheduled_date:r.get(4)?,completed_at:r.get(5)?,completed_date:r.get(6)? })).map_err(err)?.collect::<rusqlite::Result<Vec<_>>>().map_err(err)?;
    Ok((topics, reviews))
}
fn reschedule(conn: &Connection, id: i64, date: NaiveDate, now: &str) -> Result<()> {
    let (previous, completed): (NaiveDate, Option<String>) = conn
        .query_row(
            "SELECT scheduled_date,completed_at FROM study_reviews WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(err)?;
    if completed.is_some() {
        return Err("Completed reviews cannot be rescheduled".into());
    }
    if previous != date {
        conn.execute("INSERT INTO review_reschedules(review_id,previous_date,new_date,changed_at) VALUES(?1,?2,?3,?4)",params![id,previous,date,now]).map_err(err)?;
        conn.execute(
            "UPDATE study_reviews SET scheduled_date=?1 WHERE id=?2",
            params![date, id],
        )
        .map_err(err)?;
    }
    Ok(())
}
pub(super) fn apply(tx: &Connection, action: Action, today: NaiveDate, now: &str) -> Result<()> {
    match action {
        Action::AddNumericHabit { name, unit, goal } => {
            nonblank(&name)?;
            nonblank(&unit)?;
            valid_goal(&goal)?;
            tx.execute("INSERT INTO habits(name,position,created_at,tracked_from,kind,unit) VALUES(?1,coalesce((SELECT max(position)+1 FROM habits),0),?2,?3,'numeric',?4)",params![name.trim(),now,today,unit.trim()]).map_err(err)?;
            let id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO habit_rules VALUES(?1,?2,1)",
                params![id, today],
            )
            .map_err(err)?;
            tx.execute(
                "INSERT INTO habit_goals VALUES(?1,?2,?3)",
                params![id, today, serde_json::to_string(&goal).map_err(err)?],
            )
            .map_err(err)?;
        }
        Action::SetNumericGoal {
            id,
            effective_from,
            goal,
        } => {
            numeric(tx, id)?;
            valid_goal(&goal)?;
            if effective_from <= today {
                return Err("Goal changes must take effect after today".into());
            }
            tx.execute("INSERT INTO habit_goals VALUES(?1,?2,?3) ON CONFLICT(habit_id,effective_from) DO UPDATE SET goal=excluded.goal",params![id,effective_from,serde_json::to_string(&goal).map_err(err)?]).map_err(err)?;
        }
        Action::SetHabitValue { id, date, value } => {
            numeric(tx, id)?;
            if date > today {
                return Err("Future habit entries are not allowed".into());
            }
            let eligible: bool = tx.query_row("SELECT ?2>=tracked_from AND (unknown_before IS NULL OR ?2>=unknown_before) AND coalesce((SELECT active FROM habit_rules WHERE habit_id=habits.id AND effective_from<=?2 ORDER BY effective_from DESC LIMIT 1),0) FROM habits WHERE id=?1",params![id,date],|r|r.get(0)).map_err(err)?;
            if !eligible {
                return Err("Habit was not known to be eligible on that date".into());
            }
            if let Some(value) = value {
                if !value.is_finite() || value < 0.0 {
                    return Err("Values must be finite and nonnegative".into());
                }
                tx.execute("INSERT INTO habit_values VALUES(?1,?2,?3) ON CONFLICT(habit_id,date) DO UPDATE SET value=excluded.value",params![id,date,value]).map_err(err)?;
            } else {
                tx.execute(
                    "DELETE FROM habit_values WHERE habit_id=?1 AND date=?2",
                    params![id, date],
                )
                .map_err(err)?;
            }
        }
        Action::AddStudyTopic {
            title,
            notes,
            initial_date,
        } => {
            nonblank(&title)?;
            let dates = fixed_review_dates(initial_date)?;
            tx.execute("INSERT INTO study_topics(title,notes,initial_date,created_at,status) VALUES(?1,?2,?3,?4,'active')",params![title.trim(),notes,initial_date,now]).map_err(err)?;
            let id = tx.last_insert_rowid();
            for (stage, date) in dates.into_iter().enumerate() {
                tx.execute("INSERT INTO study_reviews(topic_id,review_stage,original_due_date,scheduled_date) VALUES(?1,?2,?3,?3)",params![id,stage+1,date]).map_err(err)?;
            }
        }
        Action::UpdateStudyTopic { id, title, notes } => {
            nonblank(&title)?;
            changed(
                tx.execute(
                    "UPDATE study_topics SET title=?1,notes=?2 WHERE id=?3",
                    params![title.trim(), notes, id],
                )
                .map_err(err)?,
            )?;
        }
        Action::RescheduleStudyTopic { id, initial_date } => {
            let dates = fixed_review_dates(initial_date)?;
            changed(
                tx.execute(
                    "UPDATE study_topics SET initial_date=?1 WHERE id=?2",
                    params![initial_date, id],
                )
                .map_err(err)?,
            )?;
            let unfinished=tx.prepare("SELECT id,review_stage FROM study_reviews WHERE topic_id=?1 AND completed_at IS NULL").map_err(err)?.query_map([id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,usize>(1)?))).map_err(err)?.collect::<rusqlite::Result<Vec<_>>>().map_err(err)?;
            for (review, stage) in unfinished {
                reschedule(tx, review, dates[stage - 1], now)?;
            }
        }
        Action::DeleteStudyTopic(id) => {
            tx.execute("DELETE FROM review_reschedules WHERE review_id IN (SELECT id FROM study_reviews WHERE topic_id=?1)",[id]).map_err(err)?;
            tx.execute("DELETE FROM study_reviews WHERE topic_id=?1", [id])
                .map_err(err)?;
            changed(
                tx.execute("DELETE FROM study_topics WHERE id=?1", [id])
                    .map_err(err)?,
            )?;
        }
        Action::SetReviewCompleted { id, completed } => {
            changed(tx.execute("UPDATE study_reviews SET completed_date=CASE WHEN ?1 AND completed_at IS NULL THEN ?3 WHEN ?1 THEN completed_date ELSE NULL END,completed_at=CASE WHEN ?1 AND completed_at IS NULL THEN ?2 WHEN ?1 THEN completed_at ELSE NULL END WHERE id=?4",params![completed,now,today,id]).map_err(err)?)?;
            tx.execute("UPDATE study_topics SET status=CASE WHEN (SELECT count(*)=3 AND count(completed_at)=3 FROM study_reviews WHERE topic_id=study_topics.id) THEN 'completed' ELSE 'active' END WHERE id=(SELECT topic_id FROM study_reviews WHERE id=?1)",[id]).map_err(err)?;
        }
        Action::RescheduleReview { id, date } => reschedule(tx, id, date, now)?,
        _ => return Err("Unsupported study/numeric action".into()),
    }
    Ok(())
}
