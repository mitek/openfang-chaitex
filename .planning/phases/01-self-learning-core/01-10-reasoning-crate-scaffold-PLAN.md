---
phase: 01-self-learning-core
plan: 10
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
files_created:
  - crates/openfang-reasoning/Cargo.toml
  - crates/openfang-reasoning/src/lib.rs
  - crates/openfang-reasoning/src/error.rs
autonomous: true
must_haves:
  truths:
    - "`cargo build -p openfang-reasoning` succeeds (MR-01)"
    - "Crate is a workspace member: `cargo metadata` lists it under workspace_members (MR-01)"
    - "Public surface exports `ReasoningEngine`, `ReasoningLevel`, `ReasoningQuery`, `ReasoningResult`, `ReasoningError`, `ReasoningLlm` — symbols resolvable from another crate via `use openfang_reasoning::*;`"
    - "Dependencies limited to the MR-01 list: openfang-types, openfang-memory, tokio, serde, serde_json, tracing, async-trait, chrono. No HTTP clients, no provider SDKs (MR-01, MR-03)"
  artifacts:
    - "crates/openfang-reasoning/Cargo.toml"
    - "crates/openfang-reasoning/src/lib.rs"
    - "crates/openfang-reasoning/src/error.rs with `pub enum ReasoningError` per design § 2.3"
  key_links:
    - "Plans 01-11, 01-12 implement the bodies — this plan ships the empty types + module skeleton with everything `unimplemented!() -> ReasoningError::NotYetImplemented` stubs are NOT acceptable; types must be real but `reason()` may return `ReasoningError::NotYetImplemented` until 01-11"
---

<objective>
Stand up the empty `openfang-reasoning` crate with the public type surface, the error enum, and the `ReasoningLlm` trait — but no engine logic. Plans 01-11 and 01-12 fill in `engine.rs`, `fact_retrieval.rs`, and `budget.rs` against this stable scaffold.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@Cargo.toml
@crates/openfang-memory/src/lib.rs
@crates/openfang-types/src/agent.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Create crate dir + Cargo.toml + register as workspace member</name>
  <files>Cargo.toml, crates/openfang-reasoning/Cargo.toml</files>
  <action>
Create `crates/openfang-reasoning/Cargo.toml`:
```
[package]
name = "openfang-reasoning"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
openfang-types  = { path = "../openfang-types" }
openfang-memory = { path = "../openfang-memory" }
tokio       = { workspace = true }
serde       = { workspace = true, features = ["derive"] }
serde_json  = { workspace = true }
tracing     = { workspace = true }
async-trait = { workspace = true }
chrono      = { workspace = true }

[dev-dependencies]
tokio-test = { workspace = true }
tempfile   = { workspace = true }
```
NOTE: design § 2.1 example says `edition = "2024"` — use `edition.workspace = true` to inherit `2021` (MR-01 mandates Rust 2021 / MSRV 1.75; addendum/REQUIREMENTS overrides design). In the root `Cargo.toml`, add `"crates/openfang-reasoning"` to the `members = [...]` array (alphabetical position next to openfang-runtime). Verify `workspace.dependencies` already covers chrono / async-trait — if not, add them workspace-wide (search for `chrono` and `async-trait` first to confirm).
  </action>
  <verify>
`cargo build -p openfang-reasoning` succeeds (empty crate compiles as `cargo new --lib` would).
`cargo metadata --format-version=1 | grep openfang-reasoning` returns a member entry.
  </verify>
  <done>
Crate exists, workspace recognizes it, MSRV/edition inherited.
  </done>
</task>

<task type="auto">
  <name>Task 2: Public type surface (lib.rs + error.rs)</name>
  <files>crates/openfang-reasoning/src/lib.rs, crates/openfang-reasoning/src/error.rs</files>
  <action>
Create `error.rs`:
```rust
//! Error type for the reasoning crate.
use thiserror::Error;
use crate::ReasoningLevel;

#[derive(Debug, Error)]
pub enum ReasoningError {
    #[error("Reasoning level {requested:?} not allowed (max_level={max_allowed:?})")]
    LevelNotAllowed { requested: ReasoningLevel, max_allowed: ReasoningLevel },
    #[error("Approval required for level {level} (estimated_cost_usd={estimated_cost_usd:.4})")]
    ApprovalRequired { level: String, estimated_cost_usd: f64, query_preview: String },
    #[error("Monthly reasoning budget exceeded: spent ${spent:.2} of ${limit:.2}")]
    BudgetExceeded { spent: f64, limit: f64 },
    #[error("Memory access error: {0}")]
    Memory(String),
    #[error("LLM synthesis error: {0}")]
    Llm(String),
    #[error("Reasoning subsystem not yet wired: {0}")]
    NotYetImplemented(String),
}
```
Add `thiserror` to deps (it's already workspace-wide). Create `lib.rs`:
```rust
//! Memory reasoning subsystem.
//!
//! Implementations land in plans 01-11 (engine + levels) and 01-12 (budget).
//! This crate currently exposes only the public type surface.

pub mod error;

pub use error::ReasoningError;

use serde::{Deserialize, Serialize};
use openfang_types::AgentId;
use std::sync::Arc;

/// Depth of reasoning. Ordered: Minimal < Low < Medium < High < Max.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningLevel { Minimal, Low, Medium, High, Max }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningQuery {
    pub query: String,
    pub level: ReasoningLevel,
    pub agent_id: Option<AgentId>,
    pub max_facts: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningResult {
    pub answer: String,
    pub supporting_facts: Vec<FactReference>,
    pub confidence: f32,
    pub level: ReasoningLevel,
    pub caveats: Vec<String>,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactReference {
    pub source: FactSource,
    pub content: String,
    pub relevance: f32,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FactSource {
    Memory { memory_id: String },
    Session { session_id: String, message_index: usize },
    KnowledgeGraph { entity_id: String },
    StructuredKv { key: String },
}

#[async_trait::async_trait]
pub trait ReasoningLlm: Send + Sync {
    async fn synthesize(
        &self,
        query: &str,
        facts: &[FactReference],
        level: ReasoningLevel,
    ) -> Result<String, ReasoningError>;
}

/// The reasoning engine. Body is filled in by plan 01-11.
pub struct ReasoningEngine {
    pub(crate) memory: Arc<openfang_memory::substrate::MemorySubstrate>,
    pub(crate) llm: Option<Arc<dyn ReasoningLlm>>,
}

impl ReasoningEngine {
    pub fn new(memory: Arc<openfang_memory::substrate::MemorySubstrate>) -> Self {
        Self { memory, llm: None }
    }
    pub fn with_llm(mut self, llm: Arc<dyn ReasoningLlm>) -> Self {
        self.llm = Some(llm); self
    }
    pub async fn reason(&self, _query: ReasoningQuery) -> Result<ReasoningResult, ReasoningError> {
        Err(ReasoningError::NotYetImplemented("ReasoningEngine::reason — see plan 01-11".into()))
    }
}
```
Verify the path `openfang_memory::substrate::MemorySubstrate` matches the actual export. Adjust if the substrate type is at a different module path.
  </action>
  <verify>
`cargo build -p openfang-reasoning` clean.
`cargo clippy -p openfang-reasoning --all-targets -- -D warnings` clean.
A scratch consumer (`cargo check -p openfang-runtime`) doesn't break — this crate is not yet a dependency anywhere.
  </verify>
  <done>
All public types compile; `reason()` returns a typed error stub; no warnings.
  </done>
</task>

<task type="auto">
  <name>Task 3: Smoke test the empty crate</name>
  <files>crates/openfang-reasoning/src/lib.rs</files>
  <action>
Add `#[cfg(test)] mod tests` with:
- `level_ordering_holds` — `assert!(ReasoningLevel::Minimal < ReasoningLevel::Max)` etc.
- `query_round_trip_json` — serde JSON round-trip for ReasoningQuery.
- `engine_reason_returns_not_yet_implemented` — `MemorySubstrate::open_in_memory(0.0)?`, build engine, call reason on Minimal, expect `Err(ReasoningError::NotYetImplemented(_))`.
  </action>
  <verify>
`cargo test -p openfang-reasoning` — 3 tests pass.
  </verify>
  <done>
Smoke tests green.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (3 new tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `grep -r 'openfang-reasoning' Cargo.toml` shows it as a workspace member.
</verification>

<success_criteria>
- [ ] Crate exists and builds.
- [ ] Workspace member registered.
- [ ] Public types resolvable from another crate.
- [ ] Deps limited to MR-01 list.
- [ ] `reason()` returns `NotYetImplemented` (filled by 01-11).
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-10-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files created (final list)
- Tests added (count + brief)
- Any path corrections to MemorySubstrate import that this plan had to make
- Any follow-ups for later plans
</output>
