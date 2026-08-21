//! Pure admission and retry helpers.

use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use locron_core::policy::{BackoffMode, ExecutionPolicy};

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

/// Known attempt result used by the retry policy. Unknown and control-flow
/// outcomes are represented explicitly so they can never become retryable by
/// an adapter accident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryClass {
    /// Successful target completion.
    Succeeded,
    /// Known process or transport/status failure eligible by default.
    KnownFailure,
    /// Attempt timeout, eligible only with explicit opt-in.
    Timeout,
    /// User cancellation.
    Cancelled,
    /// Invalid or unavailable runtime configuration.
    Configuration,
    /// Overlap replacement termination.
    Replacement,
    /// Outcome lost across scheduler lifetime termination.
    InterruptedUnknown,
}

/// Durable retry decision for the same run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryDecision {
    /// Durable UTC eligibility instant.
    pub not_before_us: i64,
    /// Stable known-result classification.
    pub classification: &'static str,
}

/// Applies retry eligibility, count, and fixed/capped-exponential delay using
/// one injected completion instant.
#[must_use]
pub fn decide_retry(
    policy: &ExecutionPolicy,
    attempt: u32,
    completed_at_us: i64,
    class: RetryClass,
) -> Option<RetryDecision> {
    if attempt == 0 || attempt > u32::from(policy.retries) {
        return None;
    }
    let classification = match class {
        RetryClass::KnownFailure => "known_failure",
        RetryClass::Timeout if policy.retry_timeout => "timeout",
        RetryClass::Succeeded
        | RetryClass::Timeout
        | RetryClass::Cancelled
        | RetryClass::Configuration
        | RetryClass::Replacement
        | RetryClass::InterruptedUnknown => return None,
    };
    let delay = retry_delay(
        Duration::from_micros(policy.retry_delay.get()),
        Duration::from_micros(policy.retry_cap.get()),
        attempt,
        matches!(policy.backoff, BackoffMode::Exponential),
    );
    Some(RetryDecision {
        not_before_us: completed_at_us
            .saturating_add(i64::try_from(delay.as_micros()).unwrap_or(i64::MAX)),
        classification,
    })
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

    #[test]
    fn retry_decision_covers_fixed_exponential_and_forbidden_classes() {
        let mut policy = ExecutionPolicy {
            retries: 3,
            retry_delay: "10s".parse().unwrap(),
            retry_cap: "25s".parse().unwrap(),
            ..ExecutionPolicy::default()
        };
        policy.backoff = BackoffMode::Fixed;
        assert_eq!(
            decide_retry(&policy, 2, 1_000_000, RetryClass::KnownFailure)
                .unwrap()
                .not_before_us,
            11_000_000
        );
        policy.backoff = BackoffMode::Exponential;
        assert_eq!(
            decide_retry(&policy, 3, 1_000_000, RetryClass::KnownFailure)
                .unwrap()
                .not_before_us,
            26_000_000
        );
        for class in [
            RetryClass::Succeeded,
            RetryClass::Timeout,
            RetryClass::Cancelled,
            RetryClass::Configuration,
            RetryClass::Replacement,
            RetryClass::InterruptedUnknown,
        ] {
            assert_eq!(decide_retry(&policy, 1, 0, class), None);
        }
        policy.retry_timeout = true;
        assert_eq!(
            decide_retry(&policy, 1, 0, RetryClass::Timeout)
                .unwrap()
                .classification,
            "timeout"
        );
        assert_eq!(decide_retry(&policy, 4, 0, RetryClass::KnownFailure), None);
    }
}
