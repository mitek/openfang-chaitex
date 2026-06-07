//! Error type for the reasoning crate.

use crate::ReasoningLevel;
use thiserror::Error;

/// Errors returned by the reasoning engine.
///
/// Each variant carries enough structured data for the agent-facing tool
/// (`memory_reason`) to render an actionable error message — e.g. how to
/// approve a `Max`-level call, what the budget limit was, what subsystem
/// failed.
#[derive(Debug, Error)]
pub enum ReasoningError {
    /// The requested level exceeds `max_level` from the `[reasoning]` config.
    #[error(
        "Reasoning level {requested:?} not allowed (max_level={max_allowed:?})"
    )]
    LevelNotAllowed {
        /// The level the caller asked for.
        requested: ReasoningLevel,
        /// The configured ceiling.
        max_allowed: ReasoningLevel,
    },

    /// Caller asked for `Max` and `require_approval_for_max=true`. The
    /// agent loop is expected to surface `query_preview` and
    /// `estimated_cost_usd` to the user, then retry with an approval flag
    /// once approved.
    #[error(
        "Approval required for level {level} (estimated_cost_usd={estimated_cost_usd:.4})"
    )]
    ApprovalRequired {
        /// String form of the level (so log/JSON serialization is stable).
        level: String,
        /// Pre-call cost estimate in USD.
        estimated_cost_usd: f64,
        /// First ≤100 chars of the query so the user knows what they're
        /// approving without leaking long prompts to logs.
        query_preview: String,
    },

    /// Monthly reasoning budget exhausted.
    ///
    /// In `budget_exceeded_action="block"` mode this is returned by
    /// `ReasoningEngine::reason`. In `"warn"` mode the engine downgrades to
    /// `Low` and the warning is logged, not returned.
    #[error("Monthly reasoning budget exceeded: spent ${spent:.2} of ${limit:.2}")]
    BudgetExceeded {
        /// USD already spent this billing window.
        spent: f64,
        /// USD ceiling from `monthly_budget_usd`.
        limit: f64,
    },

    /// Wraps `MemorySubstrate` failures during fact retrieval.
    #[error("Memory access error: {0}")]
    Memory(String),

    /// Wraps LLM synthesis failures bubbled up from `ReasoningLlm`.
    #[error("LLM synthesis error: {0}")]
    Llm(String),

    /// Sentinel returned by stubbed methods until plan 01-11 fills the body.
    #[error("Reasoning subsystem not yet wired: {0}")]
    NotYetImplemented(String),
}
