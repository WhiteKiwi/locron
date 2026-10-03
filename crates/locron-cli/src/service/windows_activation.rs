//! Local authentication values; raw wire decoding remains a different type boundary.
//!
//! This pure qualification slice exposes no production constructor or live authentication path.
//! The effectful producer/consumer must be integrated before the module is enabled in production.

use serde::Serialize;

use super::Target;
use super::windows_activation_wire::{CanonicalUuid, Digest, ProcessId, Role};

impl From<Target> for Role {
    fn from(target: Target) -> Self {
        match target {
            Target::Daemon => Self::Daemon,
            Target::Dashboard => Self::Dashboard,
        }
    }
}

impl From<Role> for Target {
    fn from(role: Role) -> Self {
        match role {
            Role::Daemon => Self::Daemon,
            Role::Dashboard => Self::Dashboard,
        }
    }
}

/// Minted only by service-owned live authentication, never Deserialize or a public constructor.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct AuthenticatedActivation {
    context: CanonicalUuid,
    digest: Digest,
    scheduler_instance: CanonicalUuid,
    launcher_pid: ProcessId,
}

/// Fixture-only authority for serializing the actual complete bounded RuntimeFacts object.
/// This factory is absent from production and protected-fact/wire decoding.
#[cfg(test)]
pub(super) fn maximal_authenticated_activation_fixture() -> AuthenticatedActivation {
    let uuid = CanonicalUuid::parse("ffffffff-ffff-ffff-ffff-ffffffffffff").unwrap();
    AuthenticatedActivation {
        context: uuid.clone(),
        digest: Digest::from_bytes([255; 32]),
        scheduler_instance: uuid,
        launcher_pid: ProcessId::new(u32::MAX).unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Role, Target, maximal_authenticated_activation_fixture};
    use serde_json::json;

    #[test]
    fn role_conversion_preserves_both_fixed_targets() {
        for target in [Target::Daemon, Target::Dashboard] {
            let role = Role::from(target);
            assert_eq!(Target::from(role), target);
            assert_eq!(Role::parse(role.as_str()).unwrap(), role);
        }
    }

    #[test]
    fn maximal_witness_serializes_exactly_four_public_bindings_without_a_capability() {
        let witness = maximal_authenticated_activation_fixture();
        let encoded = serde_json::to_vec(&witness).unwrap();
        assert_eq!(encoded.len(), 212);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&encoded).unwrap(),
            json!({
                "context": "ffffffff-ffff-ffff-ffff-ffffffffffff",
                "digest": "f".repeat(64),
                "scheduler_instance": "ffffffff-ffff-ffff-ffff-ffffffffffff",
                "launcher_pid": u32::MAX,
            })
        );
        assert_eq!(serde_json::to_vec(&witness.clone()).unwrap(), encoded);
        assert!(format!("{witness:?}").contains("AuthenticatedActivation"));
        // Complete RuntimeFacts sizing remains the runtime consumer's actual whole-object test.
    }
}
