# STATE

**Updated:** 2026-06-06

## Project reference

- **Core value:** self-learning agent OS that fits on pc162 (1.9 GB ARM).
- **Current focus:** Phase 1 — Self-Learning Core (skill self-patching, FTS5 session search, memory reasoning).

## Current position

- **Phase:** 01 — Self-Learning Core
- **Wave:** W1 complete (5/5 plans). Next step: `/gsd:execute-phase 01` resumed from W2.
- **Status:** W1 shipped — fts5/flattener, manifest mutable/protected flags, api_key Zeroizing, openfang-reasoning crate scaffold, SkillRegistry mutation surface. All workspace gates green.
- **Progress:** ▓▓▓░░░░░░░ 31% (5 of 16 plans done; W2 next: 01-02, 01-07, 01-11, 01-12)

## Performance metrics

- Codebase map: 7 docs, 1769 lines, committed in `c7d3841`.
- Phase 1 design + addendum: 855 + ~450 lines, committed in `c7d3841`.
- Phase 1 plans: 16 PLAN.md files, 2337 lines, 46 tasks; wave distribution W1=5 W2=4 W3=3 W4=3 W5=1.
- W1 execution: 11 commits (5 feat + 5 docs + 1 docs cumulative); 5 SUMMARY files, 567 lines; ~30+ new tests, ~2750 workspace tests passing.
- Workspace state: clean `main`, HEAD = `1c6c5e2`.

## Accumulated context

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
- Next user action: `/gsd:execute-phase 01` resumed from W2 — 4 parallel plans: 01-02 v9 migration + backfill + transition test, 01-07 SYSTEM_SKILLS code-level defaults, 01-11 ReasoningEngine 5-level dispatch, 01-12 BudgetTracker + `[reasoning]` config (deny_unknown_fields) + v9 amendment.
