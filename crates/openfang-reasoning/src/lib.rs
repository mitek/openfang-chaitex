//! Memory reasoning subsystem.
//!
//! Implementations land in plan 01-11 (engine + levels) and plan 01-12
//! (budget). This crate currently exposes only the public type surface, the
//! `ReasoningLlm` integration seam, and a stubbed `ReasoningEngine::reason`
//! that returns `NotYetImplemented` so downstream plans can wire the agent
//! loop / tool registration against a stable API.
//!
//! Architecture invariants (REQ MR-01 / MR-03):
//! - Depends only on `openfang-types`, `openfang-memory`, `tokio`, `serde`,
//!   `serde_json`, `tracing`, `async-trait`, `chrono`, `thiserror`. No HTTP
//!   clients, no provider SDKs — LLM calls go through `ReasoningLlm` and
//!   `KernelHandle` in the integration plan (01-13).
//! - Reasoning levels are totally ordered (`Minimal < Low < Medium < High <
//!   Max`); ordering is preserved by `derive(PartialOrd, Ord)`.

pub mod budget;
pub mod engine;
pub mod error;
pub mod fact_retrieval;

pub use budget::{
    format_effective_log, log_effective_reasoning_config, BudgetRecord, BudgetTracker,
};
pub use engine::FIRST_TURN_CAVEAT;
pub use error::ReasoningError;
pub use fact_retrieval::retrieve_facts;

use openfang_types::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Depth of reasoning. Ordered: `Minimal < Low < Medium < High < Max`.
///
/// The order matters: budget checks compare `requested <= max_allowed` and
/// downgrade-to-`Low` is implemented as `requested.min(ReasoningLevel::Low)`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningLevel {
    /// Mechanical lookup only — no synthesis.
    Minimal,
    /// Single FTS5 search; surface raw hits.
    Low,
    /// Synthesize across a small fact set; bounded cost.
    Medium,
    /// Multi-hop synthesis with the full memory substrate.
    High,
    /// Deep reasoning; requires approval by default
    /// (`require_approval_for_max=true`).
    Max,
}

/// Input to a reasoning call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningQuery {
    /// The agent-facing question.
    pub query: String,
    /// Requested depth. Subject to `max_level` clamping by the engine.
    pub level: ReasoningLevel,
    /// Scope to a single agent's history; `None` means cross-agent allowed
    /// (gated by capabilities at the tool layer, not here).
    pub agent_id: Option<AgentId>,
    /// Cap on supporting facts surfaced; `None` uses the level's default.
    pub max_facts: Option<usize>,
}

/// Result of a reasoning call. The shape matches the agent-facing
/// `memory_reason` tool result (REQ MR-04).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningResult {
    /// Synthesized answer text.
    pub answer: String,
    /// Facts that supported the answer, in relevance order.
    pub supporting_facts: Vec<FactReference>,
    /// Confidence ∈ [0.0, 1.0]. `Minimal`/`Low` are lookup-only; their
    /// confidence is the raw FTS5/structured-store relevance.
    pub confidence: f32,
    /// The level actually used (after `max_level` clamping).
    pub level: ReasoningLevel,
    /// Caveats the agent should surface — e.g. "first turn, no history",
    /// "budget exceeded, downgraded to Low", "low confidence".
    pub caveats: Vec<String>,
    /// USD cost of this call (REQ MR-07). Included so the agent and dashboard
    /// see per-query cost.
    pub estimated_cost_usd: f64,
}

/// Pointer to a single fact used in synthesis. The variant tells the caller
/// where to retrieve the raw payload if they want to drill in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactReference {
    /// Origin of the fact.
    pub source: FactSource,
    /// Short excerpt — limited so a long facts list doesn't blow up the
    /// agent's context window.
    pub content: String,
    /// Relevance score from the retrieval layer. `[0.0, 1.0]`-ish; not
    /// strictly normalized across sources.
    pub relevance: f32,
    /// Optional timestamp of the fact (RFC3339), if the source carries one.
    pub timestamp: Option<String>,
}

/// Where a `FactReference` came from. Tag is serialized as a `type` field so
/// the JSON shape is stable across language clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FactSource {
    /// A `MemoryFragment` from the semantic store.
    Memory {
        /// `MemoryId`'s string form.
        memory_id: String,
    },
    /// A message inside a `Session`.
    Session {
        /// `SessionId`'s string form.
        session_id: String,
        /// Zero-based index of the message in the session.
        message_index: usize,
    },
    /// An entity from the knowledge graph.
    KnowledgeGraph {
        /// Entity ID as stored.
        entity_id: String,
    },
    /// A key in the structured KV store.
    StructuredKv {
        /// Storage key.
        key: String,
    },
}

/// Integration seam to the host LLM driver.
///
/// `ReasoningEngine` does NOT spawn its own HTTP client (REQ MR-03). Instead,
/// the agent loop wires a `ReasoningLlm` whose implementation forwards to the
/// agent's configured `LlmDriver` via `KernelHandle`. Plan 01-13 wires this
/// in `openfang-runtime`.
#[async_trait::async_trait]
pub trait ReasoningLlm: Send + Sync {
    /// Synthesize a final answer from a query + retrieved fact set at a
    /// given depth. The implementation chooses model/temperature based on
    /// `level`.
    async fn synthesize(
        &self,
        query: &str,
        facts: &[FactReference],
        level: ReasoningLevel,
    ) -> Result<String, ReasoningError>;
}

/// The reasoning engine. Body filled in by plan 01-11 (level dispatch) and
/// plan 01-12 (budget tracking). Until then `reason()` returns a typed
/// `NotYetImplemented` error so callers fail loudly instead of silently
/// returning empty results.
pub struct ReasoningEngine {
    /// Memory access for fact retrieval. Held as `Arc` because the engine
    /// lives behind an `Arc` on the kernel side and the substrate is shared
    /// across the whole kernel. Plan 01-11 reads this in `engine::reason_impl`.
    pub(crate) memory: Arc<openfang_memory::MemorySubstrate>,
    /// Optional LLM seam. `None` is allowed so `Minimal`/`Low` lookups
    /// (which need no synthesis) work without an LLM hooked up — useful in
    /// tests and during early boot.
    ///
    /// `has_llm()` reads this; field stays `pub(crate)` so plan 01-11 can
    /// `Option::as_deref` or `.expect("...")` in the dispatch body.
    pub(crate) llm: Option<Arc<dyn ReasoningLlm>>,
}

impl ReasoningEngine {
    /// Construct an engine with the shared memory substrate and no LLM
    /// attached. Use [`Self::with_llm`] to attach one.
    pub fn new(memory: Arc<openfang_memory::MemorySubstrate>) -> Self {
        Self { memory, llm: None }
    }

    /// Builder-style: attach an LLM seam. Required for `Medium+`; ignored
    /// for `Minimal`/`Low`.
    pub fn with_llm(mut self, llm: Arc<dyn ReasoningLlm>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// Whether an LLM is currently attached. Tests assert this without
    /// reaching into private fields.
    pub fn has_llm(&self) -> bool {
        self.llm.is_some()
    }

    /// Run a reasoning call.
    ///
    /// Dispatches by `query.level` per design § 2.4. Plan 01-11 lands the
    /// body in `engine::reason_impl`; this method is a thin delegation
    /// shell so the trait-object surface (and any future budget /
    /// approval clamps in plan 01-12 / 01-13) can sit here without
    /// requiring callers to import a separate module.
    pub async fn reason(
        &self,
        query: ReasoningQuery,
    ) -> Result<ReasoningResult, ReasoningError> {
        crate::engine::reason_impl(self, query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::config::MemoryConfig;

    fn fresh_memory() -> Arc<openfang_memory::MemorySubstrate> {
        // The open_in_memory constructor is the documented test path
        // (TESTING.md). Decay rate 0.0 means no time-based decay during
        // tests so results are deterministic.
        let mem = openfang_memory::MemorySubstrate::open_in_memory(0.0)
            .expect("in-memory MemorySubstrate must open");
        Arc::new(mem)
    }

    #[test]
    fn level_ordering_holds() {
        // Ord/PartialOrd are derived; verify the order is the one users
        // expect (smaller = cheaper / shallower).
        assert!(ReasoningLevel::Minimal < ReasoningLevel::Low);
        assert!(ReasoningLevel::Low < ReasoningLevel::Medium);
        assert!(ReasoningLevel::Medium < ReasoningLevel::High);
        assert!(ReasoningLevel::High < ReasoningLevel::Max);
        // Downgrade idiom: clamp at Low.
        let downgraded = ReasoningLevel::High.min(ReasoningLevel::Low);
        assert_eq!(downgraded, ReasoningLevel::Low);
    }

    #[test]
    fn level_serializes_lowercase() {
        // The tool layer (plan 01-13) and the dashboard rely on lowercase
        // JSON strings — pin the wire format here.
        let s = serde_json::to_string(&ReasoningLevel::Medium).unwrap();
        assert_eq!(s, "\"medium\"");
        let back: ReasoningLevel = serde_json::from_str("\"max\"").unwrap();
        assert_eq!(back, ReasoningLevel::Max);
    }

    #[test]
    fn query_round_trip_json() {
        let q = ReasoningQuery {
            query: "What did the user say about Rust?".to_string(),
            level: ReasoningLevel::Medium,
            agent_id: None,
            max_facts: Some(8),
        };
        let s = serde_json::to_string(&q).expect("serialize");
        let back: ReasoningQuery = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back.query, q.query);
        assert_eq!(back.level, q.level);
        assert_eq!(back.max_facts, q.max_facts);
    }

    #[test]
    fn fact_source_tag_is_type_field() {
        // Pin the JSON wire format so the tool-layer schema (plan 01-13)
        // can advertise the same shape to LLM tool schemas.
        let f = FactReference {
            source: FactSource::Session {
                session_id: "00000000-0000-0000-0000-000000000001".to_string(),
                message_index: 3,
            },
            content: "I love Rust".to_string(),
            relevance: 0.8,
            timestamp: None,
        };
        let v = serde_json::to_value(&f).unwrap();
        assert_eq!(v["source"]["type"], "Session");
        assert_eq!(v["source"]["session_id"], "00000000-0000-0000-0000-000000000001");
        assert_eq!(v["source"]["message_index"], 3);
    }

    #[tokio::test]
    async fn engine_reason_minimal_smoke_test() {
        // Plan 01-11 replaced the NotYetImplemented stub with the real
        // five-level dispatch. Minimal on an empty memory should still
        // return Ok (no LLM call required) with the first-turn caveat.
        let _ = MemoryConfig::default(); // touch the import path
        let engine = ReasoningEngine::new(fresh_memory());
        assert!(!engine.has_llm(), "fresh engine should not have an LLM");
        let q = ReasoningQuery {
            query: "smoke test".to_string(),
            level: ReasoningLevel::Minimal,
            agent_id: None,
            max_facts: None,
        };
        let r = engine.reason(q).await.expect("Minimal on empty memory must Ok");
        assert_eq!(r.level, ReasoningLevel::Minimal);
        // Empty memory ⇒ no facts ⇒ first-turn caveat surfaces.
        assert!(r.supporting_facts.is_empty());
        assert!((r.confidence - 0.0).abs() < 1e-9);
        assert_eq!(r.caveats.len(), 1);
        assert_eq!(r.caveats[0], crate::FIRST_TURN_CAVEAT);
    }

    #[test]
    fn level_not_allowed_error_format() {
        // Pin the Display string so log scrapers/operators can rely on it.
        let e = ReasoningError::LevelNotAllowed {
            requested: ReasoningLevel::Max,
            max_allowed: ReasoningLevel::High,
        };
        let s = e.to_string();
        assert!(s.contains("Max"), "got: {}", s);
        assert!(s.contains("High"), "got: {}", s);
    }

    #[test]
    fn approval_required_error_format() {
        let e = ReasoningError::ApprovalRequired {
            level: "max".to_string(),
            estimated_cost_usd: 0.1234,
            query_preview: "...".to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("Approval required"), "got: {}", s);
        assert!(s.contains("0.1234"), "got: {}", s);
    }
}
