---
phase: 01-self-learning-core
plan: 11
type: execute
wave: 2
depends_on: [01-10]
files_modified: []
files_created:
  - crates/openfang-reasoning/src/engine.rs
  - crates/openfang-reasoning/src/fact_retrieval.rs
autonomous: true
must_haves:
  truths:
    - "`ReasoningEngine::reason(query)` dispatches by `query.level` per design § 2.4 (MR-02)"
    - "Minimal level executes structured-KV + knowledge-graph lookup and returns facts WITHOUT calling the LLM (MR-02)"
    - "Low level retrieves semantic + FTS5 top-5 and calls `ReasoningLlm::synthesize` once (MR-02)"
    - "Medium/High pull from semantic + FTS5 + graph + KV, cross-reference, synthesize via LLM (MR-02)"
    - "First-turn use (no history): returns `caveats: [\"No conversation history available — answers are speculative.\"]`, `confidence=0.0`, `supporting_facts: []` (skeleton open-decision 4)"
  artifacts:
    - "`crates/openfang-reasoning/src/engine.rs` containing the real body of `ReasoningEngine::reason`"
    - "`crates/openfang-reasoning/src/fact_retrieval.rs` with `fn retrieve_facts(memory, query, level, limit) -> Vec<FactReference>`"
---

<objective>
Land the five-level reasoning dispatch and the multi-source fact retrieval used by Medium+ queries. Uses a mock `ReasoningLlm` in tests; the real `KernelLlmAdapter` integration is plan 01-13's job.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-reasoning/src/lib.rs
@crates/openfang-memory/src/substrate.rs
@crates/openfang-memory/src/session.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: fact_retrieval module</name>
  <files>crates/openfang-reasoning/src/fact_retrieval.rs</files>
  <action>
Create `fact_retrieval.rs`. Public:
```rust
pub async fn retrieve_facts(
    memory: &MemorySubstrate,
    query: &str,
    level: ReasoningLevel,
    max_facts: usize,
) -> Result<Vec<FactReference>, ReasoningError>;
```
Per design § 2.4 table:
- `Minimal`: structured KV exact-match on tokens of `query` → KnowledgeGraph entity lookup → semantic search fallback (cosine). No FTS5, no LLM.
- `Low`: semantic search (top-5) + FTS5 session search (top-5 via the helper from plan 01-04, `SessionStore::search_sessions_fts(query, 5, None)`). Each hit becomes a `FactReference { source: FactSource::Session{..}, content: snippet, relevance: 1.0/(1.0+score), timestamp: Some(ts) }`.
- `Medium`/`High`/`Max`: same as Low PLUS knowledge graph related-entity walk + structured KV scan. Combined output capped at `max_facts` (default 20 for High, 10 for Medium).
Resolve every memory call against the actual `MemorySubstrate` surface in `crates/openfang-memory/src/substrate.rs` — pick the method names that exist (e.g. `semantic_search`, `graph.related`, `kv.get`). Wrap memory errors as `ReasoningError::Memory(e.to_string())`. Locks: hold the substrate's connection mutex briefly, never across `.await` (CONVENTIONS.md and the `!Send` rule from CONTEXT.md). For FTS5 (sync) call from a `spawn_blocking` if the substrate doesn't already give an async wrapper.
  </action>
  <verify>
`cargo build -p openfang-reasoning` clean.
  </verify>
  <done>
fact_retrieval compiles; each level fetches from the documented sources.
  </done>
</task>

<task type="auto">
  <name>Task 2: engine.rs — fill in `reason()`</name>
  <files>crates/openfang-reasoning/src/engine.rs, crates/openfang-reasoning/src/lib.rs</files>
  <action>
Move the `ReasoningEngine` impl to a new `engine.rs` module. In `lib.rs` replace the stub `pub async fn reason` with a delegation to `engine::reason_impl`. Implement dispatch:
```rust
match query.level {
    ReasoningLevel::Minimal => reason_minimal(self, &query).await,
    ReasoningLevel::Low     => reason_low(self, &query).await,
    ReasoningLevel::Medium | ReasoningLevel::High | ReasoningLevel::Max
                            => reason_deep(self, &query).await,
}
```
Bodies:
- `reason_minimal`: facts via `retrieve_facts(.., Minimal, max)`; if empty return `ReasoningResult { answer: "No facts found.".into(), supporting_facts: vec![], confidence: 0.0, level, caveats: vec!["No conversation history available — answers are speculative.".into()], estimated_cost_usd: 0.0 }`; else return facts concatenated into a deterministic answer (`"Found {N} facts: ..."`). NO LLM call.
- `reason_low`: facts via `retrieve_facts(.., Low, 5)`; if no LLM → return `ReasoningError::Llm("no LLM configured".into())`; else call `self.llm.synthesize(&query.query, &facts, Low)`. Confidence = `0.5 + 0.05 * facts.len().min(10) as f32`.
- `reason_deep`: facts via `retrieve_facts(.., level, max_facts.unwrap_or(20))`; cross-ref (dedupe by source key); call `self.llm.synthesize(&query.query, &facts, level)`. Confidence base 0.7 for Medium, 0.8 for High, 0.85 for Max.
First-turn handling (skeleton open-decision 4): if `supporting_facts.is_empty()` return with `confidence=0.0` and the caveat verbatim. `estimated_cost_usd` is computed by a `cost_estimate(level, in_tokens, out_tokens)` free fn (token counts via a coarse `chars/4` heuristic; budget tracker in 01-12 will replace this with the real number).
  </action>
  <verify>
`cargo build -p openfang-reasoning` clean.
`cargo clippy -p openfang-reasoning --all-targets -- -D warnings` clean.
  </verify>
  <done>
Five-level dispatch + first-turn caveat + cost estimate stub.
  </done>
</task>

<task type="auto">
  <name>Task 3: Mock ReasoningLlm + tests per level</name>
  <files>crates/openfang-reasoning/src/engine.rs</files>
  <action>
In `engine.rs` test module:
- Mock `MockLlm { calls: AtomicU32 }` impl `ReasoningLlm` returning a deterministic synthesized string and bumping the counter.
- `fn engine_for_test()` → `MemorySubstrate::open_in_memory(0.0)?`, optional `with_llm(Arc::new(MockLlm::default()))`.
Tests:
- `reason_minimal_returns_facts_without_calling_llm` — populate KV with a fact, call reason(Minimal), assert facts non-empty AND `mock.calls == 0`.
- `reason_low_calls_llm_once` — populate one session via `save_session`, call reason(Low), assert `mock.calls == 1`.
- `reason_medium_calls_llm_with_multi_source_context` — populate KV + session + graph; assert LLM call's `facts.len() >= 2`.
- `reason_first_turn_returns_no_history_caveat` — fresh memory, call reason(Medium), assert `confidence == 0.0` and caveat string matches verbatim.
- `reason_low_without_llm_returns_llm_error` — engine built without `.with_llm`, call reason(Low), assert `Err(ReasoningError::Llm(_))`.
  </action>
  <verify>
`cargo test -p openfang-reasoning engine` runs 5 tests, all green.
`cargo clippy -p openfang-reasoning --all-targets -- -D warnings` clean.
  </verify>
  <done>
≥ 5 tests proving level-by-level behavior; first-turn caveat verified.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (≥ 5 new tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Public surface from plan 01-10 unchanged (no signature breaks).
</verification>

<success_criteria>
- [ ] All five levels dispatch and return a `ReasoningResult` (Minimal/Low/Medium/High deterministically; Max via deep path).
- [ ] Minimal does NOT call LLM; Low/Medium/High DO.
- [ ] First-turn returns the caveat string verbatim with `confidence=0.0`.
- [ ] `retrieve_facts` reads from semantic + FTS5 + graph + KV per level.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-11-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files created (final list)
- Tests added (count + brief)
- Memory accessor method names actually used (record for plan 01-13)
- Any follow-ups for later plans
</output>
