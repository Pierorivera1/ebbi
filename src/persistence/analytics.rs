use super::*;
use crate::analytics::{report, HabitHistory};
use std::collections::BTreeMap;

impl Database {
    pub fn analytics(
        &self,
        end: NaiveDate,
        days: u32,
        today: NaiveDate,
    ) -> Result<AnalyticsReport> {
        if !(1..=366).contains(&days) {
            return Err("Analytics range must be 1–366 days".into());
        }
        let mut histories=self.conn.prepare("SELECT id,name,kind,unit,tracked_from,unknown_before FROM habits ORDER BY position,id").map_err(err)?.query_map([], |r| Ok(HabitHistory {
            id:r.get(0)?,name:r.get(1)?,kind:if r.get::<_,String>(2)?=="numeric" { HabitKind::Numeric } else { HabitKind::Binary },unit:r.get(3)?,tracked_from:r.get(4)?,unknown_before:r.get(5)?,rules:BTreeMap::new(),completions:BTreeMap::new(),values:BTreeMap::new(),goals:BTreeMap::new(),
        })).map_err(err)?.collect::<rusqlite::Result<Vec<_>>>().map_err(err)?;
        // Fetch each history table once; chart rendering never queries SQLite.
        let indexes: BTreeMap<_, _> = histories
            .iter()
            .enumerate()
            .map(|(i, h)| (h.id, i))
            .collect();
        for (table, column) in [("habit_rules", "active"), ("habit_entries", "completed")] {
            let date_column = if table == "habit_rules" {
                "effective_from"
            } else {
                "date"
            };
            let mut stmt = self
                .conn
                .prepare(&format!(
                    "SELECT habit_id,{date_column},{column} FROM {table} WHERE {date_column}<=?1"
                ))
                .map_err(err)?;
            for row in stmt
                .query_map([end], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, NaiveDate>(1)?,
                        r.get::<_, bool>(2)?,
                    ))
                })
                .map_err(err)?
            {
                let (id, date, value) = row.map_err(err)?;
                let index = *indexes.get(&id).ok_or("Orphan habit history")?;
                if table == "habit_rules" {
                    histories[index].rules.insert(date, value);
                } else {
                    histories[index].completions.insert(date, value);
                }
            }
        }
        let mut stmt = self
            .conn
            .prepare("SELECT habit_id,date,value FROM habit_values WHERE date<=?1")
            .map_err(err)?;
        for row in stmt
            .query_map([end], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, NaiveDate>(1)?,
                    r.get::<_, f64>(2)?,
                ))
            })
            .map_err(err)?
        {
            let (id, date, value) = row.map_err(err)?;
            histories[*indexes.get(&id).ok_or("Orphan habit value")?]
                .values
                .insert(date, value);
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT habit_id,effective_from,goal FROM habit_goals WHERE effective_from<=?1",
            )
            .map_err(err)?;
        for row in stmt
            .query_map([end], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, NaiveDate>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?
        {
            let (id, date, goal) = row.map_err(err)?;
            histories[*indexes.get(&id).ok_or("Orphan habit goal")?]
                .goals
                .insert(date, serde_json::from_str(&goal).map_err(err)?);
        }
        let (_, reviews) = study_numeric::study_snapshot(&self.conn)?;
        report(histories, reviews, end, days, today)
    }
}
