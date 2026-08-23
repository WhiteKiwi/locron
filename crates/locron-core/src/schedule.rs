//! Deterministic calendar, fixed-interval, and one-time schedules.

use std::collections::BTreeSet;
use std::str::FromStr;

use jiff::civil::Date;
use jiff::tz::{AmbiguousOffset, TimeZone};
use serde::{Deserialize, Serialize};

use crate::policy::MissedRunPolicy;
use crate::{DurationMicros, Timestamp, ValidationError};

#[cfg(test)]
thread_local! {
    static CRON_COMPILE_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Why a contiguous range of elapsed occurrences was not materialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmittedRangeKind {
    /// The occurrence was already outside its start deadline.
    StartDeadline,
    /// The selected missed-run policy discarded the occurrence.
    MissedRunPolicy,
    /// The newest bounded `all` window displaced older eligible occurrences.
    CatchUpLimit,
}

/// One compact, exact explanation for a contiguous omitted occurrence range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OmittedRange {
    /// Reason the occurrences were omitted.
    pub kind: OmittedRangeKind,
    /// Number of contiguous occurrences omitted.
    pub count: u64,
    /// Nominal instant of the first omitted occurrence.
    pub first: Timestamp,
    /// Nominal instant of the last omitted occurrence.
    pub last: Timestamp,
}

/// One selected occurrence and whether it is catch-up work.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectedOccurrence {
    /// Nominal instant of the occurrence.
    pub nominal: Timestamp,
    /// Whether the occurrence is catch-up work for an elapsed range.
    pub catch_up: bool,
}

/// Pure result of reconciling one durable cursor interval.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScheduleReconciliation {
    /// Occurrences selected for materialization, in nominal order.
    pub selected: Vec<SelectedOccurrence>,
    /// Ranges of occurrences omitted and why.
    pub omitted: Vec<OmittedRange>,
}

/// Durable classification of elapsed cursor time for one reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElapsedKind {
    /// The daemon remained responsible for the elapsed range.
    Normal,
    /// Startup, disabled time, suspend, or downtime made the range missed.
    Missed,
}

/// A normalized schedule with exactly one scheduling mode.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// Five-field calendar schedule.
    Cron {
        /// Cron expression in the standard five-field syntax.
        expression: String,
        /// Calendar timezone the expression is evaluated in.
        timezone: ScheduleTimeZone,
    },
    /// Whole-second fixed interval from a durable anchor.
    Every {
        /// Interval between consecutive occurrences.
        interval: DurationMicros,
        /// Durable anchor instant occurrences align to.
        anchor: Timestamp,
    },
    /// One-time absolute instant.
    At {
        /// The single occurrence instant.
        at: Timestamp,
    },
}

/// Calendar timezone selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "name", rename_all = "snake_case")]
pub enum ScheduleTimeZone {
    /// The process-local system timezone.
    Local,
    /// A named IANA timezone.
    Iana(String),
}

/// A validated schedule whose calendar match cycle can be reused across passes.
#[derive(Clone, Debug)]
pub struct CompiledSchedule {
    inner: CompiledScheduleKind,
}

#[derive(Clone, Debug)]
enum CompiledScheduleKind {
    Cron {
        expression: Box<CronExpression>,
        timezone: ScheduleTimeZone,
    },
    Every {
        interval: DurationMicros,
        anchor: Timestamp,
    },
    At {
        at: Timestamp,
    },
}

impl Schedule {
    /// Validates and compiles work that is independent of the current wall clock and timezone.
    pub fn compile(&self) -> Result<CompiledSchedule, ValidationError> {
        let inner = match self {
            Self::Cron {
                expression,
                timezone,
            } => {
                timezone.resolve().map(|_| ())?;
                CompiledScheduleKind::Cron {
                    expression: Box::new(CronExpression::parse(expression)?),
                    timezone: timezone.clone(),
                }
            }
            Self::Every { interval, anchor } => {
                interval_occurrences(*interval, *anchor, *anchor, 0)?;
                CompiledScheduleKind::Every {
                    interval: *interval,
                    anchor: *anchor,
                }
            }
            Self::At { at } => CompiledScheduleKind::At { at: *at },
        };
        Ok(CompiledSchedule { inner })
    }

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
        self.compile().map(|_| ())
    }

    /// Reconciles `(after, now]` using one already-sampled local timezone.
    ///
    /// Calendar selection retains only a bounded newest window and range
    /// accounting never walks elapsed occurrences or UTC minutes.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn reconcile(
        &self,
        after: Timestamp,
        now: Timestamp,
        missed_run: MissedRunPolicy,
        start_deadline: Option<DurationMicros>,
        catch_up_limit: u16,
        local_timezone: &TimeZone,
        elapsed_kind: ElapsedKind,
    ) -> Result<ScheduleReconciliation, ValidationError> {
        self.compile()?.reconcile(
            after,
            now,
            missed_run,
            start_deadline,
            catch_up_limit,
            local_timezone,
            elapsed_kind,
        )
    }
}

impl CompiledSchedule {
    /// Reconciles `(after, now]` without recompiling the calendar expression.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn reconcile(
        &self,
        after: Timestamp,
        now: Timestamp,
        missed_run: MissedRunPolicy,
        start_deadline: Option<DurationMicros>,
        catch_up_limit: u16,
        local_timezone: &TimeZone,
        elapsed_kind: ElapsedKind,
    ) -> Result<ScheduleReconciliation, ValidationError> {
        if !(1..=1000).contains(&catch_up_limit) {
            return Err(ValidationError::new(
                "catch_up_limit",
                "out_of_range",
                "must be from 1 through 1000",
            ));
        }
        if now <= after {
            return Ok(ScheduleReconciliation::default());
        }
        let deadline_after = start_deadline.map_or(after, |deadline| {
            let cutoff = now
                .epoch_micros()
                .saturating_sub(i64::try_from(deadline.get()).unwrap_or(i64::MAX));
            Timestamp::from_epoch_micros(after.epoch_micros().max(cutoff.saturating_sub(1)))
        });
        let expired = if deadline_after > after {
            self.range_stats(after, deadline_after, local_timezone)?
        } else {
            OccurrenceStats::default()
        };
        let eligible = self.range_stats(deadline_after, now, local_timezone)?;
        let keep = usize::from(catch_up_limit);
        let newest = self.newest_between(
            deadline_after,
            now,
            keep.saturating_add(usize::from(matches!(elapsed_kind, ElapsedKind::Normal))),
            local_timezone,
        )?;
        let mut selected = Vec::with_capacity(newest.len());
        match elapsed_kind {
            ElapsedKind::Missed => {
                let retained = match missed_run {
                    MissedRunPolicy::All => newest.as_slice(),
                    MissedRunPolicy::Skip => &[],
                    MissedRunPolicy::Latest => &newest[newest.len().saturating_sub(1)..],
                };
                selected.extend(retained.iter().copied().map(|nominal| SelectedOccurrence {
                    nominal,
                    catch_up: true,
                }));
            }
            ElapsedKind::Normal => {
                if let Some((&normal, missed_prefix)) = newest.split_last() {
                    let retained = match missed_run {
                        MissedRunPolicy::All => missed_prefix,
                        MissedRunPolicy::Skip => &[],
                        MissedRunPolicy::Latest => {
                            &missed_prefix[missed_prefix.len().saturating_sub(1)..]
                        }
                    };
                    selected.extend(retained.iter().copied().map(|nominal| SelectedOccurrence {
                        nominal,
                        catch_up: true,
                    }));
                    selected.push(SelectedOccurrence {
                        nominal: normal,
                        catch_up: false,
                    });
                }
            }
        }
        let mut omitted = Vec::with_capacity(2);
        if expired.count > 0 {
            omitted.push(expired.into_range(OmittedRangeKind::StartDeadline));
        }
        let missed_total = eligible.count.saturating_sub(u64::from(
            matches!(elapsed_kind, ElapsedKind::Normal) && eligible.count > 0,
        ));
        let selected_missed = selected.iter().filter(|item| item.catch_up).count() as u64;
        let omitted_count = missed_total.saturating_sub(selected_missed);
        if omitted_count > 0 {
            let first = eligible
                .first
                .ok_or_else(|| cron_error("non-empty range has no first occurrence"))?;
            let last = if let Some(selected_first) = selected.first().map(|item| item.nominal) {
                self.newest_between(
                    deadline_after,
                    Timestamp::from_epoch_micros(selected_first.epoch_micros().saturating_sub(1)),
                    1,
                    local_timezone,
                )?
                .into_iter()
                .next()
                .ok_or_else(|| cron_error("omitted prefix has no last occurrence"))?
            } else {
                eligible
                    .last
                    .ok_or_else(|| cron_error("non-empty range has no last occurrence"))?
            };
            let kind = if matches!(missed_run, MissedRunPolicy::All) {
                OmittedRangeKind::CatchUpLimit
            } else {
                OmittedRangeKind::MissedRunPolicy
            };
            omitted.push(OmittedRange {
                kind,
                count: omitted_count,
                first,
                last,
            });
        }
        Ok(ScheduleReconciliation { selected, omitted })
    }

    fn range_stats(
        &self,
        after: Timestamp,
        through: Timestamp,
        local_timezone: &TimeZone,
    ) -> Result<OccurrenceStats, ValidationError> {
        if through <= after {
            return Ok(OccurrenceStats::default());
        }
        match &self.inner {
            CompiledScheduleKind::At { at } => Ok(if *at > after && *at <= through {
                OccurrenceStats::one(*at)
            } else {
                OccurrenceStats::default()
            }),
            CompiledScheduleKind::Every { interval, anchor } => {
                interval_range_stats(*interval, *anchor, after, through)
            }
            CompiledScheduleKind::Cron {
                expression,
                timezone,
            } => {
                let zone = timezone.resolve_with(local_timezone)?;
                expression.range_stats(after, through, &zone)
            }
        }
    }

    fn newest_between(
        &self,
        after: Timestamp,
        through: Timestamp,
        limit: usize,
        local_timezone: &TimeZone,
    ) -> Result<Vec<Timestamp>, ValidationError> {
        if limit == 0 || through <= after {
            return Ok(Vec::new());
        }
        match &self.inner {
            CompiledScheduleKind::At { at } => Ok(if *at > after && *at <= through {
                vec![*at]
            } else {
                Vec::new()
            }),
            CompiledScheduleKind::Every { interval, anchor } => {
                interval_newest(*interval, *anchor, after, through, limit)
            }
            CompiledScheduleKind::Cron {
                expression,
                timezone,
            } => {
                let zone = timezone.resolve_with(local_timezone)?;
                expression.newest_between(after, through, limit, &zone)
            }
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

    fn resolve_with(&self, local: &TimeZone) -> Result<TimeZone, ValidationError> {
        match self {
            Self::Local => Ok(local.clone()),
            Self::Iana(name) => TimeZone::get(name).map_err(|error| {
                ValidationError::new("timezone", "unknown_timezone", error.to_string())
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct OccurrenceStats {
    count: u64,
    first: Option<Timestamp>,
    last: Option<Timestamp>,
}

impl OccurrenceStats {
    const fn one(at: Timestamp) -> Self {
        Self {
            count: 1,
            first: Some(at),
            last: Some(at),
        }
    }

    fn into_range(self, kind: OmittedRangeKind) -> OmittedRange {
        OmittedRange {
            kind,
            count: self.count,
            first: self.first.expect("non-empty range has a first occurrence"),
            last: self.last.expect("non-empty range has a last occurrence"),
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

fn interval_bounds(
    interval: DurationMicros,
    anchor: Timestamp,
    after: Timestamp,
    through: Timestamp,
) -> Result<Option<(i64, i64, i64)>, ValidationError> {
    let step = i64::try_from(interval.get())
        .map_err(|_| ValidationError::new("every", "duration_overflow", "interval is too large"))?;
    if step < 1_000_000 || step % 1_000_000 != 0 {
        return Err(ValidationError::new(
            "every",
            "whole_seconds_required",
            "interval must be a positive whole number of seconds",
        ));
    }
    let first = after
        .epoch_micros()
        .saturating_sub(anchor.epoch_micros())
        .div_euclid(step)
        .saturating_add(1)
        .max(0);
    let last = through
        .epoch_micros()
        .saturating_sub(anchor.epoch_micros())
        .div_euclid(step);
    Ok((last >= first).then_some((step, first, last)))
}

fn interval_at(anchor: Timestamp, step: i64, index: i64) -> Result<Timestamp, ValidationError> {
    anchor
        .epoch_micros()
        .checked_add(index.checked_mul(step).ok_or_else(|| {
            ValidationError::new("every", "time_overflow", "occurrence time overflows")
        })?)
        .map(Timestamp::from_epoch_micros)
        .ok_or_else(|| ValidationError::new("every", "time_overflow", "occurrence time overflows"))
}

fn interval_range_stats(
    interval: DurationMicros,
    anchor: Timestamp,
    after: Timestamp,
    through: Timestamp,
) -> Result<OccurrenceStats, ValidationError> {
    let Some((step, first, last)) = interval_bounds(interval, anchor, after, through)? else {
        return Ok(OccurrenceStats::default());
    };
    Ok(OccurrenceStats {
        count: u64::try_from(last.saturating_sub(first).saturating_add(1)).unwrap_or(u64::MAX),
        first: Some(interval_at(anchor, step, first)?),
        last: Some(interval_at(anchor, step, last)?),
    })
}

fn interval_newest(
    interval: DurationMicros,
    anchor: Timestamp,
    after: Timestamp,
    through: Timestamp,
    limit: usize,
) -> Result<Vec<Timestamp>, ValidationError> {
    let Some((step, first, last)) = interval_bounds(interval, anchor, after, through)? else {
        return Ok(Vec::new());
    };
    let retain = i64::try_from(limit).unwrap_or(i64::MAX);
    let bounded_first = first.max(last.saturating_sub(retain.saturating_sub(1)));
    (bounded_first..=last)
        .map(|index| interval_at(anchor, step, index))
        .collect()
}

#[derive(Clone, Debug)]
struct CronExpression {
    minute: Field,
    hour: Field,
    day: Field,
    month: Field,
    weekday: Field,
    matching_days: MatchingDayCycle,
}

#[derive(Clone, Debug)]
struct MatchingDayCycle {
    words: Box<[u64]>,
    prefix: Box<[u32]>,
    count: u32,
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
        #[cfg(test)]
        CRON_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
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
        let mut expression = Self {
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
            matching_days: MatchingDayCycle::empty(),
        };
        expression.matching_days = expression.build_matching_day_offsets();
        if expression.matching_days.count == 0 {
            return Err(cron_error(
                "calendar fields can never match a Gregorian date",
            ));
        }
        Ok(expression)
    }

    fn next(
        &self,
        timezone: &ScheduleTimeZone,
        after: Timestamp,
        count: usize,
    ) -> Result<Vec<Timestamp>, ValidationError> {
        let zone = timezone.resolve()?;
        self.next_in_zone(after, count, &zone)
    }

    fn next_in_zone(
        &self,
        after: Timestamp,
        count: usize,
        zone: &TimeZone,
    ) -> Result<Vec<Timestamp>, ValidationError> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let lower = jiff::Timestamp::from_microsecond(after.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?
            .to_zoned(zone.clone())
            .date();
        let mut date = self.matching_date_at_or_after(lower)?;
        let mut result = Vec::with_capacity(count);
        while let Some(candidate_date) = date {
            for candidate in self.candidates_on_date(candidate_date, zone)? {
                if candidate > after {
                    result.push(candidate);
                    if result.len() == count {
                        return Ok(result);
                    }
                }
            }
            date = candidate_date
                .tomorrow()
                .ok()
                .map(|next| self.matching_date_at_or_after(next))
                .transpose()?
                .flatten();
        }
        Ok(result)
    }

    fn date_matches(&self, date: Date) -> bool {
        let day_matches = self.day.matches(date.day().cast_unsigned());
        let weekday = date.weekday().to_sunday_zero_offset().cast_unsigned();
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
        self.month.matches(date.month().cast_unsigned()) && calendar_day_matches
    }

    fn resolve_civil(
        date: Date,
        hour: u8,
        minute: u8,
        zone: &TimeZone,
    ) -> Result<Option<Timestamp>, ValidationError> {
        let civil = date.at(
            i8::try_from(hour).expect("cron hour fits i8"),
            i8::try_from(minute).expect("cron minute fits i8"),
            0,
            0,
        );
        let ambiguous = zone.to_ambiguous_timestamp(civil);
        if matches!(ambiguous.offset(), AmbiguousOffset::Gap { .. }) {
            return Ok(None);
        }
        let timestamp = ambiguous
            .earlier()
            .map_err(|error| cron_error(&error.to_string()))?;
        Ok(Some(Timestamp::from_epoch_micros(
            timestamp.as_microsecond(),
        )))
    }

    fn candidates_on_date(
        &self,
        date: Date,
        zone: &TimeZone,
    ) -> Result<Vec<Timestamp>, ValidationError> {
        if !self.date_matches(date) {
            return Ok(Vec::new());
        }
        let mut candidates =
            Vec::with_capacity(self.hour.allowed.len() * self.minute.allowed.len());
        for &hour in &self.hour.allowed {
            for &minute in &self.minute.allowed {
                if let Some(timestamp) = Self::resolve_civil(date, hour, minute, zone)? {
                    candidates.push(timestamp);
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        Ok(candidates)
    }

    fn newest_between(
        &self,
        after: Timestamp,
        through: Timestamp,
        limit: usize,
        zone: &TimeZone,
    ) -> Result<Vec<Timestamp>, ValidationError> {
        let lower = jiff::Timestamp::from_microsecond(after.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?
            .to_zoned(zone.clone())
            .date();
        let upper = jiff::Timestamp::from_microsecond(through.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?
            .to_zoned(zone.clone())
            .date();
        let mut date = self.matching_date_at_or_before(upper)?;
        let mut newest = Vec::with_capacity(limit);
        while let Some(candidate_date) = date {
            if candidate_date < lower {
                break;
            }
            let candidates = self.candidates_on_date(candidate_date, zone)?;
            for candidate in candidates.into_iter().rev() {
                if candidate > after && candidate <= through {
                    newest.push(candidate);
                    if newest.len() == limit {
                        newest.reverse();
                        return Ok(newest);
                    }
                }
            }
            date = candidate_date
                .yesterday()
                .ok()
                .map(|previous| self.matching_date_at_or_before(previous))
                .transpose()?
                .flatten();
        }
        newest.reverse();
        Ok(newest)
    }

    fn range_stats(
        &self,
        after: Timestamp,
        through: Timestamp,
        zone: &TimeZone,
    ) -> Result<OccurrenceStats, ValidationError> {
        let lower_zoned = jiff::Timestamp::from_microsecond(after.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?
            .to_zoned(zone.clone());
        let upper_zoned = jiff::Timestamp::from_microsecond(through.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?
            .to_zoned(zone.clone());
        let lower_date = lower_zoned.date();
        let upper_date = upper_zoned.date();
        let time_count = u64::try_from(self.hour.allowed.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(self.minute.allowed.len()).unwrap_or(u64::MAX));
        let matching_days = self.matching_days_inclusive(lower_date, upper_date);
        let mut count = matching_days.saturating_mul(time_count);

        let mut exceptional_dates = BTreeSet::new();
        exceptional_dates.insert(lower_date);
        exceptional_dates.insert(upper_date);
        let lower_timestamp = jiff::Timestamp::from_microsecond(after.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?;
        let upper_timestamp = jiff::Timestamp::from_microsecond(through.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?;
        for transition in zone.following(lower_timestamp) {
            if transition.timestamp() > upper_timestamp {
                break;
            }
            let at = transition.timestamp();
            let before = at
                .checked_sub(jiff::Span::new().microseconds(1))
                .map_err(|error| cron_error(&error.to_string()))?
                .to_zoned(zone.clone());
            let after_transition = at.to_zoned(zone.clone());
            if after_transition.datetime() > before.datetime() {
                let mut date = before.date();
                loop {
                    exceptional_dates.insert(date);
                    if date >= after_transition.date() {
                        break;
                    }
                    date = date
                        .tomorrow()
                        .map_err(|error| cron_error(&error.to_string()))?;
                }
            }
        }
        for date in exceptional_dates {
            if date < lower_date || date > upper_date || !self.date_matches(date) {
                continue;
            }
            count = count.saturating_sub(time_count);
            let actual = self
                .candidates_on_date(date, zone)?
                .into_iter()
                .filter(|candidate| *candidate > after && *candidate <= through)
                .count();
            count = count.saturating_add(u64::try_from(actual).unwrap_or(u64::MAX));
        }
        if count == 0 {
            return Ok(OccurrenceStats::default());
        }
        let first = self
            .first_between(after, through, zone)?
            .ok_or_else(|| cron_error("counted range has no first occurrence"))?;
        let last = self
            .newest_between(after, through, 1, zone)?
            .into_iter()
            .next()
            .ok_or_else(|| cron_error("counted range has no last occurrence"))?;
        Ok(OccurrenceStats {
            count,
            first: Some(first),
            last: Some(last),
        })
    }

    fn first_between(
        &self,
        after: Timestamp,
        through: Timestamp,
        zone: &TimeZone,
    ) -> Result<Option<Timestamp>, ValidationError> {
        let lower = jiff::Timestamp::from_microsecond(after.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?
            .to_zoned(zone.clone())
            .date();
        let upper = jiff::Timestamp::from_microsecond(through.epoch_micros())
            .map_err(|error| cron_error(&error.to_string()))?
            .to_zoned(zone.clone())
            .date();
        let mut date = self.matching_date_at_or_after(lower)?;
        while let Some(candidate_date) = date {
            if candidate_date > upper {
                return Ok(None);
            }
            for candidate in self.candidates_on_date(candidate_date, zone)? {
                if candidate > after && candidate <= through {
                    return Ok(Some(candidate));
                }
            }
            date = candidate_date
                .tomorrow()
                .ok()
                .map(|next| self.matching_date_at_or_after(next))
                .transpose()?
                .flatten();
        }
        Ok(None)
    }

    fn matching_days_inclusive(&self, first: Date, last: Date) -> u64 {
        if last < first {
            return 0;
        }
        let before_first = matching_positions_before(date_day(first), &self.matching_days);
        let after_last =
            matching_positions_before(date_day(last).saturating_add(1), &self.matching_days);
        u64::try_from(after_last.saturating_sub(before_first)).unwrap_or(u64::MAX)
    }

    fn build_matching_day_offsets(&self) -> MatchingDayCycle {
        let mut words = vec![0_u64; GREGORIAN_CYCLE_WORDS];
        let mut date = Date::ZERO;
        let end = Date::new(400, 1, 1).expect("400-year cycle end is valid");
        let mut offset = 0_usize;
        while date < end {
            if self.date_matches(date) {
                words[offset / 64] |= 1_u64 << (offset % 64);
            }
            date = date.tomorrow().expect("400-year cycle remains in range");
            offset += 1;
        }
        MatchingDayCycle::from_words(words)
    }

    fn matching_date_at_or_before(&self, date: Date) -> Result<Option<Date>, ValidationError> {
        matching_date(date_day(date), &self.matching_days, false)
    }

    fn matching_date_at_or_after(&self, date: Date) -> Result<Option<Date>, ValidationError> {
        matching_date(date_day(date), &self.matching_days, true)
    }
}

const GREGORIAN_CYCLE_DAYS: i64 = 146_097;
const GREGORIAN_CYCLE_WORDS: usize = 2_283;

fn date_day(date: Date) -> i64 {
    date.duration_since(Date::ZERO).as_secs() / 86_400
}

fn date_from_day(day: i64) -> Option<Date> {
    Date::ZERO.checked_add(jiff::Span::new().days(day)).ok()
}

impl MatchingDayCycle {
    fn empty() -> Self {
        Self::from_words(vec![0; GREGORIAN_CYCLE_WORDS])
    }

    fn from_words(words: Vec<u64>) -> Self {
        let mut prefix = Vec::with_capacity(words.len() + 1);
        prefix.push(0_u32);
        for word in &words {
            prefix.push(
                prefix
                    .last()
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(word.count_ones()),
            );
        }
        let count = prefix.last().copied().unwrap_or(0);
        Self {
            words: words.into_boxed_slice(),
            prefix: prefix.into_boxed_slice(),
            count,
        }
    }

    fn rank_before(&self, offset: i64) -> u32 {
        if offset <= 0 {
            return 0;
        }
        if offset >= GREGORIAN_CYCLE_DAYS {
            return self.count;
        }
        let offset = usize::try_from(offset).expect("cycle offset is non-negative");
        let word = offset / 64;
        let bit = offset % 64;
        let mask = if bit == 0 { 0 } else { (1_u64 << bit) - 1 };
        self.prefix[word].saturating_add((self.words[word] & mask).count_ones())
    }

    fn select(&self, rank: u32) -> Option<i64> {
        if rank >= self.count {
            return None;
        }
        let word_index = self.prefix.partition_point(|prefix| *prefix <= rank) - 1;
        let mut word = self.words[word_index];
        let mut remaining = rank - self.prefix[word_index];
        while remaining > 0 {
            word &= word - 1;
            remaining -= 1;
        }
        let bit = word.trailing_zeros();
        Some(i64::try_from(word_index).ok()?.saturating_mul(64) + i64::from(bit))
    }
}

fn matching_positions_before(day: i64, cycle_days: &MatchingDayCycle) -> i128 {
    let cycle = day.div_euclid(GREGORIAN_CYCLE_DAYS);
    let within = day.rem_euclid(GREGORIAN_CYCLE_DAYS);
    i128::from(cycle) * i128::from(cycle_days.count) + i128::from(cycle_days.rank_before(within))
}

fn matching_date(
    day: i64,
    cycle_days: &MatchingDayCycle,
    forward: bool,
) -> Result<Option<Date>, ValidationError> {
    if cycle_days.count == 0 {
        return Ok(None);
    }
    let mut cycle = day.div_euclid(GREGORIAN_CYCLE_DAYS);
    let within = day.rem_euclid(GREGORIAN_CYCLE_DAYS);
    let offset = if forward {
        let rank = cycle_days.rank_before(within);
        if let Some(offset) = cycle_days.select(rank) {
            offset
        } else {
            cycle = cycle.saturating_add(1);
            cycle_days.select(0).expect("cycle has matches")
        }
    } else {
        let rank = cycle_days.rank_before(within.saturating_add(1));
        if rank > 0 {
            cycle_days.select(rank - 1).expect("rank is in cycle")
        } else {
            cycle = cycle.saturating_sub(1);
            cycle_days
                .select(cycle_days.count - 1)
                .expect("cycle has matches")
        }
    };
    let target = cycle
        .checked_mul(GREGORIAN_CYCLE_DAYS)
        .and_then(|base| base.checked_add(offset))
        .ok_or_else(|| cron_error("calendar date overflows"))?;
    Ok(date_from_day(target))
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

    fn brute_force_calendar(
        cron: &CronExpression,
        after: Timestamp,
        through: Timestamp,
        zone: &TimeZone,
    ) -> Vec<Timestamp> {
        const MINUTE_US: i64 = 60_000_000;
        let mut current = after
            .epoch_micros()
            .div_euclid(MINUTE_US)
            .saturating_add(1)
            .saturating_mul(MINUTE_US);
        let mut result = Vec::new();
        while current <= through.epoch_micros() {
            let instant = jiff::Timestamp::from_microsecond(current).unwrap();
            let zoned = instant.to_zoned(zone.clone());
            let datetime = zoned.datetime();
            let date = datetime.date();
            let hour = datetime.hour().cast_unsigned();
            let minute = datetime.minute().cast_unsigned();
            if cron.date_matches(date)
                && cron.hour.matches(hour)
                && cron.minute.matches(minute)
                && CronExpression::resolve_civil(date, hour, minute, zone).unwrap()
                    == Some(Timestamp::from_epoch_micros(current))
            {
                result.push(Timestamp::from_epoch_micros(current));
            }
            current = current.saturating_add(MINUTE_US);
        }
        result
    }

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
        assert!(CronExpression::parse("0 0 31 2 *").is_err());
    }

    #[test]
    fn reconciliation_range_is_cursor_exclusive_and_now_inclusive() {
        let schedule = Schedule::Every {
            interval: "1s".parse().unwrap(),
            anchor: Timestamp::UNIX_EPOCH,
        };
        let result = schedule
            .reconcile(
                Timestamp::from_epoch_micros(1_000_000),
                Timestamp::from_epoch_micros(2_000_000),
                MissedRunPolicy::All,
                None,
                10,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.selected[0].nominal.epoch_micros(), 2_000_000);
    }

    #[test]
    fn gregorian_jumps_terminate_at_supported_date_bounds() {
        let cron = CronExpression::parse("0 0 29 2 *").unwrap();
        assert!(cron.matching_date_at_or_after(Date::MIN).unwrap().is_some());
        assert!(
            cron.matching_date_at_or_before(Date::MAX)
                .unwrap()
                .is_some()
        );
        let never = CronExpression::parse("0 0 31 2 *");
        assert!(never.is_err());
    }

    #[test]
    fn steady_range_splits_older_boundaries_as_missed_without_silent_loss() {
        let schedule = Schedule::Every {
            interval: "1s".parse().unwrap(),
            anchor: Timestamp::UNIX_EPOCH,
        };
        let result = schedule
            .reconcile(
                Timestamp::UNIX_EPOCH,
                Timestamp::from_epoch_micros(3_000_000),
                MissedRunPolicy::Skip,
                None,
                2,
                &TimeZone::UTC,
                ElapsedKind::Normal,
            )
            .unwrap();
        assert_eq!(result.selected.len(), 1);
        assert!(!result.selected[0].catch_up);
        assert_eq!(result.selected[0].nominal.epoch_micros(), 3_000_000);
        assert_eq!(result.omitted.len(), 1);
        assert_eq!(result.omitted[0].count, 2);
        assert_eq!(result.omitted[0].kind, OmittedRangeKind::MissedRunPolicy);
    }

    #[test]
    fn interval_all_keeps_exact_newest_window_oldest_first() {
        let schedule = Schedule::Every {
            interval: "1s".parse().unwrap(),
            anchor: "2026-01-01T00:00:00Z".parse().unwrap(),
        };
        let result = schedule
            .reconcile(
                "2026-01-01T00:00:00Z".parse().unwrap(),
                "2026-01-12T13:46:40Z".parse().unwrap(),
                MissedRunPolicy::All,
                None,
                1000,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(result.selected.len(), 1000);
        assert!(
            result
                .selected
                .windows(2)
                .all(|pair| pair[0].nominal < pair[1].nominal)
        );
        assert_eq!(
            result.selected[0].nominal.to_string(),
            "2026-01-12T13:30:01Z"
        );
        assert_eq!(result.omitted[0].kind, OmittedRangeKind::CatchUpLimit);
        assert_eq!(result.omitted[0].count, 999_000);
        assert_eq!(result.omitted[0].first.to_string(), "2026-01-01T00:00:01Z");
        assert_eq!(result.omitted[0].last.to_string(), "2026-01-12T13:30:00Z");
    }

    #[test]
    fn catch_up_limit_one_keeps_only_the_newest_overdue_occurrence() {
        let schedule = Schedule::Every {
            interval: "1s".parse().unwrap(),
            anchor: Timestamp::UNIX_EPOCH,
        };
        let result = schedule
            .reconcile(
                Timestamp::UNIX_EPOCH,
                Timestamp::from_epoch_micros(3_000_000),
                MissedRunPolicy::All,
                None,
                1,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();

        assert_eq!(
            result.selected,
            [SelectedOccurrence {
                nominal: Timestamp::from_epoch_micros(3_000_000),
                catch_up: true,
            }]
        );
        assert_eq!(
            result.omitted,
            [OmittedRange {
                kind: OmittedRangeKind::CatchUpLimit,
                count: 2,
                first: Timestamp::from_epoch_micros(1_000_000),
                last: Timestamp::from_epoch_micros(2_000_000),
            }]
        );
    }

    #[test]
    fn start_deadline_filters_multiple_occurrences_before_missed_run_policy() {
        let schedule = Schedule::Every {
            interval: "1s".parse().unwrap(),
            anchor: Timestamp::UNIX_EPOCH,
        };
        let cases: &[(MissedRunPolicy, &[i64], u64, OmittedRangeKind, i64)] = &[
            (
                MissedRunPolicy::Skip,
                &[],
                4,
                OmittedRangeKind::MissedRunPolicy,
                10_000_000,
            ),
            (
                MissedRunPolicy::Latest,
                &[10_000_000],
                3,
                OmittedRangeKind::MissedRunPolicy,
                9_000_000,
            ),
            (
                MissedRunPolicy::All,
                &[9_000_000, 10_000_000],
                2,
                OmittedRangeKind::CatchUpLimit,
                8_000_000,
            ),
        ];

        for &(policy, selected_micros, policy_omitted_count, omitted_kind, omitted_last) in cases {
            let result = schedule
                .reconcile(
                    Timestamp::UNIX_EPOCH,
                    Timestamp::from_epoch_micros(10_000_000),
                    policy,
                    Some("3s".parse().unwrap()),
                    2,
                    &TimeZone::UTC,
                    ElapsedKind::Missed,
                )
                .unwrap();

            assert_eq!(
                result
                    .selected
                    .iter()
                    .map(|occurrence| occurrence.nominal.epoch_micros())
                    .collect::<Vec<_>>(),
                selected_micros,
                "policy: {policy:?}"
            );
            assert_eq!(result.omitted.len(), 2, "policy: {policy:?}");
            assert_eq!(
                result.omitted[0],
                OmittedRange {
                    kind: OmittedRangeKind::StartDeadline,
                    count: 6,
                    first: Timestamp::from_epoch_micros(1_000_000),
                    last: Timestamp::from_epoch_micros(6_000_000),
                },
                "policy: {policy:?}"
            );
            assert_eq!(result.omitted[1].kind, omitted_kind, "policy: {policy:?}");
            assert_eq!(
                result.omitted[1].count, policy_omitted_count,
                "policy: {policy:?}"
            );
            assert_eq!(
                result.omitted[1].first,
                Timestamp::from_epoch_micros(7_000_000),
                "policy: {policy:?}"
            );
            assert_eq!(
                result.omitted[1].last,
                Timestamp::from_epoch_micros(omitted_last),
                "policy: {policy:?}"
            );
        }
    }

    #[test]
    fn deadline_cutoff_is_inclusive_at_one_microsecond() {
        let schedule = Schedule::At {
            at: Timestamp::from_epoch_micros(9),
        };
        let eligible = schedule
            .reconcile(
                Timestamp::from_epoch_micros(0),
                Timestamp::from_epoch_micros(10),
                MissedRunPolicy::Latest,
                Some(DurationMicros::new(1)),
                1,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(eligible.selected.len(), 1);

        let expired = Schedule::At {
            at: Timestamp::from_epoch_micros(8),
        }
        .reconcile(
            Timestamp::from_epoch_micros(0),
            Timestamp::from_epoch_micros(10),
            MissedRunPolicy::Latest,
            Some(DurationMicros::new(1)),
            1,
            &TimeZone::UTC,
            ElapsedKind::Missed,
        )
        .unwrap();
        assert!(expired.selected.is_empty());
        assert_eq!(expired.omitted[0].kind, OmittedRangeKind::StartDeadline);
        assert_eq!(expired.omitted[0].count, 1);
    }

    #[test]
    fn missed_classification_is_explicit_not_wall_lateness() {
        let schedule = Schedule::At {
            at: "2026-01-01T00:00:00Z".parse().unwrap(),
        };
        let after = "2025-01-01T00:00:00Z".parse().unwrap();
        let now = "2026-12-31T00:00:00Z".parse().unwrap();
        let normal = schedule
            .reconcile(
                after,
                now,
                MissedRunPolicy::Skip,
                None,
                1,
                &TimeZone::UTC,
                ElapsedKind::Normal,
            )
            .unwrap();
        let recovery = schedule
            .reconcile(
                after,
                now,
                MissedRunPolicy::Skip,
                None,
                1,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(normal.selected.len(), 1);
        assert!(recovery.selected.is_empty());
        assert_eq!(recovery.omitted[0].count, 1);
    }

    #[test]
    fn long_sparse_calendar_range_has_exact_bounded_summary() {
        let schedule = utc_cron("0 0 29 2 *");
        let result = schedule
            .reconcile(
                "1900-01-01T00:00:00Z".parse().unwrap(),
                "2026-12-31T23:59:59Z".parse().unwrap(),
                MissedRunPolicy::All,
                None,
                10,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(result.selected.len(), 10);
        assert_eq!(
            result.selected[0].nominal.to_string(),
            "1988-02-29T00:00:00Z"
        );
        assert_eq!(
            result.selected[9].nominal.to_string(),
            "2024-02-29T00:00:00Z"
        );
        assert_eq!(result.omitted[0].count, 21);
        assert_eq!(result.omitted[0].first.to_string(), "1904-02-29T00:00:00Z");
        assert_eq!(result.omitted[0].last.to_string(), "1984-02-29T00:00:00Z");
    }

    #[test]
    fn compiled_calendar_is_reused_across_reconciliation_work() {
        CRON_COMPILE_COUNT.with(|count| count.set(0));
        let compiled = utc_cron("* * * * *").compile().unwrap();
        for through in ["2026-01-02T00:00:00Z", "2126-01-02T00:00:00Z"] {
            let result = compiled
                .reconcile(
                    "1900-01-01T00:00:00Z".parse().unwrap(),
                    through.parse().unwrap(),
                    MissedRunPolicy::Latest,
                    None,
                    1,
                    &TimeZone::UTC,
                    ElapsedKind::Missed,
                )
                .unwrap();
            assert_eq!(result.selected.len(), 1);
        }
        CRON_COMPILE_COUNT.with(|count| assert_eq!(count.get(), 1));
        let calendar = match &compiled.inner {
            CompiledScheduleKind::Cron { expression, .. } => &expression.matching_days,
            _ => unreachable!(),
        };
        assert!(calendar.words.len() < 2_500);
        assert!(calendar.prefix.len() < 2_500);
    }

    #[test]
    fn calendar_rank_and_newest_window_match_minute_oracle() {
        let after: Timestamp = "2025-01-01T00:00:00Z".parse().unwrap();
        let through: Timestamp = "2026-01-01T00:00:00Z".parse().unwrap();
        for (source, zone_name) in [
            ("*/17 1-5 * * mon-fri", "UTC"),
            ("5,35 0,12 31 * mon", "Asia/Seoul"),
            ("0 1-3 * 3,11 *", "America/New_York"),
        ] {
            let cron = CronExpression::parse(source).unwrap();
            let zone = TimeZone::get(zone_name).unwrap();
            let oracle = brute_force_calendar(&cron, after, through, &zone);
            let stats = cron.range_stats(after, through, &zone).unwrap();
            assert_eq!(stats.count, oracle.len() as u64, "{source} in {zone_name}");
            assert_eq!(
                stats.first,
                oracle.first().copied(),
                "{source} in {zone_name}"
            );
            assert_eq!(
                stats.last,
                oracle.last().copied(),
                "{source} in {zone_name}"
            );
            let expected = oracle
                .iter()
                .rev()
                .take(37)
                .rev()
                .copied()
                .collect::<Vec<_>>();
            assert_eq!(
                cron.newest_between(after, through, 37, &zone).unwrap(),
                expected,
                "{source} in {zone_name}"
            );
        }
    }

    #[test]
    fn calendar_count_preserves_dom_dow_or_and_dst_rules() {
        let or_schedule = utc_cron("0 9 31 * mon");
        let result = or_schedule
            .reconcile(
                "2026-08-01T00:00:00Z".parse().unwrap(),
                "2026-09-01T00:00:00Z".parse().unwrap(),
                MissedRunPolicy::All,
                None,
                100,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(result.selected.len(), 5);

        let gap = Schedule::Cron {
            expression: "30 2 * * *".into(),
            timezone: ScheduleTimeZone::Iana("America/New_York".into()),
        };
        let gap_result = gap
            .reconcile(
                "2026-03-07T00:00:00Z".parse().unwrap(),
                "2026-03-11T00:00:00Z".parse().unwrap(),
                MissedRunPolicy::All,
                None,
                100,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(gap_result.selected.len(), 3);
        assert!(
            gap_result
                .selected
                .iter()
                .all(|item| !item.nominal.to_string().starts_with("2026-03-08"))
        );

        let fold = Schedule::Cron {
            expression: "30 1 * * *".into(),
            timezone: ScheduleTimeZone::Iana("America/New_York".into()),
        };
        let fold_result = fold
            .reconcile(
                "2026-10-31T00:00:00Z".parse().unwrap(),
                "2026-11-03T00:00:00Z".parse().unwrap(),
                MissedRunPolicy::All,
                None,
                100,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(fold_result.selected.len(), 3);
        assert_eq!(
            fold_result.selected[1].nominal.to_string(),
            "2026-11-01T05:30:00Z"
        );
    }

    #[test]
    fn symbolic_local_follows_resolver_but_fixed_zone_does_not() {
        let local = Schedule::Cron {
            expression: "0 9 * * *".into(),
            timezone: ScheduleTimeZone::Local,
        };
        let after = "2026-08-20T00:00:00Z".parse().unwrap();
        let now = "2026-08-22T00:00:00Z".parse().unwrap();
        let utc = local
            .reconcile(
                after,
                now,
                MissedRunPolicy::Latest,
                None,
                1,
                &TimeZone::UTC,
                ElapsedKind::Missed,
            )
            .unwrap();
        let seoul_zone = TimeZone::get("Asia/Seoul").unwrap();
        let seoul = local
            .reconcile(
                after,
                now,
                MissedRunPolicy::Latest,
                None,
                1,
                &seoul_zone,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(utc.selected[0].nominal.to_string(), "2026-08-21T09:00:00Z");
        assert_eq!(
            seoul.selected[0].nominal.to_string(),
            "2026-08-22T00:00:00Z"
        );

        let fixed = utc_cron("0 9 * * *");
        let fixed_with_seoul_resolver = fixed
            .reconcile(
                after,
                now,
                MissedRunPolicy::Latest,
                None,
                1,
                &seoul_zone,
                ElapsedKind::Missed,
            )
            .unwrap();
        assert_eq!(fixed_with_seoul_resolver, utc);
    }

    #[test]
    fn backward_wall_move_is_empty() {
        let schedule = utc_cron("* * * * *");
        assert!(
            schedule
                .reconcile(
                    "2026-08-22T00:00:00Z".parse().unwrap(),
                    "2026-08-21T00:00:00Z".parse().unwrap(),
                    MissedRunPolicy::All,
                    None,
                    100,
                    &TimeZone::UTC,
                    ElapsedKind::Missed,
                )
                .unwrap()
                .selected
                .is_empty()
        );
    }
}
