//! Deterministic calendar, fixed-interval, and one-time schedules.

use std::collections::BTreeSet;
use std::str::FromStr;

use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

use crate::{DurationMicros, Timestamp, ValidationError};

/// A normalized schedule with exactly one scheduling mode.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// Five-field calendar schedule.
    Cron {
        expression: String,
        timezone: ScheduleTimeZone,
    },
    /// Whole-second fixed interval from a durable anchor.
    Every {
        interval: DurationMicros,
        anchor: Timestamp,
    },
    /// One-time absolute instant.
    At { at: Timestamp },
}

/// Calendar timezone selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "name", rename_all = "snake_case")]
pub enum ScheduleTimeZone {
    Local,
    Iana(String),
}

impl Schedule {
    /// Validates and returns the next `count` instants strictly after `after`.
    pub fn next(&self, after: Timestamp, count: usize) -> Result<Vec<Timestamp>, ValidationError> {
        if count > 1000 {
            return Err(ValidationError::new(
                "count",
                "out_of_range",
                "count must not exceed 1000",
            ));
        }
        match self {
            Self::At { at } => Ok(if *at > after && count > 0 {
                vec![*at]
            } else {
                Vec::new()
            }),
            Self::Every { interval, anchor } => {
                interval_occurrences(*interval, *anchor, after, count)
            }
            Self::Cron {
                expression,
                timezone,
            } => CronExpression::parse(expression)?.next(timezone, after, count),
        }
    }

    /// Checks that the schedule is executable without enumerating occurrences.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Cron {
                expression,
                timezone,
            } => {
                CronExpression::parse(expression)?;
                timezone.resolve().map(|_| ())
            }
            Self::Every { interval, .. }
                if interval.get() < 1_000_000 || !interval.get().is_multiple_of(1_000_000) =>
            {
                Err(ValidationError::new(
                    "every",
                    "whole_seconds_required",
                    "interval must be a positive whole number of seconds",
                ))
            }
            Self::Every { .. } | Self::At { .. } => Ok(()),
        }
    }
}

impl ScheduleTimeZone {
    fn resolve(&self) -> Result<TimeZone, ValidationError> {
        match self {
            Self::Local => Ok(TimeZone::system()),
            Self::Iana(name) => TimeZone::get(name).map_err(|error| {
                ValidationError::new("timezone", "unknown_timezone", error.to_string())
            }),
        }
    }
}

fn interval_occurrences(
    interval: DurationMicros,
    anchor: Timestamp,
    after: Timestamp,
    count: usize,
) -> Result<Vec<Timestamp>, ValidationError> {
    if interval.get() < 1_000_000 || !interval.get().is_multiple_of(1_000_000) {
        return Err(ValidationError::new(
            "every",
            "whole_seconds_required",
            "interval must be a positive whole number of seconds",
        ));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let step = i64::try_from(interval.get())
        .map_err(|_| ValidationError::new("every", "duration_overflow", "interval is too large"))?;
    let delta = after.epoch_micros().saturating_sub(anchor.epoch_micros());
    let multiple = if delta < 0 {
        0
    } else {
        delta.checked_div(step).unwrap_or(0).saturating_add(1)
    };
    let mut current = anchor
        .epoch_micros()
        .checked_add(multiple.checked_mul(step).ok_or_else(|| {
            ValidationError::new("every", "time_overflow", "next occurrence overflows")
        })?)
        .ok_or_else(|| {
            ValidationError::new("every", "time_overflow", "next occurrence overflows")
        })?;
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        result.push(Timestamp::from_epoch_micros(current));
        current = current.checked_add(step).ok_or_else(|| {
            ValidationError::new("every", "time_overflow", "next occurrence overflows")
        })?;
    }
    Ok(result)
}

#[derive(Clone, Debug)]
struct CronExpression {
    minute: Field,
    hour: Field,
    day: Field,
    month: Field,
    weekday: Field,
}

#[derive(Clone, Debug)]
struct Field {
    min: u8,
    max: u8,
    allowed: BTreeSet<u8>,
    wildcard: bool,
}

impl CronExpression {
    fn parse(source: &str) -> Result<Self, ValidationError> {
        let expanded = match source.trim().to_ascii_lowercase().as_str() {
            "@yearly" | "@annually" => "0 0 1 1 *",
            "@monthly" => "0 0 1 * *",
            "@weekly" => "0 0 * * 0",
            "@daily" | "@midnight" => "0 0 * * *",
            "@hourly" => "0 * * * *",
            value if value.starts_with('@') => return Err(cron_error("unsupported cron alias")),
            _ => source,
        };
        let parts: Vec<&str> = expanded.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(cron_error("cron expression must have exactly five fields"));
        }
        Ok(Self {
            minute: Field::parse(parts[0], 0, 59, &[])?,
            hour: Field::parse(parts[1], 0, 23, &[])?,
            day: Field::parse(parts[2], 1, 31, &[])?,
            month: Field::parse(
                parts[3],
                1,
                12,
                &[
                    ("jan", 1),
                    ("feb", 2),
                    ("mar", 3),
                    ("apr", 4),
                    ("may", 5),
                    ("jun", 6),
                    ("jul", 7),
                    ("aug", 8),
                    ("sep", 9),
                    ("oct", 10),
                    ("nov", 11),
                    ("dec", 12),
                ],
            )?,
            weekday: Field::parse(
                parts[4],
                0,
                7,
                &[
                    ("sun", 0),
                    ("mon", 1),
                    ("tue", 2),
                    ("wed", 3),
                    ("thu", 4),
                    ("fri", 5),
                    ("sat", 6),
                ],
            )?
            .normalize_sunday(),
        })
    }

    fn next(
        &self,
        timezone: &ScheduleTimeZone,
        after: Timestamp,
        count: usize,
    ) -> Result<Vec<Timestamp>, ValidationError> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let zone = timezone.resolve()?;
        let minute = 60_000_000_i64;
        let start = after
            .epoch_micros()
            .div_euclid(minute)
            .saturating_add(1)
            .saturating_mul(minute);
        let mut result = Vec::with_capacity(count);
        let mut seen_civil = BTreeSet::new();
        // Five years is a bounded guard and covers every valid five-field cron expression.
        let max_steps = 5 * 366 * 24 * 60;
        for step in 0..max_steps {
            let micros = start
                .checked_add(i64::from(step).saturating_mul(minute))
                .ok_or_else(|| cron_error("occurrence time overflow"))?;
            let timestamp = jiff::Timestamp::from_microsecond(micros)
                .map_err(|error| cron_error(&error.to_string()))?;
            let zoned = timestamp.to_zoned(zone.clone());
            let weekday = zoned.weekday().to_sunday_zero_offset().cast_unsigned();
            let civil = (
                zoned.year(),
                zoned.month(),
                zoned.day(),
                zoned.hour(),
                zoned.minute(),
            );
            let day_matches = self.day.matches(zoned.day().cast_unsigned());
            let weekday_matches = self.weekday.matches(weekday);
            let calendar_day_matches = if self.day.wildcard && self.weekday.wildcard {
                true
            } else if self.day.wildcard {
                weekday_matches
            } else if self.weekday.wildcard {
                day_matches
            } else {
                day_matches || weekday_matches
            };
            if self.minute.matches(zoned.minute().cast_unsigned())
                && self.hour.matches(zoned.hour().cast_unsigned())
                && self.month.matches(zoned.month().cast_unsigned())
                && calendar_day_matches
                && seen_civil.insert(civil)
            {
                result.push(Timestamp::from_epoch_micros(micros));
                if result.len() == count {
                    return Ok(result);
                }
            }
        }
        Err(cron_error("could not find occurrence within five years"))
    }
}

impl Field {
    fn parse(
        source: &str,
        min: u8,
        max: u8,
        names: &[(&str, u8)],
    ) -> Result<Self, ValidationError> {
        let source = source.to_ascii_lowercase();
        let wildcard = source == "*" || source.starts_with("*/");
        let mut allowed = BTreeSet::new();
        for segment in source.split(',') {
            let (base, step) = match segment.split_once('/') {
                Some((base, step)) => {
                    let step = parse_value(step, names)?;
                    if step == 0 {
                        return Err(cron_error("cron step must be positive"));
                    }
                    (base, step)
                }
                None => (segment, 1),
            };
            let (start, end) = if base == "*" {
                (min, max)
            } else if let Some((start, end)) = base.split_once('-') {
                (parse_value(start, names)?, parse_value(end, names)?)
            } else {
                let value = parse_value(base, names)?;
                (value, value)
            };
            if start < min || end > max || start > end {
                return Err(cron_error("cron field value is out of range"));
            }
            let mut value = start;
            while value <= end {
                allowed.insert(value);
                match value.checked_add(step) {
                    Some(next) => value = next,
                    None => break,
                }
            }
        }
        if allowed.is_empty() {
            return Err(cron_error("cron field is empty"));
        }
        Ok(Self {
            min,
            max,
            allowed,
            wildcard,
        })
    }

    fn normalize_sunday(mut self) -> Self {
        if self.allowed.remove(&7) {
            self.allowed.insert(0);
        }
        self.max = 6;
        self
    }

    fn matches(&self, value: u8) -> bool {
        value >= self.min && value <= self.max && self.allowed.contains(&value)
    }
}

fn parse_value(source: &str, names: &[(&str, u8)]) -> Result<u8, ValidationError> {
    if let Some((_, value)) = names.iter().find(|(name, _)| *name == source) {
        return Ok(*value);
    }
    u8::from_str(source).map_err(|_| cron_error("invalid cron field value"))
}

fn cron_error(message: &str) -> ValidationError {
    ValidationError::new("cron", "invalid_cron", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc_cron(expression: &str) -> Schedule {
        Schedule::Cron {
            expression: expression.into(),
            timezone: ScheduleTimeZone::Iana("UTC".into()),
        }
    }

    #[test]
    fn every_uses_anchor_not_previous_completion() {
        let anchor = "2026-01-01T00:00:00Z".parse().unwrap();
        let after = "2026-01-01T00:00:25Z".parse().unwrap();
        let schedule = Schedule::Every {
            interval: "10s".parse().unwrap(),
            anchor,
        };
        assert_eq!(
            schedule.next(after, 2).unwrap(),
            [
                "2026-01-01T00:00:30Z".parse().unwrap(),
                "2026-01-01T00:00:40Z".parse().unwrap()
            ]
        );
    }

    #[test]
    fn cron_alias_and_named_fields_work() {
        let after = "2026-08-21T12:34:00Z".parse().unwrap();
        assert_eq!(
            utc_cron("@hourly").next(after, 1).unwrap()[0],
            "2026-08-21T13:00:00Z".parse().unwrap()
        );
        assert!(CronExpression::parse("0 0 * jan mon").is_ok());
    }

    #[test]
    fn restricted_dom_and_dow_use_or() {
        let after = "2026-08-01T00:00:00Z".parse().unwrap();
        let next = utc_cron("0 9 31 * mon").next(after, 1).unwrap()[0];
        assert_eq!(next, "2026-08-03T09:00:00Z".parse().unwrap());
    }

    #[test]
    fn spring_gap_is_absent_and_fall_fold_occurs_once() {
        let gap = Schedule::Cron {
            expression: "30 2 * * *".into(),
            timezone: ScheduleTimeZone::Iana("America/New_York".into()),
        };
        let gap_after = "2026-03-08T06:00:00Z".parse().unwrap();
        assert_eq!(
            gap.next(gap_after, 1).unwrap()[0],
            "2026-03-09T06:30:00Z".parse().unwrap()
        );
        let fold = Schedule::Cron {
            expression: "30 1 * * *".into(),
            timezone: ScheduleTimeZone::Iana("America/New_York".into()),
        };
        let fold_after = "2026-11-01T04:00:00Z".parse().unwrap();
        let occurrences = fold.next(fold_after, 2).unwrap();
        assert_eq!(occurrences[0], "2026-11-01T05:30:00Z".parse().unwrap());
        assert_eq!(occurrences[1], "2026-11-02T06:30:00Z".parse().unwrap());
    }

    #[test]
    fn rejects_six_fields_and_reboot() {
        assert!(CronExpression::parse("0 0 0 * * *").is_err());
        assert!(CronExpression::parse("@reboot").is_err());
    }
}
