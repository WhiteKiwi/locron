use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ValidationError;

macro_rules! uuid_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Allocate a time-ordered RFC 9562 UUIDv7 identity.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = ValidationError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let parsed = Uuid::parse_str(value).map_err(|error| {
                    ValidationError::new($field, "invalid_uuid", error.to_string())
                })?;
                if value != parsed.hyphenated().to_string() {
                    return Err(ValidationError::new(
                        $field,
                        "non_canonical_uuid",
                        "identity must use lowercase canonical UUID syntax",
                    ));
                }
                Ok(Self(parsed))
            }
        }
    };
}

uuid_id!(JobId, "job_id");
uuid_id!(RunId, "run_id");
uuid_id!(SchedulerLifetimeId, "scheduler_lifetime_id");

macro_rules! positive_number {
    ($name:ident, $field:literal, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            /// Creates a positive durable sequence value.
            pub fn new(value: u64) -> Result<Self, ValidationError> {
                if value == 0 {
                    return Err(ValidationError::new(
                        $field,
                        "zero_sequence",
                        concat!($description, " must be greater than zero"),
                    ));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl TryFrom<u64> for $name {
            type Error = ValidationError;

            fn try_from(value: u64) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

positive_number!(
    RevisionNumber,
    "revision",
    "parent-scoped job revision number"
);
positive_number!(AttemptNumber, "attempt", "parent-scoped run attempt number");
positive_number!(EventId, "event_id", "database-local event identity");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trips_in_canonical_form() {
        let id = JobId::new();
        assert_eq!(id.to_string().parse::<JobId>().unwrap(), id);
        assert_eq!(id.as_uuid().get_version_num(), 7);
    }

    #[test]
    fn identity_rejects_non_canonical_spelling() {
        let upper = JobId::new().to_string().to_uppercase();
        assert_eq!(
            upper.parse::<JobId>().unwrap_err().code,
            "non_canonical_uuid"
        );
    }

    #[test]
    fn durable_sequence_values_are_positive_and_typed() {
        assert_eq!(RevisionNumber::new(1).unwrap().get(), 1);
        assert_eq!(AttemptNumber::new(2).unwrap().to_string(), "2");
        assert_eq!(EventId::new(3).unwrap().get(), 3);
        assert_eq!(RevisionNumber::new(0).unwrap_err().code, "zero_sequence");
        assert_eq!(AttemptNumber::new(0).unwrap_err().field, "attempt");
        assert_eq!(EventId::new(0).unwrap_err().field, "event_id");
    }
}
