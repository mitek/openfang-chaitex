# Phase 01 — Self-Learning Core — SUMMARY

**Status:** ✅ shipped + signed off
**Started:** 2026-06-06
**Signed off:** 2026-06-08 (Dmitry Shilov)
**Final HEAD at sign-off:** (this commit)

---

## One-liner

OpenFang gained four agent-visible tools — `session_search`, `skill_manage`, `memory_reason`, `memory_conclude` — plus a 5-level reasoning engine with budget control, a flat session table indexed by SQLite FTS5, and a two-tier `protected`/`mutable` skill defense, all shipped additively without breaking the existing 14-crate workspace or any of the 60 bundled skills.

## What was delivered

### Four new agent-visible tools

| Tool | What it does | Plan |
|---|---|---|
| `session_search(query, limit?, agent_id?)` | Ranked FTS5 full-text search across the agent's conversation history; returns BM25 score + snippet per hit | 01-04 |
| `skill_manage(action, ...)` | Create / patch / edit / delete / write_file / remove_file / list — agent can author and improve its own skills; protected by `[capabilities] allow_skill_mutation` flag and the `protected`/`mutable` per-skill defense | 01-05, 01-07, 01-08 |
| `memory_reason(query, reasoning_level?, max_facts?)` | 5-level (Minimal/Low/Medium/High/Max) synthesized reasoning over the agent's memory; Max requires explicit `approved=true`; per-call cost surfaced in result; BudgetTracker clamps spending | 01-11, 01-13 |
| `memory_conclude(fact/preference/pattern, ...)` | Agent explicitly persists a derived fact about the user into `__user_profile__` KV (alternative: opt-in auto-writeback via `reasoning.auto_update_profile=true`) | 01-14 |

### New crate

- `openfang-reasoning` — 5-level `ReasoningEngine`, `BudgetTracker` with `[reasoning]` config + `deny_unknown_fields`, opt-in `UserProfile` writeback, `KernelLlmAdapter` wrapping `Weak<KH>` so reasoning reuses the agent's existing LLM driver without a new HTTP client.

### Schema v9 + storage

- New flat companion table `session_messages (session_id, agent_id, message_index, role, content, timestamp)` populated by **dual-write** in `SessionStore::save_session` + a one-time **best-effort backfill** of existing v8 msgpack BLOB sessions on upgrade.
- External-content FTS5 virtual table `session_messages_fts` with insert/update/delete triggers — gives session_search BM25 + `snippet()` capability without disturbing the existing msgpack BLOB read path.
- New `reasoning_budget` table for persisted per-query `BudgetRecord` (timestamp, level, in/out tokens, est. cost USD, query preview).
- v8→v9 transition tests on populated DB (`migrate_v8_to_v9_preserves_sessions_and_backfills_fts`, `migrate_v8_to_v9_skips_broken_blob`).

### Self-modification safety

- `SkillRegistry` gained six mutation methods (`create_skill`, `patch_skill`, `edit_skill`, `write_skill_file`, `reload_skill`, `set_skill_enabled`) + two further wrappers (`delete_skill`, `remove_skill_file`) — total 8 actions exposed via `skill_manage`.
- Two-tier defense:
  - `mutable: Option<bool>` and `protected: Option<bool>` on `SkillMeta`
  - Defaults applied at load time via `apply_load_time_defaults` + `SYSTEM_SKILLS` const (no build-script mutation of `bundled/` files; `git diff bundled/` is empty)
- Every mutation goes through: prompt-injection scan → SHA256 + audit append → TOML validation → atomic tmp+rename write → in-memory reload → `SkillUpdated` event on broadcast bus.
- `skill_manage` returns `"skill_refresh_required": true` JSON sentinel after every mutation; the agent loop subscribes to `kernel.skill_updated_tx` AND consumes the sentinel to call `KernelHandle::fresh_skill_snapshot()` before the next tool dispatch → mid-turn self-modification is visible to the agent in the same turn.

### Cross-cutting hardening

- `KernelConfig.api_key: String` → `Zeroizing<String>` via scoped serde adapter (closes drift between SECURITY.md claim and code).
- New `[capabilities]` config section with `allow_skill_mutation: bool` (default `false`) gating `skill_manage`.
- **Loud-degrade config policy** (closes long-standing `TODO(GAP-012-Tier-2)`): `ConfigStatus::{Ok, Degraded}` + `load_config_with_status` + ERROR-level log + stderr banner + `/api/health config_status: degraded` + `/api/health/detail config_error+source`. Daemon still boots on defaults so the operator can recover via the API.
- `[reasoning]` config block uses `#[serde(deny_unknown_fields)]` — typos in this section are detected by serde and surfaced via the loud-degrade path.
- Boot-time INFO log emits the effective reasoning config with explicit `(from config)` vs `(DEFAULT — no [reasoning] section)` markers.

## Numbers

| | |
|---|---|
| Implementation plans | 16 (W1=5, W2=4, W3=3, W3.5=1 cross-cutting, W4=3, W5=1 UAT) |
| Total commits | 57 (feat / fix / test / chore / docs) |
| Net new tests added | +90 across the phase (2750 baseline → ~2840 at sign-off) |
| Workspace test suite at sign-off | 2834 passing, 0 failed |
| Workspace gates | `cargo build --workspace --lib` clean · `cargo clippy --workspace --all-targets -- -D warnings` clean |
| New crate | `openfang-reasoning` |
| Schema migration | v8 → v9 (additive; existing BLOB read path unchanged) |
| New SQLite tables | `session_messages`, `session_messages_fts` (virtual), `reasoning_budget` |
| New built-in agent tools | 4 (`session_search`, `skill_manage`, `memory_reason`, `memory_conclude`) |
| New built-in agent tool actions | 8 for `skill_manage` |
| Bundled skill files mutated by build scripts | 0 (`git diff crates/openfang-skills/bundled/` empty) |
| Live UAT spend on DeepSeek | ~$0.03 (10+ agent turns) |

## Key architectural decisions captured during the phase

(See `STATE.md` decisions log for the full list with rationale.)

- **No `Tool` trait refactor** — new tools land as free fns + dispatch-match arms + schema-list entries. Anchor comments (`// === PHASE 1 PLAN 01-NN <tool> ===`) make parallel-wave plans merge-clean.
- **Snapshot refresh via signal**, not RwLock refactor — `kernel.skill_updated_tx` broadcast + JSON sentinel. Surgical, no blast radius.
- **`KernelLlm` trait, not `KernelHandle::complete`** — matches the existing `AuditAppend` / `SkillEventBus` trait-extension pattern. Crate DAG stays acyclic.
- **`execute_tool_with_outcome` wrapper, not signature change** — `execute_tool -> ToolResult` preserved for backward compat with ~15 test sites + the openfang-api route; agent loop migrated to the new wrapper. Zero downstream churn.
- **`SessionStore::search_sessions_fts` is the canonical FTS5 SQL** — lifted from `openfang-reasoning::fact_retrieval` so reasoning and the `session_search` tool share one implementation.
- **`UserProfile` writeback is opt-in**, default off (`reasoning.auto_update_profile = false`). Resolves the design-doc contradiction about automatic memory writes.
- **rusqlite 0.31 has no `fts5` Cargo feature** — that feature is in ≥0.32 for Rust-side tokenizer API; the FTS5 virtual table itself was already available via `libsqlite3-sys` bundled build. No version bump needed for Phase 1.
- **`SYSTEM_SKILLS` const is forward-looking** — no bundled skill currently has a name in the list; the protection mechanism is in place and will activate when any future bundled skill is named into the list. `mutable=false` default for bundled covers the immediate case.
- **Loud-degrade chosen over hard-fail or hybrid** (W3.5 decision). Aligns with the in-code `TODO(GAP-012-Tier-2)` author intent. ERROR log + stderr banner + `/api/health` exposure. Doesn't break `load_config`'s existing signature.

## Tests against design drift

UAT (plan 01-16) surfaced one real bug: `openfang start` was bypassing the new `load_config_with_status` path and always reporting `config_status: ok`. Fixed inline (commit `b2b056d`) and re-verified live. This is the kind of integration-only issue unit tests can't catch — value of live UAT confirmed.

## Two follow-ups deferred to post-Phase-1

1. **`ApprovalRequired` should be terminal.** The gate mechanism is correct (JSON returned, loop_guard catches runaway), but the LLM interprets it as a transient error and retries. Preferred fix: return `Err(...)` from `tool_memory_reason` so the agent's error path treats it as terminal on the first attempt. Three options sketched in UAT § Criterion 6.
2. **Budget exposure endpoint.** `/api/reasoning/budget` (or fold into `/api/budget`) so warn-downgrade and block paths can be probed without daemon restarts. Currently covered by W2 unit tests on `BudgetTracker`.

Neither blocked sign-off.

## What this phase did NOT change

- The 6-layer `MemorySubstrate` public API (reasoning is a new layer above; substrate's surface is untouched).
- The msgpack BLOB read path for sessions (`get_session` unchanged).
- The 60 bundled skill source files (`git diff crates/openfang-skills/bundled/` empty).
- The 40 channel adapters, workflow engine, trigger engine, A2A/OFP, MCP wiring, dashboard.
- `openfang-cli` source beyond mechanical type-error fixes from 01-15 (`api_key` Zeroizing) and the W3.5 daemon-start fix.
- The `MeteringEngine` for per-model cost tracking. `BudgetTracker` is a separate dimension specific to reasoning.

## Next

Phase 2 — Tool Expansion — currently unscoped. Pre-Phase-2 inventory step: enumerate Hermes built-in tools and pick the ~27 highest-value additions to bring OpenFang from 23 (now 27 with Phase 1) to 50+.

---

## Sign-off

Phase 1 (Self-Learning Core) **signed off** by Dmitry Shilov, CTO ООО «Чайтекс» on **2026-06-08** per the live UAT recorded at [`01-16-UAT.md`](01-16-UAT.md). All 12 REQUIREMENTS.md success criteria addressed; two follow-up items captured for post-Phase-1 polish.
