# REQUIREMENTS

**Created:** 2026-06-06
**Scope:** Phase 1 (Self-Learning Core). Phases 2-3 requirements deferred until Phase 1 lands.
**Traceability:** REQ-IDs are referenced by `ROADMAP.md` phase entries and by per-phase verification docs.

---

## Phase 1: Self-Learning Core — Skill Self-Patching (SP-*)

**SP-01.** Built-in tool `skill_manage` exposed to agents, accepting actions: `create`, `patch`, `edit`, `delete`, `write_file`, `remove_file`, `list`. Schema per [`phase1-self-learning-architecture.md` § 1.4](../docs/chaitex/phase1-self-learning-architecture.md).

**SP-02.** `SkillRegistry` gains six mutation methods (`create_skill`, `patch_skill`, `edit_skill`, `write_skill_file`, `reload_skill`, `set_skill_enabled`). Every mutation runs through: prompt injection scan → SHA256 + audit append → TOML validation → config resolution → atomic file write (tmp + rename) → in-memory reload → `SkillUpdated` event on bus.

**SP-03.** Two-tier defense for skill mutation:
- `mutable: Option<bool>` and `protected: Option<bool>` in `skill.toml`.
- Defaults applied in `SkillRegistry::load_bundled` from a static `SYSTEM_SKILLS` array (not from a build script). Bundled skills load with `mutable=false`. Skills in `SYSTEM_SKILLS` (including `skill-manage`) load with `protected=true`.
- `create_skill` takes an optional `mutable` parameter, default `true` for user-created skills.
- Attempting `patch`/`edit`/`delete` on a protected skill returns a structured `ProtectedSkill` error with unlock instructions, without disk modification.

**SP-04.** Snapshot refresh after mutation: `skill_manage` results include a `skill_refresh_required: true` sentinel; the agent loop detects it and re-snapshots `SkillRegistry` before the next tool dispatch. Mid-turn `patch_skill` is visible to subsequent calls in the same turn. (Per [`phase1-addendum-codebase-grounding.md` § B.1](../docs/chaitex/phase1-addendum-codebase-grounding.md).)

**SP-05.** All mutations append a Merkle audit entry via the existing audit infrastructure (`audit_entries` table). No new audit format.

## Phase 1: Self-Learning Core — FTS5 Session Search (FTS-*)

**FTS-01.** `Cargo.toml` rusqlite features include `"fts5"`. `cargo build --workspace --lib` succeeds; `PRAGMA compile_options` reports `ENABLE_FTS5`.

**FTS-02.** Schema v9 migration introduces:
- A flat companion table `session_messages (session_id, agent_id, message_index, role, content, timestamp)` with composite primary key and indexes on `agent_id` and `session_id`.
- An external-content FTS5 virtual table `session_messages_fts` over `session_messages.content`, tokenizer `porter unicode61 remove_diacritics 1`.
- Insert/update/delete triggers that keep the FTS index synchronized with `session_messages`.
- Best-effort backfill of existing sessions: walks the msgpack BLOB in `sessions.messages`, flattens each `Message` via a stable `flatten_message_content()`, inserts rows. Decode failures log WARN and skip the session; do not abort the migration.

**FTS-03.** `SessionStore::save_session` writes the msgpack BLOB AND rewrites `session_messages` for that session within a single transaction (delete-then-insert by session_id). Read path (`get_session`) unchanged. `delete_session` and `delete_agent` cascade to `session_messages`.

**FTS-04.** Built-in tool `session_search(query, limit?, agent_id?)` exposed to agents. Returns ranked results with FTS5 BM25 score and content snippet (`snippet()` SQL helper). Per [`phase1-self-learning-architecture.md` § 3](../docs/chaitex/phase1-self-learning-architecture.md).

**FTS-05.** Migration test for v8→v9 on a populated database verifies: existing BLOB sessions preserved, `session_messages` backfilled, FTS5 returns hits for keywords known to be in test messages, broken-BLOB session skipped without aborting migration. Per [`phase1-addendum-codebase-grounding.md` § A.3](../docs/chaitex/phase1-addendum-codebase-grounding.md).

**FTS-06.** Optional config flag `[reasoning] fts_backfill = "on_startup" | "lazy" | "off"` to defer backfill on slow disks. Default `"on_startup"`. Lazy mode only populates flat table on subsequent `save_session` calls.

## Phase 1: Self-Learning Core — Memory Reasoning (MR-*)

**MR-01.** New crate `openfang-reasoning` in workspace, Rust 2021, MSRV 1.75. Dependencies limited to: `openfang-types`, `openfang-memory`, `tokio`, `serde`, `serde_json`, `tracing`, `async-trait`. Registered in root `Cargo.toml`.

**MR-02.** Five reasoning levels (`Minimal`, `Low`, `Medium`, `High`, `Max`). `ReasoningEngine::reason(query)` dispatches by level. Per-level behavior matches [`phase1-self-learning-architecture.md` § 2.4](../docs/chaitex/phase1-self-learning-architecture.md).

**MR-03.** `ReasoningEngine` calls the agent's LLM via the existing `KernelHandle` — no new HTTP client, no new provider configuration. The `ReasoningLlm` trait is the integration seam.

**MR-04.** Built-in tool `memory_reason(query, reasoning_level?, max_facts?)` exposed to agents. Returns `{answer, supporting_facts, confidence, level, caveats}`. Per [`phase1-self-learning-architecture.md` § 2.6](../docs/chaitex/phase1-self-learning-architecture.md).

**MR-05.** Budget tracking:
- `[reasoning]` config block with `max_input_tokens`, `max_output_tokens`, `max_level`, `monthly_budget_usd`, `budget_exceeded_action` (`"warn"` | `"block"`), `require_approval_for_max`.
- Deserialization for `[reasoning]` uses `deny_unknown_fields` — typos fail loud, not silent.
- At startup, the loaded reasoning config is logged at INFO with explicit marker `(from config)` vs `(DEFAULT — no [reasoning] section)`.
- `BudgetTracker` persists `BudgetRecord` per query (timestamp, level, tokens, est. cost, query preview ≤100 chars).
- Defaults: `max_level="high"`, `monthly_budget_usd=20.0`, `require_approval_for_max=true`, `max_input_tokens=40000`, `budget_exceeded_action="warn"`.

**MR-06.** `UserProfile` struct (facts, preferences, behavioral patterns) stored at `__user_profile__` key in structured KV. **Updates are opt-in**: `reasoning.auto_update_profile = false` (default). When false, the agent calls a separate `memory_conclude` tool to persist; when true, `memory_reason` writes after every Medium+ call. Resolves the contradiction noted in [`phase1-addendum-codebase-grounding.md` § C.1](../docs/chaitex/phase1-addendum-codebase-grounding.md).

**MR-07.** Each `memory_reason` result includes per-query estimated cost in the response (visible to the agent and to users via dashboard). Builds the budget transparency contract.

**MR-08.** When monthly budget is exceeded:
- `budget_exceeded_action="warn"`: subsequent calls forced to `Low` level; structured warning emitted.
- `budget_exceeded_action="block"`: subsequent calls return `BudgetExceeded` error.

## Phase 1: Self-Learning Core — Cross-cutting (X-*)

**X-01.** New tools (`skill_manage`, `session_search`, `memory_reason`) registered in all three required touchpoints of [`crates/openfang-runtime/src/tool_runner.rs`](../crates/openfang-runtime/src/tool_runner.rs): dispatch match (~line 203), schema list (~line 645+), tool-profile/permission gates as applicable. (Per addendum § B.3.)

**X-02.** Capability gates: `skill_manage` requires a new capability flag (e.g. `capabilities.allow_skill_mutation`) defaulting to off in `Stable` mode and on in `Continuous`/`Proactive` modes. `memory_reason` and `session_search` are read-only-ish and capability-free.

**X-03.** `api_key: String` in `KernelConfig` migrated to `Zeroizing<String>` (closes drift noted in `.planning/codebase/CONCERNS.md`). Same PR as Phase 1 since the budget config also touches `KernelConfig` serialization.

**X-04.** CHANGELOG.md entry for schema v9, FTS5 enablement, new crate `openfang-reasoning`, new tools. Backward-compat note for existing user databases.

**X-05.** Live integration test (CLAUDE.md workflow): `openfang start` → curl-create agent → exercise each new tool end-to-end → verify side effects (skill file changed on disk, session_messages populated, BudgetTracker persisted). Documented in phase verification doc.

**X-06.** All workspace verification gates pass: `cargo build --workspace --lib`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`. New tests added for every new public API.

**X-07.** Loud-degrade config policy (decided after W3, 2026-06-07, in lieu of changing `load_config`'s signature):
- `crates/openfang-kernel/src/config.rs` exposes `ConfigStatus::{Ok, Degraded { source, error }}` and `load_config_with_status(...) -> LoadResult { config, status }`.
- The legacy `load_config(...) -> KernelConfig` shim discards status — backward-compat for unaffected callers.
- On read/parse/deserialize failure of an *existing* config file: emit ERROR-level tracing log + stderr banner + populate `ConfigStatus::Degraded`. Daemon continues on `KernelConfig::default()`.
- Missing config file = `ConfigStatus::Ok` (defaults are intentional).
- `OpenFangKernel` gains a `pub config_status: ConfigStatus` field, surfaced on `/api/health` (`"ok" | "degraded"`) and `/api/health/detail` (full error + source path).
- Closes GAP-012-Tier-2 from `.planning/codebase/CONCERNS.md`.

---

## Mapping to Phase 1 success criteria (goal-backward)

| # | Must be TRUE | Covers REQ-IDs |
|---|--------------|----------------|
| 1 | Agent calls `skill_manage(create)`, new skill written + visible to next tool call in same turn | SP-01, SP-02, SP-04 |
| 2 | Agent calls `skill_manage(patch)` on mutable skill → file diff verified on disk → patched version dispatched on next call | SP-01, SP-02, SP-04, SP-05 |
| 3 | `skill_manage(patch)` on protected skill returns structured error, no disk mutation | SP-03 |
| 4 | `session_search("rust")` over an agent's history returns ranked results with snippets in <500ms on pc162 | FTS-01, FTS-02, FTS-03, FTS-04 |
| 5 | `memory_reason(q, level=medium)` returns synthesized answer with `supporting_facts` and `confidence` | MR-01, MR-02, MR-03, MR-04 |
| 6 | `memory_reason(q, level=max)` rejected with `ApprovalRequired` unless `require_approval_for_max=false` | MR-05 |
| 7 | Monthly budget exceeded: downgrade-to-Low (warn mode) or block (block mode); persisted records show transition | MR-05, MR-08 |
| 8 | Workspace gates pass (build, test, clippy `-D warnings`); v8→v9 migration test on populated DB included | FTS-05, X-06 |
| 9 | Live integration test passes per CLAUDE.md workflow | X-01, X-05 |
| 10 | 60 bundled skills load with correct defaults (`mutable=false`; `SYSTEM_SKILLS` subset `protected=true`); no build-script mutation of source files | SP-03 |
| 11 | Typo in any config section is **loud**: ERROR log + stderr banner; `/api/health` reports `config_status: "degraded"`; `/api/health/detail` exposes the full parse error. Daemon continues on defaults (loud-degrade policy, decided 2026-06-07 after W3 — see `STATE.md` decision log). For a clean load OR no-file-present, status is `"ok"` and the boot log carries the `(from config)` vs `(DEFAULT — no [reasoning] section)` marker from plan 01-12. | MR-05, X-07 |
| 12 | CHANGELOG entry present with schema-v9 backward-compat note | X-04 |

These twelve criteria become the Phase 1 verification checklist consumed by `gsd:verify-work`.

---

## Phase 1.1: Autonomous Skill Distillation Loop

**Scope:** Adds three autonomous loops on top of the Phase 1 mechanisms: post-turn reflection → skill distillation, skill self-improvement on failure-then-recovery, and cron-driven memory consolidation nudge. All loops are opt-in via `[distillation]` config and run behind existing security gates and budget ceilings.

### Skill Distillation (SD-*)

**SD-01.** Post-turn reflection scorer computes a `TurnStats` value (iterations/tool-call count, error-recovery count, total tokens, wall-clock ms) from `AgentLoopResult` data available at `kernel.rs:3033`. `AgentLoopResult` gains a `error_recovery_count: u32` field populated by the agent loop on each error-then-retry sequence.

**SD-02.** A pure-heuristic `reflection_score()` (no LLM) maps `TurnStats` to a [0.0, 1.0] score; turns at/above `distillation.reflection_threshold` enqueue a `DistillationJob` on a bounded in-memory queue (VecDeque, cap 50, drops oldest when full). Queue is non-blocking — a slow worker never stalls an agent turn.

**SD-03.** A distillation worker dequeues jobs, calls `ReasoningEngine::reason(query, Medium)` to judge whether a reusable procedure was discovered, and on confidence above threshold creates a draft skill via the existing `create_skill` path (injection scan + SHA256 + audit).

**SD-04.** A daily distillation cap (`distillation.daily_cap`, UTC calendar day) is enforced and the counter persists across daemon restarts via a JSON sidecar `~/.openfang/distillation_state.json` (atomic write); the counter resets when the stored date != today. `daily_cap = 0` disables distillation.

**SD-05.** Draft skills are created disabled (`enabled = false` via `set_skill_enabled`); approval flips them to `enabled = true`. Protected-skill gates are unchanged. When `distillation.auto_approve_non_protected = true`, non-protected drafts are enabled immediately upon creation.

**SD-06.** Before creating a draft, candidate skills are deduplicated by registry name lookup (`registry.get(name).is_some()`) plus a description-similarity heuristic; skills are NOT FTS5-indexed so no FTS5 dedupe is attempted. If a matching skill already exists, the job is discarded and a DEBUG log is emitted.

### Skill Self-Improvement (SI-*)

**SI-01.** When `skill_execute` returns `is_error = true`, the event is recorded in a kernel-global `SkillFailureTracker` keyed by `(skill_name, agent_id)`. The tracker records the timestamp and error hash for each failure event.

**SI-02.** When the same skill's failure pattern repeats `distillation.failure_patch_threshold` times (default 3) within the TTL window, a `skill_manage(patch)` proposal is raised: a direct patch path for mutable skills, an `ApprovalManager` request for protected skills. The patch proposal includes the error history as context.

**SI-03.** `SkillFailureTracker` is bounded (max 20 events per key) and decays (entries older than a configurable TTL, default 7 days, dropped on insert) to prevent stale counts from driving spurious patches. The tracker lives in kernel memory only — no persistence required.

### Memory Consolidation Nudge (MC-*)

**MC-01.** A kernel-internal background task (`tokio::spawn` in `start_background_tasks`, modeled on the `memory.consolidate()` loop at `kernel.rs:4671`) runs memory consolidation on the `distillation.consolidation_nudge_hours` interval (0 = disabled). This is NOT a user-facing cron job.

**MC-02.** The consolidation nudge invokes reasoning/profile helpers directly; all LLM cost is charged through the existing `BudgetTracker`, and `budget_exceeded_action` (warn-downgrade / block) is respected — on budget exceeded the task skips the LLM call and logs at INFO.

### Cross-cutting (X-*)

**X-01.** New `[distillation]` config section (`enabled`, `reflection_threshold`, `daily_cap`, `auto_approve_non_protected`, `consolidation_nudge_hours`, `failure_patch_threshold`) WITHOUT `deny_unknown_fields`. Lives in its own top-level TOML section, separate from `[reasoning]` which already uses `deny_unknown_fields` and would loud-degrade on unknown keys.

**X-02.** All config fields added to `KernelConfig` struct and its `Default` impl (CLAUDE.md mandatory requirement). `DistillationConfig` defaults: `enabled=false`, `reflection_threshold=0.5`, `daily_cap=10`, `auto_approve_non_protected=false`, `consolidation_nudge_hours=0`, `failure_patch_threshold=3`.

---

### Mapping to Phase 1.1 success criteria (goal-backward)

| # | Must be TRUE | Covers REQ-IDs |
|---|--------------|----------------|
| 1 | After a multi-tool agent turn, a draft skill appears in the registry (disabled) and can be queried via `skill_manage(list)` | SD-01, SD-02, SD-03, SD-05 |
| 2 | Draft skill for a non-protected name with `auto_approve_non_protected=true` is created enabled (no approval step) | SD-05 |
| 3 | Protected-skill failure repeating 3× within TTL raises an `ApprovalManager` request (not a direct patch) | SI-01, SI-02 |
| 4 | Mutable-skill failure repeating 3× within TTL creates a direct patch without approval | SI-01, SI-02 |
| 5 | Daily cap enforced: after `daily_cap` distillations, further jobs are discarded; cap counter survives daemon restart | SD-04 |
| 6 | `SkillFailureTracker` never exceeds 20 events per key; events older than 7 days are dropped on insert | SI-03 |
| 7 | Memory consolidation nudge fires on the configured interval, charges cost to BudgetTracker | MC-01, MC-02 |
| 8 | When budget exceeded, consolidation nudge skips LLM call and logs INFO; does not error/crash | MC-02 |
| 9 | Workspace gates pass: `cargo build --workspace --lib`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` | X-01, X-02 |
| 10 | Live integration test: start daemon, run multi-tool agent turn, verify draft skill appears; approve it and verify `enabled=true` | SD-01, SD-03, SD-05 |
