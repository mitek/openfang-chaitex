//! Phase 1.1 SI-01/SI-03: bounded, TTL-decaying tracker of skill_execute
//! failures keyed by (skill_name, agent_id). Kernel-global; recorded from
//! tool dispatch, checked by the patch-proposal logic (Plan 06).

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use dashmap::DashMap;
use std::collections::VecDeque;

/// Max events retained per (skill, agent) key — bounds memory on pc162.
const MAX_EVENTS_PER_KEY: usize = 20;
/// Default rolling window; events older than this are dropped on insert.
const DEFAULT_TTL_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy)]
pub struct FailureEvent {
    pub timestamp: DateTime<Utc>,
    /// Hash of the error message, so the patch logic can detect a *repeated*
    /// failure pattern vs. assorted distinct failures.
    pub error_hash: u64,
}

#[derive(Debug, Default)]
pub struct SkillFailureTracker {
    events: DashMap<(String, String), VecDeque<FailureEvent>>,
    ttl_days: i64,
}

impl SkillFailureTracker {
    pub fn new() -> Self {
        Self {
            events: DashMap::new(),
            ttl_days: DEFAULT_TTL_DAYS,
        }
    }

    pub fn with_ttl_days(ttl_days: i64) -> Self {
        Self {
            events: DashMap::new(),
            ttl_days,
        }
    }

    /// Record a failure now.
    pub fn record_failure(&self, skill: &str, agent: &str, error_hash: u64) {
        self.record_failure_at(skill, agent, error_hash, Utc::now());
    }

    /// Test/explicit-timestamp variant.
    pub fn record_failure_at(
        &self,
        skill: &str,
        agent: &str,
        error_hash: u64,
        ts: DateTime<Utc>,
    ) {
        let key = (skill.to_string(), agent.to_string());
        let cutoff = Utc::now() - ChronoDuration::days(self.ttl_days);
        let mut entry = self.events.entry(key).or_default();
        // Decay: drop stale entries.
        entry.retain(|e| e.timestamp >= cutoff);
        entry.push_back(FailureEvent {
            timestamp: ts,
            error_hash,
        });
        // Bound: drop oldest beyond cap.
        while entry.len() > MAX_EVENTS_PER_KEY {
            entry.pop_front();
        }
    }

    /// Count of non-stale failures currently retained for the key.
    pub fn failures_in_window(&self, skill: &str, agent: &str) -> usize {
        let cutoff = Utc::now() - ChronoDuration::days(self.ttl_days);
        self.events
            .get(&(skill.to_string(), agent.to_string()))
            .map(|q| q.iter().filter(|e| e.timestamp >= cutoff).count())
            .unwrap_or(0)
    }

    /// True when retained failures meet/exceed `threshold`.
    pub fn reached_threshold(&self, skill: &str, agent: &str, threshold: u32) -> bool {
        self.failures_in_window(skill, agent) >= threshold as usize
    }

    /// Clear a key after a patch proposal is raised (prevents repeat proposals).
    pub fn clear(&self, skill: &str, agent: &str) {
        self.events.remove(&(skill.to_string(), agent.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_failure_tracker_record_increments_count() {
        let tracker = SkillFailureTracker::new();
        tracker.record_failure("skill-a", "agent-1", 0xDEAD);
        assert_eq!(tracker.failures_in_window("skill-a", "agent-1"), 1);
    }

    #[test]
    fn skill_failure_tracker_distinct_keys_isolated() {
        let tracker = SkillFailureTracker::new();
        tracker.record_failure("skill-a", "agent-1", 0x1111);
        tracker.record_failure("skill-a", "agent-1", 0x2222);

        // Different agent, same skill — should be 0
        assert_eq!(tracker.failures_in_window("skill-a", "agent-2"), 0);
        // Different skill, same agent — should be 0
        assert_eq!(tracker.failures_in_window("skill-b", "agent-1"), 0);
        // Original key should have 2
        assert_eq!(tracker.failures_in_window("skill-a", "agent-1"), 2);
    }

    #[test]
    fn skill_failure_tracker_bounded_at_20() {
        let tracker = SkillFailureTracker::new();
        for i in 0..25 {
            tracker.record_failure("skill-x", "agent-1", i as u64);
        }
        assert_eq!(tracker.failures_in_window("skill-x", "agent-1"), 20);
    }

    #[test]
    fn skill_failure_tracker_ttl_decay() {
        let tracker = SkillFailureTracker::new();
        // Insert a stale event (8 days ago — outside the 7-day TTL).
        let stale_ts = Utc::now() - ChronoDuration::days(8);
        tracker.record_failure_at("skill-y", "agent-1", 0xBAD, stale_ts);
        // Insert a fresh failure now.
        tracker.record_failure("skill-y", "agent-1", 0xC0FFEE);
        // Only the fresh one should be counted.
        assert_eq!(tracker.failures_in_window("skill-y", "agent-1"), 1);
    }

    #[test]
    fn skill_failure_tracker_reached_threshold() {
        let tracker = SkillFailureTracker::new();
        tracker.record_failure("skill-z", "agent-1", 0x1);
        tracker.record_failure("skill-z", "agent-1", 0x2);
        tracker.record_failure("skill-z", "agent-1", 0x3);

        assert!(tracker.reached_threshold("skill-z", "agent-1", 3));
        assert!(!tracker.reached_threshold("skill-z", "agent-1", 4));
    }
}
