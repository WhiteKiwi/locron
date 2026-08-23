//! Wall-clock timestamps and non-negative duration values.

use std::{fmt, str::FromStr, time::Duration};

use jiff::Timestamp as JiffTimestamp;
use serde::{Deserialize, Serialize};

use crate::ValidationError;

/// A durable UTC instant represented as signed Unix epoch microseconds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// The Unix epoch instant.
    pub const UNIX_EPOCH: Self = Self(0);

    /// Wraps a value already expressed as signed Unix epoch microseconds.
    #[must_use]
    pub const fn from_epoch_micros(value: i64) -> Self {
        Self(value)
    }

    /// Returns the signed Unix epoch microseconds value.
    #[must_use]
    pub const fn epoch_micros(self) -> i64 {
        self.0
    }

    /// Adds a duration, returning `None` on overflow of the underlying value.
    pub fn checked_add(self, duration: DurationMicros) -> Option<Self> {
        let micros = i64::try_from(duration.0).ok()?;
        self.0.checked_add(micros).map(Self)
    }

    /// Subtracts a duration, returning `None` on underflow of the underlying value.
    pub fn checked_sub(self, duration: DurationMicros) -> Option<Self> {
        let micros = i64::try_from(duration.0).ok()?;
        self.0.checked_sub(micros).map(Self)
    }

    pub(crate) fn to_jiff(self) -> std::result::Result<JiffTimestamp, ValidationError> {
        JiffTimestamp::from_microsecond(self.0).map_err(|error| {
            ValidationError::new("timestamp", "timestamp_out_of_range", error.to_string())
        })
    }

    pub(crate) fn from_jiff(value: JiffTimestamp) -> Self {
        Self(value.as_microsecond())
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_jiff() {
            Ok(value) => value.fmt(f),
            Err(_) => write!(f, "{}us", self.0),
        }
    }
}

impl FromStr for Timestamp {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if !has_explicit_offset(value) {
            return Err(ValidationError::new(
                "timestamp",
                "offset_required",
                "timestamp must include Z or an explicit numeric offset",
            ));
        }
        let parsed: JiffTimestamp = value.parse().map_err(|error: jiff::Error| {
            ValidationError::new("timestamp", "invalid_timestamp", error.to_string())
        })?;
        Ok(Self::from_jiff(parsed))
    }
}

fn has_explicit_offset(value: &str) -> bool {
    if value.ends_with('Z') || value.ends_with('z') {
        return true;
    }
    let Some(time_start) = value.find('T').or_else(|| value.find('t')) else {
        return false;
    };
    value[time_start + 1..]
        .char_indices()
        .any(|(_, character)| character == '+' || character == '-')
}

/// Non-negative elapsed time in integer microseconds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DurationMicros(u64);

impl DurationMicros {
    /// Zero duration.
    pub const ZERO: Self = Self(0);
    /// One second.
    pub const SECOND: Self = Self(1_000_000);
    /// One minute.
    pub const MINUTE: Self = Self(60_000_000);

    /// Wraps a value already expressed in microseconds.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the underlying microseconds value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Multiplies by a scalar, saturating at the representable maximum.
    #[must_use]
    pub fn saturating_mul(self, rhs: u64) -> Self {
        Self(self.0.saturating_mul(rhs))
    }
}

impl From<Duration> for DurationMicros {
    fn from(value: Duration) -> Self {
        Self(u64::try_from(value.as_micros()).unwrap_or(u64::MAX))
    }
}

impl FromStr for DurationMicros {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() < 2 {
            return Err(invalid_duration());
        }
        let (number, suffix) = value.split_at(value.len() - 1);
        let count = number.parse::<u64>().map_err(|_| invalid_duration())?;
        if count == 0 {
            return Err(ValidationError::new(
                "duration",
                "duration_zero",
                "duration must be greater than zero",
            ));
        }
        let seconds = match suffix {
            "s" => Some(count),
            "m" => count.checked_mul(60),
            "h" => count.checked_mul(60 * 60),
            "d" => count.checked_mul(24 * 60 * 60),
            _ => return Err(invalid_duration()),
        }
        .ok_or_else(|| {
            ValidationError::new("duration", "duration_overflow", "duration is too large")
        })?;
        let micros = seconds.checked_mul(1_000_000).ok_or_else(|| {
            ValidationError::new("duration", "duration_overflow", "duration is too large")
        })?;
        Ok(Self(micros))
    }
}

fn invalid_duration() -> ValidationError {
    ValidationError::new(
        "duration",
        "invalid_duration",
        "duration must be an integer followed by s, m, h, or d",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_duration_units() {
        assert_eq!("3s".parse::<DurationMicros>().unwrap().get(), 3_000_000);
        assert_eq!("2m".parse::<DurationMicros>().unwrap().get(), 120_000_000);
        assert_eq!("1h".parse::<DurationMicros>().unwrap().get(), 3_600_000_000);
        assert_eq!(
            "1d".parse::<DurationMicros>().unwrap().get(),
            86_400_000_000
        );
    }

    #[test]
    fn duration_rejects_zero_and_overflow() {
        assert_eq!(
            "0s".parse::<DurationMicros>().unwrap_err().code,
            "duration_zero"
        );
        assert_eq!(
            format!("{}d", u64::MAX)
                .parse::<DurationMicros>()
                .unwrap_err()
                .code,
            "duration_overflow"
        );
    }

    #[test]
    fn timestamp_requires_an_offset() {
        assert!("2026-08-21T12:00:00".parse::<Timestamp>().is_err());
        assert_eq!(
            "2026-08-21T12:00:00+09:00".parse::<Timestamp>().unwrap(),
            "2026-08-21T03:00:00Z".parse::<Timestamp>().unwrap()
        );
    }
}
