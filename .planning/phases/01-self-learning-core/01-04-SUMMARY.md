---
phase: 01-self-learning-core
plan: 04
status: complete
wave: 4
commit_hashes:
  - 1ceaddb feat(01-04): SessionStore.search_sessions_fts + lift reasoning FTS
  - c1a15dd feat(01-04): session_search tool dispatch + schema
---

# Plan 01-04 — session_search tool — SUMMARY

## What was implemented

- New `openfang_memory::session::SessionSearchHit` (serde-able: session_id /
  agent_id / role / timestamp / snippet / score) and
  `SessionStore::search_sessions_fts(query, limit, agent_id)` method that
  runs the FTS5 query `MATCH ?1 ORDER BY bm25(...) LIMIT ?N` against the
  v9 `session_messages_fts` virtual table, with `snippet(... '<b>','</b>')`
  highlighting and a 50-row server-side hard cap.
- Lifted `openfang-reasoning::fact_retrieval::fts5_session_search` to
  delegate to the new helper through a fresh `SessionStore` wrapped over
  `MemorySubstrate::usage_conn()` inside `tokio::task::spawn_blocking` —
  one canonical SQL path now serves both reasoning and the new tool.
- New `KernelHandle::session_search_fts` default-empty trait method +
  `OpenFangKernel` impl that delegates to a new
  `MemorySubstrate::search_sessions_fts` pass-through (keeps the private
  `sessions` field encapsulated).
- Wired `tool_session_search` dispatch arm + schema entry in
  `tool_runner.rs` under anchor blocks
  `// === PHASE 1 PLAN 01-04 session_search ===` and
  `// === PHASE 1 PLAN 01-04 session_search schema ===` (disjoint from
  W3 anchors 01-08 / 01-13).
- Read-only tool — no capability gate; returns a JSON array of hits or
  `"[]"` on empty; structured error JSON for KernelUnavailable /
  InvalidAgentId / FtsSearchFailed.

## Files changed (final)

- `crates/openfang-memory/src/session.rs`
- `crates/openfang-memory/src/substrate.rs`
- `crates/openfang-reasoning/src/fact_retrieval.rs`
- `crates/openfang-runtime/src/kernel_handle.rs`
- `crates/openfang-runtime/src/tool_runner.rs`
- `crates/openfang-kernel/src/kernel.rs`

## Tests added

3 new unit tests on `SessionStore` in `crates/openfang-memory/src/session.rs`:
1. `search_sessions_fts_returns_hit_with_snippet` — verifies hits contain
   `<b>...</b>` highlight markers and correct session/agent IDs.
2. `search_sessions_fts_agent_filter_narrows` — proves the optional
   `agent_id` filter restricts hits to one agent only (2 → 1).
3. `search_sessions_fts_limit_cap_respected` — proves `limit` is honored
   (10 matches → 3 hits when `limit=3`).

## Live latency observed

Not run in this wave. Live integration is deferred to plan 01-16
(W5 human-verify checkpoint requiring `GROQ_API_KEY` and user sign-off,
per executor mission). Unit tests cover the SQL/snippet correctness;
501 baseline `openfang-memory` tests + the 3 new ones pass in ≈ 0.02s
in-process so the < 500ms FTS-04 budget is comfortably achievable.

## Decisions made not pinned in the plan

- The reasoning-layer `FactReference::source = Session { ..., message_index }`
  field is now always 0 — the new helper returns a row-level shape
  (session_id, agent_id, role, timestamp, snippet, score) instead of the
  old `message_index`-aware row. Downstream callers in
  `openfang-reasoning` don't read `message_index`, so this is invisible.
- The reasoning path surfaces the FTS5 `snippet(...)` text (with `<b>`
  markers) as `FactReference.content` instead of the raw message. Reading
  the highlighted span is more useful to the synthesizer; the raw text is
  still retrievable via the `session_id` if a deeper drill-down is needed.
- Added a thin `MemorySubstrate::search_sessions_fts` pass-through (vs
  exposing the private `sessions` field) so the kernel impl stays a one-
  liner and the encapsulation pattern matches `structured_get/set`.

## Follow-ups for later plans

- Plan 01-09 will consume the typed `ToolOutcome` from `skill_manage`
  results; no impact on `session_search` (read-only, no sentinel).
- Plan 01-16 live verify: send two "neutrino"/"octopus" messages, ask
  the agent to call `session_search`, verify `<b>neutrino</b>` appears
  in the result snippet, record latency.
