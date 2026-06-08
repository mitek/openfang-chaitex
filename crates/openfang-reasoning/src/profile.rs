//! User-profile read/write surface (Phase 1 plan 01-14).
//!
//! Persists a structured per-agent user profile in the
//! `MemorySubstrate` structured KV under the namespaced key
//! `__user_profile__/<agent_uuid>`. The shape matches design § 2.8:
//! facts, preferences (keyed map), and behavioral patterns.
//!
//! Lifecycle:
//! 1. The agent calls the `memory_conclude` tool (plan 01-14) with a
//!    `kind = "fact" | "preference" | "pattern"`. The tool dispatcher
//!    loads the profile via [`load_profile`], applies the mutation
//!    helper (`add_fact`, `set_preference`, `add_pattern`) and persists
//!    via [`save_profile`].
//! 2. When `reasoning.auto_update_profile = true` (off by default per
//!    plan 01-12), the `memory_reason` tool ALSO calls `add_fact` after
//!    every Medium+ successful synthesis, persisting the answer as a
//!    derived fact. Write failures here are WARN-logged and do not fail
//!    the read path — opt-in writeback is best-effort.
//!
//! Locking discipline: KV writes go through `MemorySubstrate::
//! structured_set` (sync). We never hold the substrate's connection
//! mutex across an `.await`.

use crate::ReasoningError;
use chrono::Utc;
use openfang_memory::MemorySubstrate;
use openfang_types::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Namespace prefix for the per-agent profile KV key. The final key is
/// `__user_profile__/<agent_uuid>` (one entry per agent — see SUMMARY
/// for the rationale).
const PROFILE_KEY_PREFIX: &str = "__user_profile__";

/// Build the canonical KV key for an agent's profile.
fn profile_key(agent_id: &AgentId) -> String {
    format!("{PROFILE_KEY_PREFIX}/{}", agent_id.0)
}

/// Where a [`UserFact`] originated. Matches the design's `FactSource`
/// enum surface so the profile is self-describing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FactSource {
    /// Direct observation in a stored session.
    Session {
        /// `SessionId` string form.
        session_id: String,
    },
    /// Derived from a `memory_reason` synthesis call.
    MemoryReason {
        /// Reasoning level that produced the fact, lowercase.
        level: String,
    },
    /// Explicitly entered via `memory_conclude` with no upstream source.
    StructuredKv {
        /// KV key the fact lives under (informational).
        key: String,
    },
}

impl Default for FactSource {
    fn default() -> Self {
        Self::StructuredKv {
            key: PROFILE_KEY_PREFIX.to_string(),
        }
    }
}

/// A single fact the agent has learned about the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserFact {
    /// Natural-language fact, e.g. "User prefers Rust over Python".
    pub fact: String,
    /// Confidence in `[0.0, 1.0]`. Higher = more certain.
    pub confidence: f32,
    /// Where this fact came from (session / reasoning / explicit).
    pub source: FactSource,
    /// RFC3339 timestamp of first observation.
    pub first_observed: String,
    /// RFC3339 timestamp of most recent confirmation.
    pub last_confirmed: String,
}

/// A typed preference keyed by a stable identifier (e.g. `"language"`,
/// `"timezone"`). Replaces the prior value when re-set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preference {
    /// Preference key (stable identifier).
    pub key: String,
    /// Preference value.
    pub value: String,
    /// Confidence in `[0.0, 1.0]`.
    pub confidence: f32,
}

/// A behavioral pattern the agent has observed across interactions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehavioralPattern {
    /// Natural-language pattern, e.g. "Often asks questions late at night".
    pub pattern: String,
    /// How many times this pattern has been confirmed.
    pub occurrences: u32,
    /// RFC3339 timestamp of first sighting.
    pub first_seen: String,
    /// RFC3339 timestamp of most recent sighting.
    pub last_seen: String,
}

/// Per-agent user profile persisted to structured KV.
///
/// Serialized as a single JSON value at key
/// `__user_profile__/<agent_uuid>`. The `agent_id` is `Option` so an
/// empty `Default` can be round-tripped before an agent ID is known.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    /// Owning agent, if known.
    pub agent_id: Option<AgentId>,
    /// Discrete facts about the user.
    pub facts: Vec<UserFact>,
    /// Keyed preferences (e.g. `language → "Rust"`).
    pub preferences: HashMap<String, Preference>,
    /// Observed behavioral patterns.
    pub patterns: Vec<BehavioralPattern>,
    /// RFC3339 timestamp of the last `save_profile` call.
    pub updated_at: String,
}

/// Load the per-agent profile from structured KV.
///
/// Returns a fresh `UserProfile::default()` (with `agent_id` populated)
/// when the key is missing — callers always get a writable profile.
pub fn load_profile(
    memory: &MemorySubstrate,
    agent_id: &AgentId,
) -> Result<UserProfile, ReasoningError> {
    let key = profile_key(agent_id);
    let raw = memory
        .structured_get(*agent_id, &key)
        .map_err(|e| ReasoningError::Memory(format!("load_profile: {e}")))?;

    match raw {
        Some(value) => {
            // The substrate may return either a JSON string (preferred — what
            // `save_profile` writes) or a JSON object directly. Handle both.
            let profile: UserProfile = if let Some(s) = value.as_str() {
                serde_json::from_str(s)
                    .map_err(|e| ReasoningError::Memory(format!("load_profile parse: {e}")))?
            } else {
                serde_json::from_value(value)
                    .map_err(|e| ReasoningError::Memory(format!("load_profile parse: {e}")))?
            };
            Ok(profile)
        }
        None => Ok(UserProfile {
            agent_id: Some(*agent_id),
            ..Default::default()
        }),
    }
}

/// Persist the profile to structured KV.
///
/// Always stamps `updated_at` to `Utc::now().to_rfc3339()` before
/// writing. `agent_id` falls back to the supplied `agent_id` argument
/// when the profile doesn't carry one yet.
pub fn save_profile(
    memory: &MemorySubstrate,
    agent_id: &AgentId,
    profile: &mut UserProfile,
) -> Result<(), ReasoningError> {
    if profile.agent_id.is_none() {
        profile.agent_id = Some(*agent_id);
    }
    profile.updated_at = Utc::now().to_rfc3339();
    let json = serde_json::to_string(profile)
        .map_err(|e| ReasoningError::Memory(format!("save_profile encode: {e}")))?;
    memory
        .structured_set(*agent_id, &profile_key(agent_id), serde_json::Value::String(json))
        .map_err(|e| ReasoningError::Memory(format!("save_profile: {e}")))?;
    Ok(())
}

/// Append a fact to the profile (no dedupe — the agent decides what's a
/// duplicate via `memory_reason`).
pub fn add_fact(profile: &mut UserProfile, fact: UserFact) {
    profile.facts.push(fact);
}

/// Insert / overwrite a preference. Returns the prior value if any.
pub fn set_preference(profile: &mut UserProfile, pref: Preference) -> Option<Preference> {
    profile.preferences.insert(pref.key.clone(), pref)
}

/// Add a behavioral pattern. If a pattern with identical `pattern` text
/// already exists, increment its `occurrences` and bump `last_seen`
/// rather than creating a duplicate row.
pub fn add_pattern(profile: &mut UserProfile, mut pat: BehavioralPattern) {
    if let Some(existing) = profile.patterns.iter_mut().find(|p| p.pattern == pat.pattern) {
        existing.occurrences = existing.occurrences.saturating_add(pat.occurrences);
        existing.last_seen = pat.last_seen.clone();
        return;
    }
    if pat.first_seen.is_empty() {
        pat.first_seen = Utc::now().to_rfc3339();
    }
    if pat.last_seen.is_empty() {
        pat.last_seen = pat.first_seen.clone();
    }
    profile.patterns.push(pat);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_memory() -> MemorySubstrate {
        MemorySubstrate::open_in_memory(0.0).expect("in-memory substrate must open")
    }

    fn now() -> String {
        Utc::now().to_rfc3339()
    }

    #[test]
    fn load_profile_returns_default_when_missing() {
        let mem = fresh_memory();
        let agent_id = AgentId::new();
        let profile = load_profile(&mem, &agent_id).expect("load ok");
        assert_eq!(profile.facts.len(), 0);
        assert_eq!(profile.preferences.len(), 0);
        assert_eq!(profile.patterns.len(), 0);
        assert_eq!(profile.agent_id, Some(agent_id));
    }

    #[test]
    fn save_and_load_round_trip() {
        let mem = fresh_memory();
        let agent_id = AgentId::new();
        let mut profile = UserProfile {
            agent_id: Some(agent_id),
            ..Default::default()
        };
        add_fact(
            &mut profile,
            UserFact {
                fact: "user likes Rust".to_string(),
                confidence: 0.9,
                source: FactSource::StructuredKv {
                    key: "__memory_conclude__".to_string(),
                },
                first_observed: now(),
                last_confirmed: now(),
            },
        );
        save_profile(&mem, &agent_id, &mut profile).expect("save ok");
        let loaded = load_profile(&mem, &agent_id).expect("load ok");
        assert_eq!(loaded.facts.len(), 1);
        assert_eq!(loaded.facts[0].fact, "user likes Rust");
        assert!((loaded.facts[0].confidence - 0.9).abs() < 1e-6);
        assert!(!loaded.updated_at.is_empty());
    }

    #[test]
    fn set_preference_returns_prior_value() {
        let mut profile = UserProfile::default();
        let prior = set_preference(
            &mut profile,
            Preference {
                key: "language".to_string(),
                value: "Rust".to_string(),
                confidence: 0.8,
            },
        );
        assert!(prior.is_none());
        let prior2 = set_preference(
            &mut profile,
            Preference {
                key: "language".to_string(),
                value: "Python".to_string(),
                confidence: 0.4,
            },
        );
        assert_eq!(prior2.map(|p| p.value).as_deref(), Some("Rust"));
        assert_eq!(profile.preferences.get("language").unwrap().value, "Python");
    }

    #[test]
    fn add_pattern_dedupes_on_text() {
        let mut profile = UserProfile::default();
        let t = now();
        add_pattern(
            &mut profile,
            BehavioralPattern {
                pattern: "asks at night".to_string(),
                occurrences: 1,
                first_seen: t.clone(),
                last_seen: t.clone(),
            },
        );
        add_pattern(
            &mut profile,
            BehavioralPattern {
                pattern: "asks at night".to_string(),
                occurrences: 2,
                first_seen: t.clone(),
                last_seen: t.clone(),
            },
        );
        assert_eq!(profile.patterns.len(), 1);
        assert_eq!(profile.patterns[0].occurrences, 3);
    }

    #[test]
    fn profile_keys_are_namespaced_per_agent() {
        let a = AgentId::new();
        let b = AgentId::new();
        assert_ne!(profile_key(&a), profile_key(&b));
        assert!(profile_key(&a).starts_with("__user_profile__/"));
    }
}
