---
gsd_state_version: 1.0
milestone: v0.6.9
milestone_name: milestone
status: completed
last_updated: "2026-06-10T05:49:13.340Z"
progress:
  total_phases: 2
  completed_phases: 1
  total_plans: 16
  completed_plans: 17
  percent: 100
---

# STATE

**Updated:** 2026-06-06

## Project reference

- **Core value:** self-learning agent OS that fits on pc162 (1.9 GB ARM).
- **Current focus:** Phase 1 — Self-Learning Core (skill self-patching, FTS5 session search, memory reasoning).

## Current position

- **Phase:** 01.1 — Autonomous Skill Distillation Loop — **IN PROGRESS**
- **Plan:** 01.1-03 COMPLETE (2026-06-10). 01.1-01/02 handled by parallel agents.
- **Status:** Phase 1 100% complete + signed off. Phase 1.1 distillation loop now in execution. Plan 01.1-03 shipped: SkillFailureTracker (DashMap-backed, 20-event bounded, 7-day TTL-decaying, per-(skill,agent) key concurrent tracker) — the recording substrate for SI-01/SI-03.
- **Progress:** [██████████] 100% (Phase 1 done); Phase 1.1 partial (plans 01-03 executing in parallel waves)

## Performance metrics

- Codebase map: 7 docs, 1769 lines, committed in `c7d3841`.
- Phase 1 design + addendum: 855 + ~450 lines, committed in `c7d3841`.
- Phase 1 plans: 16 PLAN.md files, 2337 lines, 46 tasks; wave distribution W1=5 W2=4 W3=3 W4=3 W5=1.
- W1 execution: 11 commits + STATE close; 5 SUMMARY files, 567 lines; ~30+ new tests.
- W2 execution: 12 commits + STATE close; 4 SUMMARY files; +24 net new tests (→ 2774).
- W3 execution: 8 commits + STATE close; 3 SUMMARY files; +34 net new tests (→ 2808).
- W3.5 (loud-degrade): 2 commits; criterion 11 closed; +5 net new tests (→ 2813).
- W4 execution: 11 commits; 3 SUMMARY files; +21 net new tests (→ 2834).
- Workspace state: clean `main`, HEAD = `64e5524`.

## Accumulated context

### Decisions made during W4 execution (2026-06-08)

- **`SessionStore::search_sessions_fts` is the canonical FTS5 query path.** Plan 01-04 lifted the SQL from `openfang-reasoning::fact_retrieval::fts5_session_search`; `fact_retrieval` now delegates to the memory-crate method. Single source of truth for FTS5 BM25 + snippet rendering. The `SessionSearchHit` struct lives in `openfang-memory`.
- **`execute_tool -> ToolResult` signature preserved**, parallel `execute_tool_with_outcome -> ToolOutcome` wrapper added by plan 01-09. The agent loop migrated to the new wrapper; ~15 existing test sites and the `openfang-api` route continue using the legacy signature with no behavior change. Zero downstream blast radius. Pragmatic deviation from the plan's literal "promote at all call sites" wording.
- **`KernelHandle::fresh_skill_snapshot()` added** in plan 01-09 as the snapshot refresh entry point. Subscribes to `kernel.skill_updated_tx` broadcast (wired by 01-08) and consumes the `"skill_refresh_required": true` JSON sentinel from `skill_manage` results. The agent loop calls it post-tool before the next dispatch.
- **`tool_memory_reason` signature gained `caller_agent_id: Option<&str>`** so 01-14's `auto_update_profile` writeback hook knows whose profile to update. Updated all 8 existing test call sites with `None`. Mechanical.

### Decisions made during W3 execution (2026-06-07)

- **`skill_manage` action list grew from 6 to 8** — plan 01-08 listed 7 actions (`create`, `patch`, `edit`, `delete`, `write_file`, `remove_file`, `list`); plan 01-05 had only delivered 6 mutation methods (no `delete_skill` / `remove_skill_file`). Added both as wrappers in `openfang-skills::registry` honoring `check_mutable` per the Protected/Immutable invariants. The full action set in the tool is now 7 + `list`.
- **`KernelCapabilities` sub-struct created on `KernelConfig`** — no such struct existed; added with a single field `allow_skill_mutation: bool` (default `false`). Wired into `Default` impl. `[capabilities]` is the new config section.
- **`KernelHandle::complete` does not exist; introduced `KernelLlm` trait** in `openfang-reasoning` instead — matches the trait-extension pattern from 01-05 (`AuditAppend`, `SkillEventBus`). `OpenFangKernel` implements `KernelLlm`. `openfang-runtime` gained `openfang-reasoning` as a direct dep so trait signatures can name public types. Crate DAG stays acyclic.
- **`ReasoningLlm::synthesize_with_usage` default method added** so real `TokenUsage` can flow without breaking existing test impls. The engine reads `usage.total() > 0` to pick the real-token path vs the `chars/4` coarse heuristic.
- **`kernel.skill_updated_tx: broadcast::Sender<SkillUpdated>`** is wired and emits on every mutation. The agent loop subscribes in W4 plan 01-09 — for W3 the `skill_refresh_required: true` sentinel ships as an in-band JSON field on the tool result.

### Decisions made during W2 execution (2026-06-07)

- **`create_skill` is exempt from `check_mutable`.** Plan 01-05 wrote `self.check_mutable(name, "create")?` as the first line of `create_skill`. That was safe with the stub returning `Ok(())`, but once 01-07 promoted `check_mutable` to a real `NotFound`-on-missing-skill body, every `create_skill` call would fail (the skill doesn't exist yet — that's the whole point). The 01-07 plan explicitly notes `create_skill` is mutation-by-definition and shouldn't pre-check; removed the call and restored 16 pre-existing 01-05 mutation tests.
- **`openfang-reasoning` now has `rusqlite` as a direct workspace dep** (narrow — only for the `params!` macro in BudgetTracker SQL). `openfang-kernel` gained an `openfang-reasoning` dep for the boot-time effective-config logger.
- **`engine_reason_returns_not_yet_implemented` smoke test replaced** by `engine_reason_minimal_smoke_test` — the 01-10 stub is now real per 01-11.
- **`BudgetRecord::new_now`, `monthly_budget_usd()` accessor, `format_effective_log`** added as a separately-testable BudgetTracker surface (Rule 2 — the privacy clamp + the boot-logger output now have direct tests, can't be bypassed accidentally).
- **MR-05 success-criterion 11 — DECIDED 2026-06-07: loud-degrade.** Closed in commit `5e98e7b` (W3.5). `ConfigStatus::{Ok, Degraded}` + `load_config_with_status` added to `crates/openfang-kernel/src/config.rs`. On read/parse/deser failure of an *existing* config file: ERROR-level tracing log + stderr banner + `kernel.config_status = Degraded`. Surfaced on `/api/health` (`config_status: "ok" | "degraded"`) and `/api/health/detail` (full error + source path). Daemon still boots on defaults so the operator can recover via the API. Missing config file = `Ok` (defaults intentional). Closes `TODO(GAP-012-Tier-2)` from CONCERNS.md. Backward-compat shim `load_config(...) -> KernelConfig` preserved. Criterion 11 in REQUIREMENTS.md updated to the observable behavior; new X-07 cross-cutting REQ documents the contract.
- **Multiple clippy fixes inline**: `field_reassign_with_default` → struct-update form; useless `format!()` → raw string literal; `manual_flatten` → `.flatten()`; `manual_div_ceil` → `.div_ceil(4)`. All from new W2 code paths.

### Decisions made during W1 execution (2026-06-07)

- **rusqlite 0.31 has no `fts5` Cargo feature.** That feature only exists in rusqlite ≥0.32 and gates the Rust-side custom-tokenizer API — not the FTS5 virtual table itself. FTS5 virtual tables work today via `libsqlite3-sys` bundled build (`-DSQLITE_ENABLE_FTS5`). `Cargo.toml` is unchanged from before W1 (apart from the `openfang-reasoning` workspace member addition from 01-10). FTS-01 satisfied by runtime `fts5_is_compiled_in` probe test. If any later plan needs the Rust tokenizer API, that's a separate rusqlite major bump — none of plans 01-02/03/04 need it (raw SQL is sufficient).
- **`Role::Tool` does not exist** in `openfang-types::message::Role`. Only three variants: `User|Assistant|System`. Tool results are carried as `ContentBlock::ToolResult` inside a user-role message. `role_string` covers the three real variants. Plan documents updated accordingly.
- **`openfang-memory` does not depend on `openfang-runtime::str_utils`.** Inlined a 6-line `safe_truncate` helper in `session_fts.rs` rather than introduce a cross-crate dep.
- **`serde_json::Value::Object` is `BTreeMap`-backed** in this workspace (no `preserve_order` feature). `serde_json::to_string` is byte-stable across runs for the same `Value`. `flatten_is_deterministic_across_runs` test pins this invariant.
- **`zeroize/serde` feature has too wide a blast radius** for plan 01-15 alone. Added a 10-line `zeroizing_string` serde adapter scoped to `KernelConfig.api_key` instead. Workspace-wide opt-in is a separate decision if it ever matters.
- **`SkillRegistry` hand-written `Debug` and `Clone`** because adding `Option<Arc<dyn Trait>>` fields broke `#[derive(Debug)]`. Pattern repeats anywhere we accept trait-object handles.
- **`SkillRegistry::list_all()` added** alongside spec'd `list()`. `list()` filters on enabled-ness (mutation surface needs this); `list_all()` is the explicit dashboard accessor when "show disabled" is needed.
- **`ReasoningEngine.memory` field marked `#[allow(dead_code)]`** in 01-10 because the field is held for plan 01-11's dispatch body. Doc comment explains why. `has_llm()` accessor added so tests don't reach into private fields.

### Decisions made during planning (2026-06-06)

- **`SkillManifest` lives in `crates/openfang-skills/src/lib.rs`**, not `manifest.rs` — the design's path reference was wrong. Plan 01-06 explicitly handles this.
- **`reasoning_budget` SQLite table is a v9 migration amendment from plan 01-12**, not its own migration. Two W2 plans (01-02, 01-12) touch `migrate_v9` body — coordinated by anchor comment `// === v9 amendment: reasoning_budget (plan 01-12) ===`.
- **`ToolOutcome { result, skill_refresh_required }` typed return** added by plan 01-08 to `tool_runner.rs`; plan 01-09 updates call sites in `agent_loop.rs`.
- **`KernelHandle` gains `reasoning_engine`, `budget_tracker`, `reasoning_config` accessors** in plan 01-13; plan 01-14 reuses them (must not re-add).
- **`ReasoningEngine::reason` is stubbed in plan 01-10** returning `ReasoningError::NotYetImplemented` so plan 01-11 has a real type to fill in while wave-1 ships independently.
- **Tool-runner anchor format**: `// === PHASE 1 PLAN 01-NN <tool_name> ===` / `// === END PHASE 1 PLAN 01-NN ===` per plan, separate `=== schema ===` block for the JSON schema list entry. Gives 2 disjoint anchor pairs per plan in `tool_runner.rs` so parallel-wave plans don't merge-conflict.
- **`ReasoningLlm` is implemented by `KernelLlmAdapter`** in plan 01-13 wrapping a `Weak<KH>` — no cycles, no new HTTP client, reuses the agent's existing LLM provider via `KernelHandle`.

### Original decisions (carryover)

- **Fork over port.** OpenFang is extended, Hermes is not ported. (`docs/chaitex/hermes-on-pc162-rust-port.md`)
- **No new database.** SQLite + bundled FTS5 only.
- **No new runtime crate for skill patching.** Extend `openfang-skills` in place.
- **New crate for reasoning.** `openfang-reasoning` to keep memory crate boundaries clean.
- **Reasoning reuses agent's LLM via KernelHandle.** No second LLM driver/config.
- **Mid-turn skill mutation visibility:** snapshot-refresh-on-mutation-signal (addendum § B.1 option 2).
- **Sessions stay msgpack BLOB on read path.** FTS5 is added via a parallel flat table populated by dual-write (addendum § A.2).
- **Protected/mutable defaults from code, not build scripts.** `SYSTEM_SKILLS` array in `SkillRegistry::load_bundled` (addendum § B.4).
- **Profile auto-update is opt-in.** `reasoning.auto_update_profile = false` by default (addendum § C.1).
- **`[reasoning]` section uses `deny_unknown_fields`.** Closes silent-default trap noted in `.planning/codebase/CONCERNS.md`.

### Todos (not yet planned)

- Tool dispatch refactor to `Tool` trait — explicitly out of scope for Phase 1, candidate for Phase 1.5 or Phase 2.
- Connection-pool migration (`r2d2_sqlite`) to relieve `Arc<Mutex<Connection>>` contention — measure first.
- Stream-based FTS5 backfill (incremental) for very large existing databases — only if profiling shows naive backfill is too slow on pc162.

### Blockers

None.

## Session continuity

- Local repo: `/Users/dshilov/openfang-chaitex` on `main`, HEAD = `1c6c5e2`.
- Daemon: not running. Live integration tests at plan 01-16 will require `openfang start` with `GROQ_API_KEY`.
- W1 commits (chronological, oldest first):
  - `15d6a1a feat(01-01): session_fts module with stable flattener + fts5 probe`
  - `9f2c0ed docs(01-01): complete plan — session_fts module with stable flattener + fts5 probe`
  - `1d74203 feat(01-06): mutable/protected manifest flags on SkillMeta`
  - `4502d21 docs(01-06): complete plan — skill manifest mutable/protected flags`
  - `f472d06 feat(01-15): zeroize KernelConfig.api_key on drop`
  - `17d1e1b docs(01-15): changelog entry for phase 1`
  - `b60f030 docs(01-15): complete plan — api_key zeroizing + changelog entry`
  - `0f76e82 feat(01-10): openfang-reasoning crate scaffold`
  - `aba7072 docs(01-10): complete plan — openfang-reasoning crate scaffold`
  - `3d49155 feat(01-05): SkillRegistry mutation surface (six methods + audit + events)`
  - `1c6c5e2 docs(01-05): complete plan — SkillRegistry mutation surface`
- W2 commits (chronological):
  - `2d20a5e feat(01-02): schema v9 — session_messages + FTS5 + backfill`
  - `8668e74 test(01-02): v8 → v9 transition tests on populated DB`
  - `97cd5a0 docs(01-02): complete plan — schema v9 + FTS5 + backfill`
  - `543aca2 feat(01-12): BudgetTracker + [reasoning] config with deny_unknown_fields`
  - `84202b0 feat(01-12): reasoning_budget table — v9 amendment`
  - `13e21d8 feat(01-12): BudgetTracker + boot-time effective-config log`
  - `9232f5e docs(01-12): complete plan — BudgetTracker + [reasoning] config`
  - `e951c52 feat(01-07): SYSTEM_SKILLS const + apply_load_time_defaults + check_mutable body`
  - `e44e26b docs(01-07): complete plan — protected/mutable defaults + check_mutable`
  - `e1bab4d feat(01-11): fact_retrieval — multi-source retrieval per level`
  - `4d80a73 feat(01-11): ReasoningEngine — 5-level dispatch + first-turn caveat`
  - `fc4cc44 docs(01-11): complete plan — ReasoningEngine level dispatch`
- W3 commits (chronological):
  - `327448d feat(01-03): SessionStore.save_session dual-writes session_messages`
  - `249b5c9 docs(01-03): complete plan — SessionStore.save_session dual-writes session_messages`
  - `950e53a feat(01-08): skill_manage tool + kernel adapters`
  - `928791d docs(01-08): complete plan — skill_manage tool + kernel adapters`
  - `51c0d5b feat(01-13): KernelLlmAdapter`
  - `1260b07 feat(01-13): wire BudgetTracker pre/post-call onto ReasoningEngine`
  - `7b39b72 feat(01-13): memory_reason tool`
  - `0c79d1d docs(01-13): complete plan — memory_reason tool + KernelLlmAdapter`
- W4 commits (chronological):
  - `1ceaddb feat(01-04): SessionStore.search_sessions_fts + lift reasoning FTS`
  - `c1a15dd feat(01-04): session_search tool dispatch + schema`
  - `cac8457 docs(01-04): complete plan — session_search tool + SessionStore.search_sessions_fts`
  - `cf752cd feat(01-09): KernelHandle::fresh_skill_snapshot + kernel impl`
  - `cf4904c feat(01-09): ToolOutcome type + execute_tool_with_outcome wrapper`
  - `14f4448 feat(01-09): wire ToolOutcome into agent_loop + snapshot refresh tests`
  - `fb4f804 docs(01-09): complete plan — ToolOutcome + snapshot refresh signal in agent_loop`
  - `7d62baa feat(01-14): UserProfile types + KV load/save helpers`
  - `12b3762 feat(01-14): memory_conclude tool dispatch + schema`
  - `4f88cba feat(01-14): auto_update_profile writeback in memory_reason + 7 tests`
  - `64e5524 docs(01-14): complete plan — UserProfile + memory_conclude tool + opt-in writeback`
- **Phase 1 SIGNED OFF** (2026-06-08, Dmitry Shilov). UAT recorded at [`.planning/phases/01-self-learning-core/01-16-UAT.md`](phases/01-self-learning-core/01-16-UAT.md).
- Two follow-ups for post-Phase-1 polish (not blocking):
  - **ApprovalRequired terminal-state UX** — `tool_memory_reason` should return `Err(...)` (or `is_error: true`) instead of `Ok(json_with_error_field)` so the agent's loop guard catches it on the first attempt and the structured cost prompt surfaces to the user instead of "Max iterations exceeded".
  - **Budget exposure endpoint** — add `/api/reasoning/budget` (or fold into `/api/budget`) so warn-downgrade and block paths can be probed live without daemon restarts.
- **Next phase: 02 — Tool Expansion** (currently unscoped). Pre-Phase-2 work: inventory Hermes built-in tools against current OpenFang built-ins to pick the ~27 highest-value additions. See [`ROADMAP.md`](ROADMAP.md#phase-2--tool-expansion).
