//! `ReasoningEngine::reason` body — five-level dispatch.
//!
//! Design § 2.4 maps each level to a retrieval + synthesis strategy:
//!
//! | Level   | Retrieval                     | Synthesis  | Confidence base |
//! |---------|-------------------------------|------------|-----------------|
//! | Minimal | KV exact + semantic fallback  | none       | 0.4             |
//! | Low     | semantic + FTS5               | LLM        | 0.5 + 0.05·n    |
//! | Medium  | semantic + FTS + KV + graph   | LLM        | 0.7 + 0.02·n    |
//! | High    | same as Medium                | LLM        | 0.8 + 0.02·n    |
//! | Max     | same as Medium                | LLM        | 0.85 + 0.02·n   |
//!
//! First-turn handling: when retrieval returns no facts, the result is
//! `ReasoningResult { confidence: 0.0, caveats: ["No conversation
//! history available — answers are speculative."], ... }` regardless of
//! level. Plan 01-13's tool layer surfaces the caveat to the agent
//! verbatim.

use crate::fact_retrieval::retrieve_facts;
use crate::{
    FactReference, ReasoningEngine, ReasoningError, ReasoningLevel, ReasoningQuery,
    ReasoningResult,
};

/// Verbatim caveat string (skeleton open-decision 4). Pinned by a test in
/// the test module so a future refactor can't quietly change the wording
/// that the agent / dashboard renders.
pub const FIRST_TURN_CAVEAT: &str =
    "No conversation history available — answers are speculative.";

/// Entry point. `ReasoningEngine::reason` delegates here.
pub async fn reason_impl(
    engine: &ReasoningEngine,
    query: ReasoningQuery,
) -> Result<ReasoningResult, ReasoningError> {
    match query.level {
        ReasoningLevel::Minimal => reason_minimal(engine, query).await,
        ReasoningLevel::Low => reason_low(engine, query).await,
        ReasoningLevel::Medium | ReasoningLevel::High | ReasoningLevel::Max => {
            reason_deep(engine, query).await
        }
    }
}

async fn reason_minimal(
    engine: &ReasoningEngine,
    query: ReasoningQuery,
) -> Result<ReasoningResult, ReasoningError> {
    let cap = query.max_facts.unwrap_or(10);
    let facts = retrieve_facts(&engine.memory, &query.query, ReasoningLevel::Minimal, cap).await?;
    if facts.is_empty() {
        return Ok(first_turn_result(ReasoningLevel::Minimal));
    }
    let answer = render_facts_answer(&facts);
    let cost = cost_estimate(ReasoningLevel::Minimal, 0, 0);
    Ok(ReasoningResult {
        answer,
        confidence: 0.4 + 0.02 * (facts.len().min(10) as f32),
        level: ReasoningLevel::Minimal,
        caveats: Vec::new(),
        estimated_cost_usd: cost,
        supporting_facts: facts,
    })
}

async fn reason_low(
    engine: &ReasoningEngine,
    query: ReasoningQuery,
) -> Result<ReasoningResult, ReasoningError> {
    let cap = query.max_facts.unwrap_or(5);
    let facts = retrieve_facts(&engine.memory, &query.query, ReasoningLevel::Low, cap).await?;
    let Some(ref llm) = engine.llm else {
        return Err(ReasoningError::Llm(
            "no LLM configured — Low+ levels require ReasoningEngine::with_llm".into(),
        ));
    };
    let answer = llm
        .synthesize(&query.query, &facts, ReasoningLevel::Low)
        .await?;
    let in_tokens = coarse_tokens(&query.query) + facts.iter().map(|f| coarse_tokens(&f.content)).sum::<u64>();
    let out_tokens = coarse_tokens(&answer);
    if facts.is_empty() {
        // We still ran the LLM (per design § 2.4: Low always synthesizes
        // even if retrieval returned nothing — the model can return a
        // best-effort speculative answer) but we tag the result as a
        // first-turn-style low-confidence answer.
        let mut r = first_turn_result(ReasoningLevel::Low);
        r.answer = answer;
        r.estimated_cost_usd = cost_estimate(ReasoningLevel::Low, in_tokens, out_tokens);
        return Ok(r);
    }
    Ok(ReasoningResult {
        answer,
        confidence: 0.5 + 0.05 * (facts.len().min(10) as f32),
        level: ReasoningLevel::Low,
        caveats: Vec::new(),
        estimated_cost_usd: cost_estimate(ReasoningLevel::Low, in_tokens, out_tokens),
        supporting_facts: facts,
    })
}

async fn reason_deep(
    engine: &ReasoningEngine,
    query: ReasoningQuery,
) -> Result<ReasoningResult, ReasoningError> {
    let level = query.level;
    let cap = query.max_facts.unwrap_or(match level {
        ReasoningLevel::Medium => 10,
        ReasoningLevel::High => 20,
        ReasoningLevel::Max => 30,
        _ => 20,
    });
    let facts = retrieve_facts(&engine.memory, &query.query, level, cap).await?;
    let Some(ref llm) = engine.llm else {
        return Err(ReasoningError::Llm(
            "no LLM configured — Medium/High/Max require ReasoningEngine::with_llm".into(),
        ));
    };
    let answer = llm.synthesize(&query.query, &facts, level).await?;
    let in_tokens =
        coarse_tokens(&query.query) + facts.iter().map(|f| coarse_tokens(&f.content)).sum::<u64>();
    let out_tokens = coarse_tokens(&answer);
    if facts.is_empty() {
        let mut r = first_turn_result(level);
        r.answer = answer;
        r.estimated_cost_usd = cost_estimate(level, in_tokens, out_tokens);
        return Ok(r);
    }
    let base = match level {
        ReasoningLevel::Medium => 0.7_f32,
        ReasoningLevel::High => 0.8_f32,
        ReasoningLevel::Max => 0.85_f32,
        _ => 0.7,
    };
    Ok(ReasoningResult {
        answer,
        confidence: base + 0.02 * (facts.len().min(10) as f32),
        level,
        caveats: Vec::new(),
        estimated_cost_usd: cost_estimate(level, in_tokens, out_tokens),
        supporting_facts: facts,
    })
}

/// First-turn / no-facts result template. `answer` and
/// `estimated_cost_usd` are filled by the caller if the LLM was still
/// invoked (Low+) or stay at the defaults (Minimal).
fn first_turn_result(level: ReasoningLevel) -> ReasoningResult {
    ReasoningResult {
        answer: "No facts found.".to_string(),
        supporting_facts: Vec::new(),
        confidence: 0.0,
        level,
        caveats: vec![FIRST_TURN_CAVEAT.to_string()],
        estimated_cost_usd: 0.0,
    }
}

fn render_facts_answer(facts: &[FactReference]) -> String {
    if facts.is_empty() {
        return "No facts found.".to_string();
    }
    let mut s = format!("Found {} fact(s):\n", facts.len());
    for f in facts {
        s.push_str("- ");
        s.push_str(&f.content);
        s.push('\n');
    }
    s
}

/// Coarse token estimator: ~4 chars per token. Plan 01-12's
/// `BudgetTracker` records real numbers; this is only used to populate
/// the `estimated_cost_usd` field on the `ReasoningResult` so the agent
/// sees a non-zero per-query cost.
fn coarse_tokens(s: &str) -> u64 {
    (s.chars().count() as u64).div_ceil(4)
}

/// Per-level USD-per-1k-token rough estimate. Real per-provider pricing
/// is plan 01-13's job; this is the conservative ceiling.
fn cost_estimate(level: ReasoningLevel, in_tokens: u64, out_tokens: u64) -> f64 {
    let (in_per_1k, out_per_1k) = match level {
        ReasoningLevel::Minimal => (0.0, 0.0),
        ReasoningLevel::Low => (0.001, 0.002),
        ReasoningLevel::Medium => (0.003, 0.015),
        ReasoningLevel::High => (0.005, 0.025),
        ReasoningLevel::Max => (0.015, 0.075),
    };
    (in_tokens as f64) / 1000.0 * in_per_1k + (out_tokens as f64) / 1000.0 * out_per_1k
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FactReference, ReasoningLlm, ReasoningQuery};
    use openfang_memory::MemorySubstrate;
    use openfang_types::agent::AgentId;
    use openfang_types::memory::{Memory, MemorySource};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct MockLlm {
        calls: AtomicU32,
    }

    #[async_trait::async_trait]
    impl ReasoningLlm for MockLlm {
        async fn synthesize(
            &self,
            _query: &str,
            facts: &[FactReference],
            level: ReasoningLevel,
        ) -> Result<String, ReasoningError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(format!(
                "mock-synthesis level={:?} facts={}",
                level,
                facts.len()
            ))
        }
    }

    fn fresh_memory() -> Arc<MemorySubstrate> {
        Arc::new(MemorySubstrate::open_in_memory(0.0).expect("in-memory open"))
    }

    fn make_engine(with_llm: bool) -> (ReasoningEngine, Arc<MockLlm>) {
        let mem = fresh_memory();
        let llm = Arc::new(MockLlm::default());
        let mut eng = ReasoningEngine::new(mem);
        if with_llm {
            eng = eng.with_llm(llm.clone());
        }
        (eng, llm)
    }

    async fn seed_memory_fragment(memory: &Arc<MemorySubstrate>, content: &str) {
        memory
            .remember(
                AgentId::new(),
                content,
                MemorySource::UserProvided,
                "episodic",
                HashMap::new(),
            )
            .await
            .expect("remember");
    }

    async fn seed_kv(memory: &Arc<MemorySubstrate>, key: &str, val: &str) {
        memory
            .set(
                AgentId::new(),
                key,
                serde_json::Value::String(val.to_string()),
            )
            .await
            .expect("kv set");
    }

    #[tokio::test]
    async fn reason_minimal_returns_facts_without_calling_llm() {
        let (engine, llm) = make_engine(true);
        // Populate KV with a key the query mentions explicitly.
        seed_kv(&engine.memory, "rust", "love it").await;
        let q = ReasoningQuery {
            query: "rust".to_string(),
            level: ReasoningLevel::Minimal,
            agent_id: None,
            max_facts: None,
        };
        let r = reason_impl(&engine, q).await.expect("reason ok");
        assert!(
            !r.supporting_facts.is_empty(),
            "Minimal should have returned at least the seeded KV fact"
        );
        assert_eq!(
            llm.calls.load(Ordering::SeqCst),
            0,
            "Minimal must NEVER call the LLM"
        );
        assert_eq!(r.level, ReasoningLevel::Minimal);
        // Minimal cost is 0 (no model call).
        assert!((r.estimated_cost_usd - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn reason_low_calls_llm_once() {
        let (engine, llm) = make_engine(true);
        // Seed semantic so retrieve_facts(Low) finds something.
        seed_memory_fragment(&engine.memory, "the user enjoys rust programming").await;
        let q = ReasoningQuery {
            query: "rust".to_string(),
            level: ReasoningLevel::Low,
            agent_id: None,
            max_facts: None,
        };
        let r = reason_impl(&engine, q).await.expect("reason ok");
        assert_eq!(llm.calls.load(Ordering::SeqCst), 1, "Low calls LLM once");
        assert_eq!(r.level, ReasoningLevel::Low);
        assert!(r.answer.contains("mock-synthesis"));
    }

    #[tokio::test]
    async fn reason_medium_calls_llm_with_multi_source_context() {
        let (engine, llm) = make_engine(true);
        // Two seeds: one semantic, one KV. Medium retrieves both.
        seed_memory_fragment(&engine.memory, "rust loved by user").await;
        seed_kv(&engine.memory, "rust", "favourite").await;
        let q = ReasoningQuery {
            query: "rust".to_string(),
            level: ReasoningLevel::Medium,
            agent_id: None,
            max_facts: None,
        };
        let r = reason_impl(&engine, q).await.expect("reason ok");
        assert_eq!(llm.calls.load(Ordering::SeqCst), 1);
        assert!(
            r.supporting_facts.len() >= 2,
            "Medium should aggregate from ≥2 sources, got {}",
            r.supporting_facts.len()
        );
    }

    #[tokio::test]
    async fn reason_first_turn_returns_no_history_caveat() {
        let (engine, _) = make_engine(true);
        let q = ReasoningQuery {
            query: "anything".to_string(),
            level: ReasoningLevel::Medium,
            agent_id: None,
            max_facts: None,
        };
        let r = reason_impl(&engine, q).await.expect("reason ok");
        assert!((r.confidence - 0.0).abs() < 1e-9, "first-turn confidence is 0.0");
        assert_eq!(r.caveats.len(), 1);
        assert_eq!(r.caveats[0], FIRST_TURN_CAVEAT);
        assert!(r.supporting_facts.is_empty());
    }

    #[tokio::test]
    async fn reason_low_without_llm_returns_llm_error() {
        let (engine, _) = make_engine(false);
        let q = ReasoningQuery {
            query: "anything".to_string(),
            level: ReasoningLevel::Low,
            agent_id: None,
            max_facts: None,
        };
        let err = reason_impl(&engine, q).await.unwrap_err();
        assert!(
            matches!(err, ReasoningError::Llm(_)),
            "expected ReasoningError::Llm, got {err:?}"
        );
    }

    #[tokio::test]
    async fn reason_high_without_llm_also_errors() {
        let (engine, _) = make_engine(false);
        let q = ReasoningQuery {
            query: "anything".to_string(),
            level: ReasoningLevel::High,
            agent_id: None,
            max_facts: None,
        };
        let err = reason_impl(&engine, q).await.unwrap_err();
        assert!(matches!(err, ReasoningError::Llm(_)));
    }

    #[test]
    fn first_turn_caveat_is_verbatim() {
        // Pin the verbatim wording — the dashboard and agent surface this
        // string to the user. Any change to it MUST be intentional.
        assert_eq!(
            FIRST_TURN_CAVEAT,
            "No conversation history available — answers are speculative."
        );
    }

    #[test]
    fn cost_estimate_minimal_is_zero() {
        assert!((cost_estimate(ReasoningLevel::Minimal, 1_000_000, 1_000_000) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn cost_estimate_max_is_highest() {
        let in_t = 1000;
        let out_t = 1000;
        let low = cost_estimate(ReasoningLevel::Low, in_t, out_t);
        let med = cost_estimate(ReasoningLevel::Medium, in_t, out_t);
        let high = cost_estimate(ReasoningLevel::High, in_t, out_t);
        let max = cost_estimate(ReasoningLevel::Max, in_t, out_t);
        assert!(low < med);
        assert!(med < high);
        assert!(high < max);
    }
}
