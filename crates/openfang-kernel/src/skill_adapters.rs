//! Kernel-side adapters for the `openfang-skills` trait seams.
//!
//! Phase 1 plan 01-05 introduced two trait seams on `SkillRegistry`
//! (`AuditAppend`, `SkillEventBus`) so the registry could record
//! mutations and emit refresh events without depending on the kernel
//! crate (which would invert the crate DAG).
//!
//! Plan 01-08 wires the kernel-side implementations:
//!
//! - `KernelAuditAppender` forwards every skill mutation event onto the
//!   existing Merkle audit chain (`audit_log.record(...)`), tagging the
//!   action as `ConfigChange` and embedding `event_type` + the structured
//!   payload in the detail string. A serialization failure aborts the
//!   mutation as `SkillError::Io` so the registry's "atomic-with-audit"
//!   contract from plan 01-05 holds.
//!
//! - `KernelSkillEventBus` publishes a `skill.updated` envelope on the
//!   dedicated broadcast channel below so plan 01-09's snapshot refresh
//!   subscriber in the agent loop wakes up. The channel is created at
//!   kernel boot and lives on the kernel struct.

use openfang_runtime::audit::{AuditAction, AuditLog};
use openfang_skills::registry::{AuditAppend, SkillEventBus};
use openfang_skills::SkillError;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::debug;

/// Capacity of the `skill.updated` broadcast channel. 64 fits the
/// expected mutation rate (skills mutated by hand, not by automation)
/// and bounds the lag without dropping events.
const SKILL_UPDATED_CHANNEL_CAPACITY: usize = 64;

/// Payload delivered on the `skill.updated` channel after every
/// successful skill mutation. Plan 01-09's subscriber reads `name` to
/// invalidate the agent's registry snapshot.
#[derive(Debug, Clone)]
pub struct SkillUpdated {
    /// Name of the skill that was mutated.
    pub name: String,
}

/// Build a new `skill.updated` broadcast channel. The kernel keeps the
/// `Sender` on its struct so subscribers (plan 01-09) can call
/// `subscribe()` to get fresh receivers at any point during runtime.
pub fn new_skill_updated_channel() -> broadcast::Sender<SkillUpdated> {
    broadcast::channel(SKILL_UPDATED_CHANNEL_CAPACITY).0
}

/// Adapter that forwards skill mutation audit events into the kernel's
/// Merkle audit chain.
pub struct KernelAuditAppender {
    audit_log: Arc<AuditLog>,
}

impl KernelAuditAppender {
    /// Wrap the kernel's `AuditLog` so the skill registry can append into it.
    pub fn new(audit_log: Arc<AuditLog>) -> Self {
        Self { audit_log }
    }
}

impl AuditAppend for KernelAuditAppender {
    fn append(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), SkillError> {
        // Serialize the structured payload so the Merkle chain hashes the
        // full record verbatim. A serialization failure is treated as an
        // Io error so the registry's mutation method aborts before the
        // write hits disk — consistent with the "atomic-with-audit"
        // contract from plan 01-05.
        let detail_json = serde_json::to_string(&payload).map_err(|e| {
            SkillError::Io(std::io::Error::other(format!(
                "skill audit payload serialize failed: {e}"
            )))
        })?;
        // Tag every skill mutation as a ConfigChange — the audit chain
        // already has that variant; adding a new `SkillMutation` action
        // would cascade through every persisted entry decoder.
        let detail = format!("skill_event={event_type} {detail_json}");
        self.audit_log
            .record("kernel", AuditAction::ConfigChange, detail, "ok");
        Ok(())
    }
}

/// Adapter that publishes `SkillUpdated` events onto a dedicated
/// `tokio::sync::broadcast` channel. The publish is sync (the
/// `SkillEventBus` trait method is sync; the broadcast `send` is
/// non-blocking and infallible from the receiver's perspective). Errors
/// from `send` mean no subscribers — we degrade to a debug log so a
/// skill mutation never fails because nobody is listening yet (the
/// snapshot-refresh subscriber is added in plan 01-09).
pub struct KernelSkillEventBus {
    sender: broadcast::Sender<SkillUpdated>,
}

impl KernelSkillEventBus {
    /// Wrap a `broadcast::Sender` from `new_skill_updated_channel`.
    pub fn new(sender: broadcast::Sender<SkillUpdated>) -> Self {
        Self { sender }
    }
}

impl SkillEventBus for KernelSkillEventBus {
    fn publish_skill_updated(&self, name: &str) {
        let evt = SkillUpdated {
            name: name.to_string(),
        };
        if self.sender.send(evt).is_err() {
            debug!(
                skill = name,
                "skill.updated event has no subscribers — snapshot refresh deferred until plan 01-09 lands"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_appender_serializes_payload_into_detail() {
        let audit = Arc::new(AuditLog::new());
        let adapter = KernelAuditAppender::new(audit.clone());
        adapter
            .append(
                "skill_create",
                serde_json::json!({"name": "demo", "sha256": "abc"}),
            )
            .unwrap();
        assert_eq!(audit.len(), 1);
        let entries = audit.recent(10);
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert!(entry.detail.contains("skill_event=skill_create"));
        assert!(entry.detail.contains("\"name\":\"demo\""));
        assert!(entry.detail.contains("\"sha256\":\"abc\""));
    }

    #[test]
    fn event_bus_send_with_subscriber_succeeds() {
        let tx = new_skill_updated_channel();
        let mut rx = tx.subscribe();
        let bus = KernelSkillEventBus::new(tx);
        bus.publish_skill_updated("my-skill");
        // try_recv to keep the test sync
        let got = rx.try_recv().expect("expected a SkillUpdated event");
        assert_eq!(got.name, "my-skill");
    }

    #[test]
    fn event_bus_send_without_subscriber_is_a_silent_debug() {
        let tx = new_skill_updated_channel();
        let bus = KernelSkillEventBus::new(tx);
        // No subscriber — send returns Err under the hood, but the
        // adapter must not panic and must not propagate the error.
        bus.publish_skill_updated("orphan-skill");
    }
}
