//! Phase 1.1 SD-01/SD-02: post-turn reflection primitives.
//! Pure heuristic — NO LLM, NO I/O. Safe to run inline on the post-turn
//! hook path (pc162-friendly).

use crate::agent_loop::AgentLoopResult;
use std::time::Duration;

/// Cheap per-turn statistics used to decide whether a completed agent turn
/// is worth distilling into a reusable skill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnStats {
    /// LLM round-trips in the loop (proxy for tool-call depth).
    pub iterations: u32,
    /// Total tokens (input + output) consumed by the turn.
    pub total_tokens: u64,
    /// Count of failure-then-recovery events within the turn.
    pub error_recovery_count: u32,
    /// Wall-clock duration of the turn in milliseconds.
    pub wall_ms: u64,
}

impl TurnStats {
    /// Build from a finished AgentLoopResult and the measured turn duration.
    pub fn from_result(result: &AgentLoopResult, elapsed: Duration) -> Self {
        Self {
            iterations: result.iterations,
            total_tokens: result.total_usage.total(),
            error_recovery_count: result.error_recovery_count,
            wall_ms: elapsed.as_millis() as u64,
        }
    }

    /// Heuristic distillation score in [0.0, 1.0]. Pure function.
    ///
    /// Weighting rationale: multi-step turns (high iterations) and turns
    /// that recovered from errors are the strongest signals that a reusable
    /// procedure was discovered; token volume is a weak secondary signal.
    pub fn reflection_score(&self) -> f32 {
        // Normalize each signal to [0,1] with saturating ceilings, then
        // combine with fixed weights summing to 1.0.
        let iter_norm = (self.iterations as f32 / 10.0).min(1.0);
        let recovery_norm = (self.error_recovery_count as f32 / 3.0).min(1.0);
        let token_norm = (self.total_tokens as f32 / 30_000.0).min(1.0);
        let score = 0.45 * iter_norm + 0.40 * recovery_norm + 0.15 * token_norm;
        score.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::message::TokenUsage;

    fn make_result(iterations: u32, total_tokens: u64, error_recovery_count: u32) -> AgentLoopResult {
        AgentLoopResult {
            response: String::new(),
            total_usage: TokenUsage {
                input_tokens: total_tokens / 2,
                output_tokens: total_tokens - total_tokens / 2,
            },
            iterations,
            cost_usd: None,
            silent: false,
            directives: Default::default(),
            error_recovery_count,
        }
    }

    #[test]
    fn turn_stats_from_result() {
        let result = make_result(6, 5000, 2);
        let dur = Duration::from_millis(8000);
        let stats = TurnStats::from_result(&result, dur);
        assert_eq!(stats.iterations, 6);
        assert_eq!(stats.total_tokens, 5000);
        assert_eq!(stats.error_recovery_count, 2);
        assert_eq!(stats.wall_ms, 8000);
    }

    #[test]
    fn reflection_score_in_range() {
        // Low case: iterations=1, tokens=100, recovery=0 → score < 0.3
        let low = TurnStats {
            iterations: 1,
            total_tokens: 100,
            error_recovery_count: 0,
            wall_ms: 500,
        };
        let low_score = low.reflection_score();
        assert!(
            (0.0..=1.0).contains(&low_score),
            "low score {low_score} out of [0,1]"
        );
        assert!(low_score < 0.3, "expected low score < 0.3, got {low_score}");

        // High case: iterations=10, tokens=30000, recovery=3 → score > 0.6
        let high = TurnStats {
            iterations: 10,
            total_tokens: 30_000,
            error_recovery_count: 3,
            wall_ms: 20_000,
        };
        let high_score = high.reflection_score();
        assert!(
            (0.0..=1.0).contains(&high_score),
            "high score {high_score} out of [0,1]"
        );
        assert!(high_score > 0.6, "expected high score > 0.6, got {high_score}");
    }

    #[test]
    fn reflection_score_monotonic_iterations() {
        // Holding tokens and recovery fixed, more iterations → higher or equal score.
        let base = TurnStats {
            iterations: 3,
            total_tokens: 1000,
            error_recovery_count: 1,
            wall_ms: 5000,
        };
        let more = TurnStats { iterations: 7, ..base };
        assert!(
            more.reflection_score() >= base.reflection_score(),
            "score should not decrease with more iterations"
        );
    }

    #[test]
    fn reflection_score_rewards_recovery() {
        // error_recovery_count=3 should score strictly higher than the same
        // stats with error_recovery_count=0 (failure-then-recovery is a strong
        // distillation signal).
        let no_recovery = TurnStats {
            iterations: 4,
            total_tokens: 2000,
            error_recovery_count: 0,
            wall_ms: 6000,
        };
        let with_recovery = TurnStats {
            error_recovery_count: 3,
            ..no_recovery
        };
        assert!(
            with_recovery.reflection_score() > no_recovery.reflection_score(),
            "recovery should increase score: {} vs {}",
            with_recovery.reflection_score(),
            no_recovery.reflection_score()
        );
    }
}
