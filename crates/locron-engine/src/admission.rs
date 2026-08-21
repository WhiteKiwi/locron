//! Pure admission and retry helpers.

use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

/// A queued work item considered by round-robin admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate<T> {
    /// Stable job identity used for fairness.
    pub job_id: String,
    /// Caller-owned work value.
    pub value: T,
}

/// Selects at most `capacity` items, rotating across jobs and admitting at most
/// one item per job in each pass.
#[must_use]
pub fn round_robin<T: Clone>(
    candidates: impl IntoIterator<Item = Candidate<T>>,
    after_job: Option<&str>,
    capacity: usize,
) -> Vec<Candidate<T>> {
    if capacity == 0 {
        return Vec::new();
    }
    let mut grouped: BTreeMap<String, VecDeque<Candidate<T>>> = BTreeMap::new();
    for candidate in candidates {
        grouped
            .entry(candidate.job_id.clone())
            .or_default()
            .push_back(candidate);
    }
    let mut jobs: Vec<String> = grouped.keys().cloned().collect();
    if let Some(after) = after_job {
        let split = jobs.partition_point(|job| job.as_str() <= after);
        jobs.rotate_left(split);
    }

    let mut selected = Vec::with_capacity(capacity.min(grouped.len()));
    while selected.len() < capacity {
        let mut progressed = false;
        for job in &jobs {
            if let Some(item) = grouped.get_mut(job).and_then(VecDeque::pop_front) {
                selected.push(item);
                progressed = true;
                if selected.len() == capacity {
                    break;
                }
            }
        }
        if !progressed {
            break;
        }
    }
    selected
}

/// Computes a retry delay with saturation and no jitter.
#[must_use]
pub fn retry_delay(base: Duration, cap: Duration, attempt: u32, exponential: bool) -> Duration {
    if !exponential || attempt <= 1 {
        return base.min(cap);
    }
    let shift = (attempt - 1).min(63);
    base.checked_mul(1_u32.checked_shl(shift).unwrap_or(u32::MAX))
        .unwrap_or(Duration::MAX)
        .min(cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(job: &str, value: i32) -> Candidate<i32> {
        Candidate {
            job_id: job.into(),
            value,
        }
    }

    #[test]
    fn noisy_job_cannot_take_first_pass() {
        let selected = round_robin(
            [
                item("a", 1),
                item("a", 2),
                item("a", 3),
                item("b", 4),
                item("c", 5),
            ],
            None,
            4,
        );
        assert_eq!(
            selected,
            [item("a", 1), item("b", 4), item("c", 5), item("a", 2)]
        );
    }

    #[test]
    fn cursor_rotates_starting_job() {
        let selected = round_robin([item("a", 1), item("b", 2), item("c", 3)], Some("b"), 2);
        assert_eq!(selected, [item("c", 3), item("a", 1)]);
    }

    #[test]
    fn retry_delay_caps_without_overflow() {
        assert_eq!(
            retry_delay(Duration::from_secs(10), Duration::from_secs(300), 50, true),
            Duration::from_secs(300)
        );
        assert_eq!(
            retry_delay(Duration::from_secs(10), Duration::from_secs(300), 3, false),
            Duration::from_secs(10)
        );
    }
}
