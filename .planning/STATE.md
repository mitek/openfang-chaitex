# STATE

**Updated:** 2026-06-06

## Project reference

- **Core value:** self-learning agent OS that fits on pc162 (1.9 GB ARM).
- **Current focus:** Phase 1 — Self-Learning Core (skill self-patching, FTS5 session search, memory reasoning).

## Current position

- **Phase:** 01 — Self-Learning Core
- **Plan:** not yet drafted (next step: `/gsd:plan-phase 01`)
- **Status:** phase registered in ROADMAP, context bundled, requirements traced. Ready for plan generation.
- **Progress:** ▓░░░░░░░░░ 5% (project bootstrapped + codebase mapped + design + addendum committed; 0 of 12 phase-1 success criteria verified)

## Performance metrics

- Codebase map: 7 docs, 1769 lines, committed in `c7d3841`.
- Phase 1 design + addendum: 855 + ~450 lines, committed.
- Workspace state: clean `main`, on commit `c7d3841`.

## Accumulated context

### Decisions made

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
- Daemon: not running. Live integration tests will require `openfang start` with `GROQ_API_KEY`.
- Recent commits: `c7d3841 chaitex: codebase map + Phase 1 addendum` (HEAD), `6675c16 chaitex: mutable flag / migration / budget`, `99efdc8 chaitex: Phase 1 design`.
- Next user action: `/gsd:plan-phase 01` to produce the per-plan task breakdown.
