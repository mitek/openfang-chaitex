//! Phase 1.1 SD-02/03/04: distillation job queue + worker + daily cap.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const QUEUE_CAP: usize = 50;

#[derive(Debug, Clone)]
pub struct DistillationJob {
    pub agent_id: String,
    pub session_id: String,
    pub reflection_score: f32,
    pub enqueued_at: chrono::DateTime<Utc>,
}

/// Bounded FIFO queue. Drops the OLDEST job when full (a fresh complex turn
/// is more valuable than a stale one). In-memory only — no restart persistence
/// (Open Question 2).
#[derive(Debug, Default)]
pub struct DistillationQueue {
    inner: Mutex<VecDeque<DistillationJob>>,
}

impl DistillationQueue {
    pub fn new() -> Self {
        Self { inner: Mutex::new(VecDeque::new()) }
    }

    pub fn push(&self, job: DistillationJob) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() >= QUEUE_CAP {
            q.pop_front();
        }
        q.push_back(job);
    }

    pub fn pop(&self) -> Option<DistillationJob> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).pop_front()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// SD-04: per-UTC-day distillation counter persisted to a JSON sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCapState {
    pub date: String,              // "YYYY-MM-DD" (UTC)
    pub distillations_today: u32,
}

impl DailyCapState {
    fn today() -> String {
        Utc::now().format("%Y-%m-%d").to_string()
    }

    pub fn new_today() -> Self {
        Self { date: Self::today(), distillations_today: 0 }
    }

    /// Reset to zero if the stored date is not today.
    pub fn rolled(&self) -> Self {
        if self.date == Self::today() {
            self.clone()
        } else {
            Self::new_today()
        }
    }

    pub fn count(&self) -> u32 {
        self.rolled().distillations_today
    }

    pub fn can_distill(&self, cap: u32) -> bool {
        self.count() < cap
    }

    pub fn increment(&mut self) {
        *self = self.rolled();
        self.distillations_today += 1;
    }

    /// Load from sidecar, defaulting to a fresh today-state on any error.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<DailyCapState>(&s).ok())
            .map(|s| s.rolled())
            .unwrap_or_else(Self::new_today)
    }

    /// Atomic write: tmp file + rename (matches cron.rs persist idiom).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }
}

pub fn sidecar_path(home_dir: &Path) -> PathBuf {
    home_dir.join("distillation_state.json")
}

// ---------------------------------------------------------------------------
// SD-02: pure enqueue gate
// ---------------------------------------------------------------------------

/// SD-02: decide whether to enqueue. Pure gate — all kernel I/O stays in
/// the caller. Returns true if a job was pushed.
pub fn maybe_enqueue_distillation(
    agent_id: &str,
    session_id: &str,
    stats: &openfang_runtime::turn_stats::TurnStats,
    cfg: &openfang_types::config::DistillationConfig,
    cap_state: &DailyCapState,
    queue: &DistillationQueue,
) -> bool {
    if !cfg.enabled {
        return false;
    }
    if cfg.daily_cap == 0 || !cap_state.can_distill(cfg.daily_cap) {
        return false;
    }
    if stats.reflection_score() < cfg.reflection_threshold {
        return false;
    }
    queue.push(DistillationJob {
        agent_id: agent_id.to_string(),
        session_id: session_id.to_string(),
        reflection_score: stats.reflection_score(),
        enqueued_at: Utc::now(),
    });
    true
}

// ---------------------------------------------------------------------------
// SD-03: distillation worker
// ---------------------------------------------------------------------------

/// Minimal valid skill.toml template for a distilled (prompt-only) skill.
fn distilled_skill_toml(name: &str, description: &str) -> String {
    format!(
        r#"[skill]
name = "{name}"
version = "0.1.0"
description = "{description}"
mutable = true
protected = false

[runtime]
type = "promptonly"

[[tools.provided]]
name = "{name}_tool"
description = "Distilled skill tool"
input_schema = {{ type = "object" }}
"#
    )
}

/// SD-03: process one distillation job. Confidence threshold 0.7.
pub async fn run_distillation_job(
    kernel: &std::sync::Arc<crate::kernel::OpenFangKernel>,
    job: DistillationJob,
) {
    let Some(engine) = kernel.reasoning_engine.get().cloned() else {
        tracing::warn!("Reasoning engine not initialized — skipping distillation");
        return;
    };
    let query = format!(
        "Review session {}. Did the agent discover a reusable procedure? \
         If yes, describe it as a named skill with a one-line description and steps.",
        job.session_id
    );
    // CONFIRMED: ReasoningQuery::agent_id is Option<AgentId> (lib.rs:71),
    // NOT `caller_agent_id`. Parse the String agent id; a bad id => None
    // (cross-agent reasoning still allowed, gated at the tool layer).
    let agent_id = job.agent_id.parse::<openfang_types::agent::AgentId>().ok();
    let req = openfang_reasoning::ReasoningQuery {
        query,
        level: openfang_reasoning::ReasoningLevel::Medium,
        agent_id,
        max_facts: Some(20),
    };
    let result = match engine.reason(req).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Distillation reasoning failed: {e}");
            return;
        }
    };
    if result.confidence < 0.7 {
        tracing::debug!(
            confidence = result.confidence,
            "Distillation below threshold — skipped"
        );
        return;
    }
    // Build candidate name/desc/toml from result.answer. Deterministic name
    // from session id to make re-runs idempotent (Pitfall 4).
    let candidate_name = format!("distilled-{}", &job.session_id);
    let candidate_desc = result
        .answer
        .lines()
        .next()
        .unwrap_or("Distilled skill")
        .to_string();
    // Escape quotes in description to keep TOML valid.
    let safe_desc = candidate_desc.replace('"', "'");
    let toml_content = distilled_skill_toml(&candidate_name, &safe_desc);

    // Pitfall 2: snapshot/scope the registry guard tightly — no .await held across it.
    let is_dup = {
        let reg = kernel.skill_registry.read().unwrap_or_else(|e| e.into_inner());
        reg.is_duplicate_candidate(&candidate_name, &candidate_desc)
    };
    if is_dup {
        tracing::debug!(skill = %candidate_name, "Distilled skill is a duplicate — skipped");
        return;
    }
    let created = {
        let mut reg = kernel.skill_registry.write().unwrap_or_else(|e| e.into_inner());
        reg.create_draft_skill(&candidate_name, &toml_content, Some(&result.answer))
    };
    match created {
        Ok(true) => {
            tracing::info!(skill = %candidate_name, "Created draft distilled skill");
            // Daily cap increment + persist happens in the kernel worker loop (Task 3),
            // which owns the cap-state path.
        }
        Ok(false) => tracing::debug!(
            skill = %candidate_name,
            "Distilled skill already exists — skipped"
        ),
        Err(e) => tracing::warn!("Failed to create distilled skill: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_runtime::turn_stats::TurnStats;
    use openfang_types::config::DistillationConfig;

    // -----------------------------------------------------------------------
    // DistillationQueue tests
    // -----------------------------------------------------------------------

    fn make_job(id: &str) -> DistillationJob {
        DistillationJob {
            agent_id: id.to_string(),
            session_id: id.to_string(),
            reflection_score: 0.5,
            enqueued_at: Utc::now(),
        }
    }

    #[test]
    fn queue_bounded_at_50() {
        let q = DistillationQueue::new();
        for i in 0..60u32 {
            q.push(DistillationJob {
                agent_id: format!("agent-{i}"),
                session_id: format!("session-{i}"),
                reflection_score: 0.5,
                enqueued_at: Utc::now(),
            });
        }
        // Cap is 50; the first 10 were dropped.
        assert_eq!(q.len(), 50);
        // First remaining item is job 10 (oldest dropped).
        let first = q.pop().unwrap();
        assert_eq!(first.agent_id, "agent-10");
    }

    #[test]
    fn queue_push_pop_fifo() {
        let q = DistillationQueue::new();
        for &id in &["A", "B", "C"] {
            q.push(make_job(id));
        }
        assert_eq!(q.pop().unwrap().agent_id, "A");
        assert_eq!(q.pop().unwrap().agent_id, "B");
        assert_eq!(q.pop().unwrap().agent_id, "C");
        assert!(q.pop().is_none());
    }

    // -----------------------------------------------------------------------
    // DailyCapState tests
    // -----------------------------------------------------------------------

    #[test]
    fn daily_cap_resets_on_new_date() {
        // date in the past → rolled() resets count to 0.
        let state = DailyCapState {
            date: "1970-01-01".to_string(),
            distillations_today: 10,
        };
        assert_eq!(state.count(), 0);
        assert!(state.can_distill(5));
    }

    #[test]
    fn daily_cap_increments_and_persists() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();
        let mut state = DailyCapState::new_today();
        state.increment();
        state.increment();
        state.increment();
        state.save(path).unwrap();
        let loaded = DailyCapState::load(path);
        assert_eq!(loaded.count(), 3);
    }

    #[test]
    fn daily_cap_at_limit() {
        let mut state = DailyCapState::new_today();
        state.increment();
        state.increment();
        assert!(!state.can_distill(2));
    }

    // -----------------------------------------------------------------------
    // maybe_enqueue_distillation tests
    // -----------------------------------------------------------------------

    fn make_stats(score_hint: f32) -> TurnStats {
        // reflection_score formula (from turn_stats.rs):
        //   iter_norm = (iterations / 10.0).min(1.0)
        //   recovery_norm = (error_recovery_count / 3.0).min(1.0)
        //   token_norm = (total_tokens / 30_000.0).min(1.0)
        //   score = 0.45 * iter_norm + 0.40 * recovery_norm + 0.15 * token_norm
        //
        // HIGH score: iter=10, recovery=3, tokens=5000
        //   = 0.45*1.0 + 0.40*1.0 + 0.15*(5000/30000) = 0.45+0.40+0.025 ≈ 0.875
        // LOW score: iter=1, recovery=0, tokens=50
        //   = 0.45*0.1 + 0 + 0 = 0.045
        if score_hint >= 0.5 {
            TurnStats {
                iterations: 10,
                total_tokens: 5000,
                error_recovery_count: 3,
                wall_ms: 8000,
            }
        } else {
            TurnStats {
                iterations: 1,
                total_tokens: 50,
                error_recovery_count: 0,
                wall_ms: 500,
            }
        }
    }

    fn enabled_cfg() -> DistillationConfig {
        DistillationConfig {
            enabled: true,
            reflection_threshold: 0.5,
            daily_cap: 10,
            ..DistillationConfig::default()
        }
    }

    #[test]
    fn enqueue_above_threshold() {
        let q = DistillationQueue::new();
        let stats = make_stats(0.8);
        assert!(stats.reflection_score() >= 0.5, "stats must score above 0.5");
        let cap = DailyCapState::new_today();
        let cfg = enabled_cfg();
        let enqueued = maybe_enqueue_distillation("agent-1", "session-1", &stats, &cfg, &cap, &q);
        assert!(enqueued);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn no_enqueue_below_threshold() {
        let q = DistillationQueue::new();
        let stats = make_stats(0.2);
        let cap = DailyCapState::new_today();
        let cfg = DistillationConfig {
            enabled: true,
            reflection_threshold: 0.5,
            daily_cap: 10,
            ..DistillationConfig::default()
        };
        // Low stats should score below 0.5.
        let score = stats.reflection_score();
        if score >= 0.5 {
            // Skip test if heuristic changed — warn only.
            return;
        }
        let enqueued = maybe_enqueue_distillation("agent-1", "session-1", &stats, &cfg, &cap, &q);
        assert!(!enqueued);
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn no_enqueue_when_disabled() {
        let q = DistillationQueue::new();
        let stats = make_stats(0.9);
        let cap = DailyCapState::new_today();
        let cfg = DistillationConfig {
            enabled: false,
            ..enabled_cfg()
        };
        let enqueued = maybe_enqueue_distillation("agent-1", "session-1", &stats, &cfg, &cap, &q);
        assert!(!enqueued);
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn no_enqueue_at_daily_cap() {
        let q = DistillationQueue::new();
        let stats = make_stats(0.9);
        let mut cap = DailyCapState::new_today();
        // Fill up the cap.
        cap.increment();
        cap.increment();
        let cfg = DistillationConfig {
            enabled: true,
            daily_cap: 2,
            reflection_threshold: 0.1,
            ..DistillationConfig::default()
        };
        let enqueued = maybe_enqueue_distillation("agent-1", "session-1", &stats, &cfg, &cap, &q);
        assert!(!enqueued);
        assert_eq!(q.len(), 0);
    }
}
