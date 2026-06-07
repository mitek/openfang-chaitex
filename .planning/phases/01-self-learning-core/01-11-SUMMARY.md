# 01-11 — ReasoningEngine level dispatch + fact retrieval — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** e1bab4d, 4d80a73

## One-liner

Replaced the `NotYetImplemented` stub on `ReasoningEngine::reason` with
five-level dispatch (`Minimal/Low/Medium/High/Max`), backed by a new
`fact_retrieval::retrieve_facts` that pulls from semantic recall, FTS5
session search (via the v9 `session_messages_fts` index from plan
01-02), structured KV, and the knowledge graph per design § 2.4.

## Files created

- `crates/openfang-reasoning/src/fact_retrieval.rs` — ~360 lines.
  Exported `retrieve_facts`; private level-specific gatherers
  (`semantic_recall`, `fts5_session_search`, `structured_kv_token_lookup`,
  `knowledge_graph_scan`); `dedupe_and_cap` + `source_key` helpers.
- `crates/openfang-reasoning/src/engine.rs` — ~280 lines.
  Exported `FIRST_TURN_CAVEAT` + `reason_impl`; per-level bodies
  (`reason_minimal`, `reason_low`, `reason_deep`);
  `first_turn_result` / `render_facts_answer` / `coarse_tokens` /
  `cost_estimate` helpers; 9 tests with `MockLlm`.

## Files modified

- `crates/openfang-reasoning/src/lib.rs`:
  - `pub mod engine; pub mod fact_retrieval;` + corresponding re-exports
    (`FIRST_TURN_CAVEAT`, `retrieve_facts`).
  - `ReasoningEngine::reason` body replaced — now `crate::engine::reason_impl(self, query).await`.
  - Dropped `#[allow(dead_code)]` on the `memory` field (now used).
  - Retargeted `engine_reason_returns_not_yet_implemented` to
    `engine_reason_minimal_smoke_test` covering the new Minimal +
    empty-memory + first-turn-caveat path.

## Tests added

9 new tests in `engine::tests`:

1. `reason_minimal_returns_facts_without_calling_llm` — seeds KV with
   key `rust`, calls `Minimal`, asserts facts non-empty AND LLM call
   counter == 0 AND `estimated_cost_usd == 0.0`.
2. `reason_low_calls_llm_once` — seeds semantic memory, calls `Low`,
   asserts LLM call counter == 1 AND result echoes the mock string.
3. `reason_medium_calls_llm_with_multi_source_context` — seeds
   semantic + KV, calls `Medium`, asserts `supporting_facts.len() >= 2`
   (multi-source aggregation).
4. `reason_first_turn_returns_no_history_caveat` — fresh memory + `Medium`,
   asserts `confidence == 0.0`, single caveat equals
   `FIRST_TURN_CAVEAT` verbatim.
5. `reason_low_without_llm_returns_llm_error` — engine built without
   `.with_llm(...)`, calls `Low`, asserts `Err(ReasoningError::Llm(_))`.
6. `reason_high_without_llm_also_errors` — same shape, level=High.
7. `first_turn_caveat_is_verbatim` — pins the caveat string byte-equal.
8. `cost_estimate_minimal_is_zero` — Minimal never costs.
9. `cost_estimate_max_is_highest` — pins `Low < Medium < High < Max`.

Plus 1 retargeted test (`engine_reason_minimal_smoke_test`) in the
lib's existing tests module.

Workspace gates clean: 2774 tests pass (was 2765 → +9 new); build
clean; clippy `-D warnings` clean.

## Memory accessor method names actually used (record for plan 01-13)

From `openfang_types::memory::Memory` (async trait, implemented on
`MemorySubstrate`):

- `recall(query: &str, limit: usize, filter: Option<MemoryFilter>) -> Vec<MemoryFragment>` —
  semantic search. Already uses internal `spawn_blocking`; safe to
  await directly.
- `set(agent: AgentId, key: &str, value: serde_json::Value)` and
  `get(...)` — structured KV. Not used directly by `retrieve_facts`
  (we run a raw `SELECT … FROM kv_store WHERE key = ?` so a single
  query covers all agents) but the seed-helper in tests uses `set`.
- `query_graph(pattern: GraphPattern) -> Vec<GraphMatch>` — graph
  scan. Same async-with-internal-spawn-blocking contract.

From the substrate directly (not on the `Memory` trait):

- `usage_conn() -> Arc<Mutex<Connection>>` — shared SQLite connection
  for raw SQL. Used for the FTS5 lookup and the KV-token-lookup
  raw SQL paths.

## Deviations

**[Rule 1 — fixed pre-existing test contract]** The lib's existing
`engine_reason_returns_not_yet_implemented` test asserted
`NotYetImplemented`. Plan 01-11 explicitly replaces that stub, so the
test was retargeted (not deleted) to verify the new Minimal-on-empty
path. Same level of coverage, just for the new contract.

**[Rule 3 — clippy fixes]** Three clippy warnings on first run:
- `manual_flatten` (×2) in `fact_retrieval::fts5_session_search` and
  `structured_kv_token_lookup`. Fixed in-place by switching to
  `rows.flatten()` per clippy's `try` hint.
- `manual_div_ceil` on `(chars + 3) / 4` in `engine::coarse_tokens`.
  Switched to `.div_ceil(4)`. Same arithmetic, modern idiom.

**[Rule 2 — added missing-critical tests]** Plan asked for 5 tests; I
shipped 9 plus 1 retarget. The extras:
- `reason_high_without_llm_also_errors` — symmetric coverage with the
  Low-without-LLM test so the `Medium/High/Max` arm doesn't regress
  unnoticed.
- `first_turn_caveat_is_verbatim` — the dashboard / agent renders this
  exact string. Pin the wording so a refactor can't silently rewrite
  user-facing text.
- `cost_estimate_*` (×2) — pin the level-monotone cost ordering so
  plan 01-13 can plumb real pricing without breaking the budget /
  approval gates that depend on `Max > High > Medium > Low`.

## Decisions made during execution

1. **FTS5 ranking via `bm25()`.** `bm25(session_messages_fts)` is
   negative; smaller (more-negative) = better. Map to `relevance = 1.0
   / (1.0 + |rank|)` so the rest of the engine sees a positive
   relevance score in `(0, 1]`. Same shape as semantic recall's
   `confidence` field.
2. **FTS5 / KV / graph failures degrade to empty.** Semantic recall is
   the only retrieval source at `Minimal` so its errors propagate.
   The other three are additive — a bad query syntax or a transient
   SQLite issue WARN-logs and yields no rows. This matches the plan's
   "best-effort" language and prevents one bad source from poisoning
   the whole call.
3. **`dedupe_and_cap` preserves first-seen order.** Callers append in
   order `Semantic → FTS → KV → Graph` and each source returns its own
   best-rank-first list, so first-seen ≈ highest-relevance globally.
   The dedup key is a synthetic string per `FactSource` variant.
4. **`reason_deep` covers Medium/High/Max with one body.** The
   level-specific deltas are limited to the default `max_facts`
   (10/20/30) and the confidence base (0.7/0.8/0.85). Keeps the code
   readable; per-level branches would just be a giant `match` over a
   linear function.
5. **First-turn handling consistent across levels.** Even `Low+` runs
   the LLM but the result gets `confidence=0.0` + caveat when
   `supporting_facts` came back empty. This is design § 2.4's "Low
   always synthesizes" + skeleton-open-decision 4's first-turn
   contract reconciled.
6. **`knowledge_graph_scan` uses `GraphPattern { max_depth: 0 }` +
   text filtering.** The substrate's graph API is built for relation
   walks (`source/relation/target` triples), not free-text entity
   lookup. The simplest entity-name-overlap filter at the reasoning
   layer keeps responsibilities cleanly split — a richer entity-text
   index can land in a future plan if Medium+ retrieval becomes a
   bottleneck.
7. **`structured_kv_token_lookup` is cross-agent.** The KV store is
   per-agent but the reasoning layer queries by token across all
   agents — the result is "bias the answer", not "authoritative".
   Plan 01-13 may add agent-scoping when the tool layer plumbs the
   caller's `agent_id`.
8. **`MAX_FACTS_CEILING = 50` as a hard cap.** Plan said
   "Combined output capped at `max_facts` (default 20 for High, 10
   for Medium)". I added a ceiling so a future caller passing
   `max_facts=1_000_000` can't OOM the engine.

## Follow-ups for later plans

- **01-12 (BudgetTracker integration):** the engine currently fills
  `estimated_cost_usd` from `cost_estimate(level, in_tokens,
  out_tokens)`. Plan 01-12's tracker should be wired here: pre-call,
  consult `BudgetTracker::current_month_spent()`; downgrade or block
  per `[reasoning].budget_exceeded_action`. Post-call, write a
  `BudgetRecord::new_now(level, in_t, out_t, cost, query)` via
  `BudgetTracker::record(...)`. Easiest plumbing: add an
  `Option<Arc<BudgetTracker>>` field to `ReasoningEngine` with a
  builder method `with_budget(...)`.
- **01-13 (`memory_reason` tool + KernelLlmAdapter):** plumb the real
  `ReasoningLlm` impl. `KernelLlmAdapter::synthesize` forwards through
  `KernelHandle::send_message`. Replace `coarse_tokens` with real
  token counts from `TokenUsage` carried by the driver response.
- **01-04 (session_search tool):** the FTS5 SQL in
  `fact_retrieval::fts5_session_search` is roughly the body that
  `SessionStore::search_sessions_fts` should expose. Lift it into
  `openfang-memory::session.rs` so both reasoning and the tool share
  one implementation.
- **`knowledge_graph_scan` entity-text index:** if Medium+ retrieval
  becomes a hotspot, add a `name`/`alias` LIKE index on `entities`
  and replace the in-Rust filter with a SQL prefix-match.
- **Caveats stack:** `caveats` is currently single-item. Plan 01-13 may
  add "budget exceeded, downgraded to Low" / "approval required and
  granted" entries; the field is `Vec<String>` so just push to it.
