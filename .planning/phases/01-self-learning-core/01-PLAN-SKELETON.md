# Phase 01 — Plan Skeleton (dependency graph + file ownership)

**Created:** 2026-06-06
**Status:** input to `gsd:plan-phase 01` — the planner agent expands each entry into a full `XX-YY-NAME-PLAN.md` file.

This skeleton is the dependency analysis the planner agent would otherwise have to re-derive. It locks file ownership for parallel execution. The planner takes each row and produces a PLAN.md with frontmatter + objective + 2-3 tasks + verification + success criteria + output trailer.

---

## Wave structure summary

| Wave | Plans | Parallel? | Rationale |
|------|-------|-----------|-----------|
| 1 | 01-01, 01-05, 01-06, 01-10, 01-15 | ✅ 5 parallel | Independent roots — no shared files |
| 2 | 01-02, 01-07, 01-11, 01-12 | ✅ 4 parallel | Depend only on Wave 1; no shared files within wave |
| 3 | 01-03, 01-08, 01-13 | ✅ 3 parallel | Tool-runner additions touch distinct match arms / schema entries |
| 4 | 01-04, 01-09, 01-14 | ⚠️ 3 sequential or careful merge | All three touch `tool_runner.rs`; planner must add anchor comments so additions don't conflict |
| 5 | 01-16 | checkpoint | Human-verify across all twelve success criteria |

Wave 1 → Wave 5 strictly sequential between waves. Within waves, parallelism per the table above.

---

## Plan table

| # | Slug | Wave | Depends on | Owned files (modified) | Owned files (new) | Sketch of work |
|---|------|------|------------|------------------------|-------------------|----------------|
| 01-01 | rusqlite-fts5-flag-and-flattener | 1 | — | `Cargo.toml`, `crates/openfang-memory/src/lib.rs` | `crates/openfang-memory/src/session_fts.rs` | Add `"fts5"` to rusqlite features. Create `session_fts` module with `flatten_message_content(&Message) -> String` and `role_string(&Role) -> &'static str`. Read `crates/openfang-types/src/message.rs` to enumerate `ContentBlock` variants and define a stable arm-per-variant mapping (text→verbatim, tool_use→`[tool_use:NAME] {input_json}` truncated to 2KB, tool_result→`[tool_result] {text_portion}`, image→`[image]`, etc.). Unit tests for each arm. |
| 01-02 | schema-v9-migration-backfill | 2 | 01-01 | `crates/openfang-memory/src/migration.rs` | — | Bump `SCHEMA_VERSION` 8→9. Add `migrate_v9` that creates `session_messages` table, `session_messages_fts` external-content FTS5 virtual table, insert/update/delete triggers, and best-effort backfill that walks existing `sessions.messages` BLOBs, calls `flatten_message_content` per message, inserts rows. Decode failure → WARN + skip session, do not abort migration. Add v8→v9 transition test on populated in-memory DB (per addendum § A.3): insert two test sessions as BLOBs at v8 state, run `migrate_v9`, assert BLOB preserved + flat table populated + FTS5 returns hit. Add broken-BLOB skip test. |
| 01-03 | sessionstore-dual-write | 3 | 01-02 | `crates/openfang-memory/src/session.rs` | — | Rewrite `save_session` to use a transaction: existing BLOB write + delete-then-insert into `session_messages` for that session_id. Use `flatten_message_content` from 01-01. Update `delete_session` and any agent-deletion path to cascade-delete from `session_messages`. Add unit test: save a session, assert flat rows present + FTS5 returns hit. |
| 01-04 | session-search-tool | 4 | 01-03 | `crates/openfang-runtime/src/tool_runner.rs` (add 1 dispatch arm + 1 schema entry — *use anchor comment* `// === PHASE 1 PLAN 01-04 session_search ===` so 01-09 + 01-13 + 01-14 additions don't merge-conflict) | — | Implement `tool_session_search(input, kernel)` calling `MemorySubstrate` via a new helper `search_sessions_fts(query: &str, limit: usize, agent_id: Option<AgentId>) -> Vec<SessionSearchHit>` added to `crates/openfang-memory/src/session.rs`. The helper runs FTS5 `MATCH` with `bm25()` ranking and `snippet()` for context. Return JSON `[{session_id, agent_id, role, timestamp, snippet, score}]`. Read-only capability — no capability flag required beyond default. Update tool count in any documented constants. |
| 01-05 | skillregistry-mutation-methods | 1 | — | `crates/openfang-skills/src/registry.rs`, `crates/openfang-skills/src/error.rs` (add `SkillError::Protected`, `SkillError::Immutable` variants) | — | Add six methods to `impl SkillRegistry`: `create_skill`, `patch_skill`, `edit_skill`, `write_skill_file`, `reload_skill`, `set_skill_enabled`. Each goes through the pipeline: prompt-injection scan (`SkillVerifier::scan_prompt_content`) → SHA256 + audit append via existing `AuditAppender` → TOML validation → config resolution (`apply_skill_config`) → atomic write (`tmp + rename` — use a small helper) → in-memory reload → emit `SkillUpdated` on event bus. Patch uses string-replacement (old_string/new_string with replace_all flag); reject if old_string appears 0 or >1 times unless `replace_all=true`. Unit tests per method using `tempfile::tempdir()`. |
| 01-06 | skill-manifest-flags | 1 | — | `crates/openfang-skills/src/manifest.rs` (add `mutable: Option<bool>`, `protected: Option<bool>` to `SkillManifestSkill`, `#[serde(default)]`) | — | Add the two optional fields to the manifest. Verify existing skill.toml files parse unchanged (the `Option<bool>` defaults to `None`, semantics applied at load time in 01-07). Add tests that explicitly check: skill.toml without fields parses; skill.toml with `mutable=true, protected=false` parses. |
| 01-07 | protected-mutable-defaults | 2 | 01-05, 01-06 | `crates/openfang-skills/src/registry.rs` (`load_bundled`), `crates/openfang-skills/src/lib.rs` (export `SYSTEM_SKILLS`) | — | Add `const SYSTEM_SKILLS: &[&str] = &["skill-manage", "tool-dispatch", "memory-core", "memory-reason", "session-manager", "session-search", "event-bus", "kernel-api", "security-scanner", "prompt-injection"];`. In `load_bundled`, apply defaults to loaded skills: if `mutable.is_none()` → `Some(false)` (bundled = immutable). If `protected.is_none()` → `Some(SYSTEM_SKILLS.contains(&name))`. Add `check_mutable(&self, name, action)` helper used by 01-05's mutation methods to return `ProtectedSkill`/`ImmutableSkill` errors with unlock hints. Plumb checks into all six mutation methods. Test: bundled skill rejects patch unless mutable=true; protected skill rejects patch even with mutable=true. |
| 01-08 | skill-manage-tool | 3 | 01-07 | `crates/openfang-runtime/src/tool_runner.rs` (anchor `// === PHASE 1 PLAN 01-08 skill_manage ===`), `crates/openfang-types/src/config.rs` (`capabilities.allow_skill_mutation: bool`, default false) | — | Implement `tool_skill_manage(input, kernel)`. Action enum: `create | patch | edit | delete | write_file | remove_file | list`. Schema per [`phase1-self-learning-architecture.md` § 1.4](../../docs/chaitex/phase1-self-learning-architecture.md). Capability gate: returns structured error if `capabilities.allow_skill_mutation == false`. On every mutation (everything except `list`), result includes `"skill_refresh_required": true` (used by 01-09). Tests: create + list shows new skill; patch on protected skill returns `ProtectedSkill`; create accepts optional `mutable` parameter. |
| 01-09 | snapshot-refresh-signal | 4 | 01-08 | `crates/openfang-runtime/src/agent_loop.rs`, `crates/openfang-runtime/src/tool_runner.rs` (post-process tool result; anchor `// === PHASE 1 PLAN 01-09 snapshot refresh ===`) | — | After each tool result in the agent loop, parse the `result` JSON for `"skill_refresh_required": true`. If true, request a fresh `SkillRegistry::snapshot()` from the kernel via `KernelHandle` (add `KernelHandle::fresh_skill_snapshot()` method). The loop replaces the `&SkillRegistry` it holds with the new snapshot before the next tool dispatch. Test: end-to-end — agent calls patch_skill, next tool call within same turn sees the patched content via the snapshot. |
| 01-10 | reasoning-crate-scaffold | 1 | — | root `Cargo.toml` (add workspace member) | `crates/openfang-reasoning/Cargo.toml`, `crates/openfang-reasoning/src/lib.rs`, `crates/openfang-reasoning/src/error.rs` (`ReasoningError` per [`phase1-self-learning-architecture.md` § 2.3](../../docs/chaitex/phase1-self-learning-architecture.md)) | New crate `openfang-reasoning`, Rust 2021, MSRV 1.75. Deps limited to: `openfang-types` (path), `openfang-memory` (path), `tokio`, `serde`, `serde_json`, `tracing`, `async-trait`, `chrono`. Public exports: `ReasoningEngine`, `ReasoningLevel`, `ReasoningQuery`, `ReasoningResult`, `ReasoningError`, `ReasoningLlm` trait. Smoke test: `cargo build -p openfang-reasoning`. |
| 01-11 | reasoning-engine-levels | 2 | 01-10 | — | `crates/openfang-reasoning/src/engine.rs`, `crates/openfang-reasoning/src/fact_retrieval.rs` | Implement `ReasoningEngine::reason(query)`. Five levels: `Minimal` (structured KV + knowledge graph lookup; no LLM), `Low` (semantic + FTS5 top-5 + light LLM synthesis), `Medium`/`High` (multi-source: semantic + FTS5 + graph + KV → cross-ref → LLM synthesis), `Max` (deep CoT synthesis). `ReasoningLlm` trait is the integration seam (implementation deferred to 01-13). Mock `ReasoningLlm` for tests. Tests: Minimal returns facts without LLM; Low calls LLM once; Medium calls LLM with multi-source context. |
| 01-12 | budget-tracker-config | 2 | 01-10 | `crates/openfang-types/src/config.rs` (new `[reasoning]` block with `deny_unknown_fields`) | `crates/openfang-reasoning/src/budget.rs` | Add `ReasoningConfig` struct with fields per [`phase1-self-learning-architecture.md` § 2.4.1](../../docs/chaitex/phase1-self-learning-architecture.md): `max_input_tokens=40000`, `max_output_tokens=8000`, `max_level="high"`, `monthly_budget_usd=20.0`, `budget_exceeded_action="warn"`, `require_approval_for_max=true`, `auto_update_profile=false`, `fts_backfill="on_startup"`. Apply `#[serde(deny_unknown_fields)]`. Implement `BudgetTracker` with `current_month_spent()`, `record(level, in_tokens, out_tokens, cost)`. Records persisted to a new SQLite table `reasoning_budget` (extends schema v9 migration in 01-02 or add as v9 amendment). On daemon startup, INFO log of effective reasoning config with explicit `(from config)` vs `(DEFAULT — no [reasoning] section)` marker. Tests: typo in `[reasoning]` returns config parse error; default marker logged correctly. |
| 01-13 | memory-reason-tool | 3 | 01-11, 01-12 | `crates/openfang-runtime/src/tool_runner.rs` (anchor `// === PHASE 1 PLAN 01-13 memory_reason ===`), `crates/openfang-kernel/src/kernel.rs` (initialize `ReasoningEngine` with `MemorySubstrate` + `BudgetTracker` + `KernelHandle`-backed `ReasoningLlm` impl) | `crates/openfang-reasoning/src/kernel_llm.rs` (the `impl ReasoningLlm for KernelLlmAdapter` that calls the agent's LLM via `KernelHandle`) | Implement `tool_memory_reason(input, kernel)`. Reads `reasoning_level` (default `medium`), `query`, `max_facts`. Calls `BudgetTracker` precheck (level allowed, budget not exceeded, Max requires approval flag). Calls `ReasoningEngine::reason`. Surfaces `estimated_cost_usd` in the result (per MR-07). Tests: medium-level call returns `answer + supporting_facts + confidence`; max-level call without approval returns `ApprovalRequired`; budget-exceeded with `warn` mode downgrades to Low; with `block` mode returns `BudgetExceeded`. |
| 01-14 | user-profile-and-conclude | 4 | 01-13 | `crates/openfang-runtime/src/tool_runner.rs` (anchor `// === PHASE 1 PLAN 01-14 memory_conclude ===`) | `crates/openfang-reasoning/src/profile.rs` | Implement `UserProfile`, `UserFact`, `Preference`, `BehavioralPattern` types per [`phase1-self-learning-architecture.md` § 2.8](../../docs/chaitex/phase1-self-learning-architecture.md). Stored at `__user_profile__` key in structured KV. New tool `tool_memory_conclude(input, kernel)`: agent explicitly persists a fact/preference/pattern. If `reasoning.auto_update_profile == true`, `memory_reason` calls this internally after Medium+ queries; if false (default), the agent must call it explicitly. Tests: explicit conclude writes; reason with auto_update=false does NOT write; reason with auto_update=true writes after Medium. |
| 01-15 | api-key-zeroizing-changelog | 1 | — | `crates/openfang-types/src/config.rs` (only the `api_key` field — does not collide with 01-12's `[reasoning]` block addition), `CHANGELOG.md` | — | Replace `pub api_key: String` with `pub api_key: zeroize::Zeroizing<String>` on the provider config struct. Update all call sites that consume it (search `grep -rn "api_key" crates/`). Add CHANGELOG entry under `[Unreleased]`: "schema v9 (FTS5 session search), `openfang-reasoning` crate, `skill_manage`/`session_search`/`memory_reason`/`memory_conclude` tools, `api_key` zeroized. Backward compatible — existing databases backfilled on first start." Confirm `cargo build / test / clippy -D warnings` pass. |
| 01-16 | live-integration-checkpoint | 5 | 01-04, 01-09, 01-14, 01-15 | — | `.planning/phases/01-self-learning-core/01-16-UAT.md` | **type: `checkpoint:human-verify`** — pauses execution. Run the 8-step live integration test per [`CLAUDE.md`](../../CLAUDE.md): kill any running daemon → `cargo build --release -p openfang-cli` → `GROQ_API_KEY=… target/release/openfang.exe start &` → curl-create agent → exercise all four new tools (`skill_manage`, `session_search`, `memory_reason`, `memory_conclude`) end-to-end → verify side effects (skill file on disk, `session_messages` populated, `reasoning_budget` row inserted) → check dashboard surfaces new tools → confirm CHANGELOG entry. User signs off on the 12 success criteria from [`REQUIREMENTS.md`](../../REQUIREMENTS.md). UAT.md captures pass/fail per criterion. |

---

## File-ownership conflict resolution

`tool_runner.rs` is touched by plans 01-04, 01-08, 01-09, 01-13, 01-14. Each plan adds:
- One match arm to the dispatch (around line 203)
- One JSON schema entry to the schema list (around line 645+)
- For 01-09, additionally a post-process step at the result-handling site

**Conflict mitigation**: each plan inserts code at unique anchor comments (`// === PHASE 1 PLAN 01-NN <tool_name> ===`). When two plans run in parallel and both modify the file, the merge is line-disjoint by construction.

Executor must surround its additions with the anchor comments verbatim. Plans 01-13/01-14 in particular both add to the schema list — they target the same section but use distinct anchors so concurrent edits don't collide.

`crates/openfang-types/src/config.rs` is touched by 01-08 (capability flag), 01-12 (`[reasoning]` block), 01-15 (api_key zeroize). All three target different lines of the same file:
- 01-08: `capabilities` sub-struct → add one bool field
- 01-12: new top-level `ReasoningConfig` struct + `reasoning: ReasoningConfig` field on `KernelConfig`
- 01-15: existing `api_key` field type change on provider configs

Line-disjoint, no conflict.

---

## Open decisions resolved here (recorded for the planner)

Five decisions deferred from `01-CONTEXT.md`:

1. **Message content flattening rules** — owned by 01-01. Planner reads `crates/openfang-types/src/message.rs` and enumerates every `ContentBlock` variant.
2. **`set_skill_enabled` semantics** — owned by 01-05. Enabled=false hides skill from `list()` / dispatch, file kept on disk, can be re-enabled. Decided.
3. **Capability flag name** — owned by 01-08. Picked `capabilities.allow_skill_mutation`.
4. **`memory_reason` first-turn behavior** — owned by 01-13. Empty `supporting_facts`, `confidence=0.0`, `caveats=["No conversation history available — answers are speculative."]`.
5. **Max approval UX** — owned by 01-13. Tool returns structured `ApprovalRequired { estimated_cost_usd, query_preview }` error; agent re-calls with `approved: true` in the input. No `ApprovalManager` integration (kept simple — agent surfaces approval to user, user re-issues query).

---

## Inputs to the planner agent

When expanding this skeleton, the planner agent reads:

1. This skeleton (line by line for each plan).
2. `.planning/phases/01-self-learning-core/01-CONTEXT.md` (loaded context, verified anchors).
3. `docs/chaitex/phase1-self-learning-architecture.md` (design intent, schemas, examples).
4. `docs/chaitex/phase1-addendum-codebase-grounding.md` (corrections — load over the original where they conflict).
5. `.planning/codebase/STRUCTURE.md`, `ARCHITECTURE.md`, `CONVENTIONS.md`, `TESTING.md` (codebase patterns).
6. `.planning/REQUIREMENTS.md` (REQ-IDs to cite in `must_haves.truths`).
7. Specific source files cited per row (the planner reads them once to confirm signatures).

The planner writes 16 `XX-NN-SLUG-PLAN.md` files into `.planning/phases/01-self-learning-core/`.
