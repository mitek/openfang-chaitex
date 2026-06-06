# STATE

**Updated:** 2026-06-06

## Project reference

- **Core value:** self-learning agent OS that fits on pc162 (1.9 GB ARM).
- **Current focus:** Phase 1 — Self-Learning Core (skill self-patching, FTS5 session search, memory reasoning).

## Current position

- **Phase:** 01 — Self-Learning Core
- **Plan:** 16 plans drafted across 5 waves (next step: `/gsd:execute-phase 01` starting from Wave 1)
- **Status:** plans drafted with dependency graph + file ownership locked. Ready for execution.
- **Progress:** ▓▓░░░░░░░░ 10% (project bootstrapped + codebase mapped + design + addendum + 16 plans committed; 0 of 16 plans executed; 0 of 12 phase-1 success criteria verified)

## Performance metrics

- Codebase map: 7 docs, 1769 lines, committed in `c7d3841`.
- Phase 1 design + addendum: 855 + ~450 lines, committed in `c7d3841`.
- Phase 1 plans: 16 PLAN.md files, 2337 lines, 46 tasks; wave distribution W1=5 W2=4 W3=3 W4=3 W5=1.
- Workspace state: clean `main`.

## Accumulated context

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

- Local repo: `/Users/dshilov/openfang-chaitex` on `main`.
- Daemon: not running. Live integration tests at plan 01-16 will require `openfang start` with `GROQ_API_KEY`.
- Next user action: `/gsd:execute-phase 01` to run Wave 1 (5 parallel plans: 01-01 fts5 flag + flattener, 01-05 SkillRegistry mutation, 01-06 manifest flags, 01-10 reasoning crate scaffold, 01-15 api_key zeroizing + CHANGELOG).
