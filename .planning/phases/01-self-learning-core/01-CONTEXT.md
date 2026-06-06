# Phase 01 — Self-Learning Core — Context

**Created:** 2026-06-06
**Status:** ready for `/gsd:plan-phase 01`
**Estimated effort:** 6-8 weeks across ~10-14 plans (final count produced by `gsd:plan-phase`)

---

## What this phase delivers

Three agent-visible capabilities, in this implementation order (revised from the original design per addendum § D):

1. **Schema v9 + FTS5 session search** — `session_search` tool. Smallest, unblocks reasoning, lowest risk. Lands first.
2. **Skill self-patching** — `skill_manage` tool with create/patch/edit/delete/write_file/remove_file/list.
3. **Memory reasoning** — `memory_reason` tool with five levels (Minimal..Max), budget control, opt-in user profile.

These are the three components originally named in `CHAITEX.md`. The reordering reflects dependencies discovered during codebase mapping: FTS5 is needed by reasoning's `Low+` levels for fact retrieval, so it ships first; skill self-patching is independent of both; reasoning lands last because it's the biggest piece.

## Source documents (load these when planning)

### Primary design (must read end-to-end)

- [`docs/chaitex/phase1-self-learning-architecture.md`](../../../docs/chaitex/phase1-self-learning-architecture.md) — 855 lines. The canonical design intent for all three components.

### Critical corrections (must read end-to-end)

- [`docs/chaitex/phase1-addendum-codebase-grounding.md`](../../../docs/chaitex/phase1-addendum-codebase-grounding.md) — corrections to the design after reading the actual code. Sections A.1, A.2, A.3, B.1, B.3, B.4 are blocking. Sections C.1–C.4 are non-blocking but important.

### Codebase baseline (skim for orientation, deep-read by area as plans need)

- [`.planning/codebase/ARCHITECTURE.md`](../../codebase/ARCHITECTURE.md) — `KernelHandle`, `AppState`, layered crate DAG, runtime/kernel boundaries.
- [`.planning/codebase/STRUCTURE.md`](../../codebase/STRUCTURE.md) — file map, "where to put new code" per crate.
- [`.planning/codebase/CONVENTIONS.md`](../../codebase/CONVENTIONS.md) — `thiserror` per crate, `tracing` with redacted `Debug` for secrets, async tokio patterns.
- [`.planning/codebase/TESTING.md`](../../codebase/TESTING.md) — hand-rolled trait fakes, `MemorySubstrate::open_in_memory`, `tempfile::tempdir`, live integration test workflow.
- [`.planning/codebase/CONCERNS.md`](../../codebase/CONCERNS.md) — every Phase-1-impacting drift point.
- [`CLAUDE.md`](../../../CLAUDE.md) — the mandatory live integration test workflow runs against the daemon after every wiring change.

### Reference docs (consult if planning brings them up)

- [`docs/architecture.md`](../../../docs/architecture.md) — upstream OpenFang architecture description.
- [`docs/skill-development.md`](../../../docs/skill-development.md) — skill format and lifecycle.
- [`docs/security.md`](../../../docs/security.md) — security model claims (verify drift in CONCERNS.md).

## Verified facts about current code (anti-design-drift anchors)

Verified against `c7d3841` HEAD. Plans must re-verify if HEAD has advanced.

- `crates/openfang-skills/src/registry.rs:14` — `SkillRegistry` struct.
- `crates/openfang-skills/src/registry.rs:55` — `snapshot()` deep-clones HashMap; `RwLockReadGuard` is `!Send` so cannot be held across `.await`.
- `crates/openfang-skills/src/registry.rs:312` — `list()` returns `Vec<&InstalledSkill>`.
- `crates/openfang-memory/src/migration.rs:8` — `SCHEMA_VERSION: u32 = 8`. Phase 1 bumps to 9.
- `crates/openfang-memory/src/migration.rs:89` — `sessions` table has `messages BLOB NOT NULL`. No `messages` row table exists.
- `crates/openfang-memory/src/session.rs:62, 83, 165, 246, 281, 386, 499` — all session message I/O uses `rmp_serde`. Every writer touchpoint must dual-write the flat table.
- `crates/openfang-runtime/src/tool_runner.rs:203` — dispatch `match tool_name` arm list. New tools insert here.
- `crates/openfang-runtime/src/tool_runner.rs:~645` — schema list. New tools insert here.
- `crates/openfang-runtime/src/agent_loop.rs:301` — `run_agent_loop` accepts `skill_registry: Option<&SkillRegistry>`. The snapshot lives in `agent_loop`; mutation signal must propagate back here.
- `Cargo.toml:58` — rusqlite features. Phase 1 adds `"fts5"`.
- `crates/openfang-memory/src/migration.rs:310` — `audit_entries` table. Reuse for skill mutation audit, do not invent a new table.

## Open decisions to settle in plans

These are recorded but not yet resolved — `/gsd:plan-phase 01` should pin them or escalate:

1. **Exact `MessageContent` flattening rules** (addendum § A.2.2). The plan author must read the real enum at `crates/openfang-types/src/message.rs` and define every arm's mapping to indexable text. The design gives the principle ("never produce empty for a non-empty message"), not the variant list.
2. **`set_skill_enabled` vs `delete`** — does enable=false hide the skill from listing/dispatch but keep the file? Design says yes. Plans must verify there is one well-defined "what does enabled=false do" semantics across the registry, the agent's view, and the marketplace.
3. **Capability flag name for skill mutation** (`capabilities.allow_skill_mutation` per X-02). Cross-check with existing `CapabilityManager` naming conventions and pick a consistent identifier.
4. **`memory_reason` for first-turn use** — if there is no history, what does Medium+ return? Caveats list, low confidence, no facts? Plan should specify.
5. **Approval mechanism for `Max` reasoning** — design says "approval required" but doesn't specify the UX. Reuse existing `ApprovalManager`? Surface in the tool result and let the agent re-call with `approved=true`? Plan must pick one.

## What this phase does NOT change

- The msgpack BLOB read path for sessions. `SessionStore::get_session()` stays unchanged.
- The 6-layer memory substrate API. Reasoning is a *new* layer above; it does not modify `MemorySubstrate`'s existing surface.
- The 60 bundled skills' source content. Defaults are applied at load time, not by mutating `skill.toml` files.
- Any channel adapter, the workflow engine, the trigger engine, A2A/OFP, MCP wiring, the dashboard.
- `openfang-cli` (user is actively developing it independently).
- The cost-tracking / MeteringEngine. Reasoning cost is a separate `BudgetTracker` to avoid coupling with existing per-model metering.

## How to use this context

`/gsd:plan-phase 01` reads this CONTEXT.md and produces a sequence of `XX-YY-<slug>-PLAN.md` files (e.g. `01-01-rusqlite-fts5-flag-PLAN.md`, `01-02-v9-migration-PLAN.md`, …). Each plan should be 2-3 tasks, complete within ~50% context.

Recommended initial wave structure (planner may revise):

| Wave | Plans | Why grouped |
|------|-------|-------------|
| W1 | 01-01 rusqlite fts5 flag · 01-02 schema v9 migration + backfill · 01-03 SessionStore dual-write · 01-04 `session_search` tool + capability gate · 01-05 v8→v9 migration test on populated DB | Pure schema/storage; one file owner each; safe to parallelize |
| W2 | 01-06 SkillRegistry mutation methods · 01-07 protected/mutable defaults + load_bundled refactor · 01-08 `skill_manage` tool · 01-09 snapshot-refresh signal in agent loop · 01-10 audit append integration | Skill self-patching; depends on W1 only via the tool registration touchpoints |
| W3 | 01-11 openfang-reasoning crate scaffold + ReasoningEngine + 5 levels · 01-12 BudgetTracker + config + deny_unknown_fields · 01-13 `memory_reason` tool + KernelHandle integration · 01-14 UserProfile (opt-in writeback) | Reasoning, depends on W1 for FTS5, on W2 for tool registration pattern |
| W4 (checkpoint) | 01-15 `human-verify` live integration test, CHANGELOG, X-03 zeroizing api_key | Verification + cross-cutting; checkpoint human-verifies all twelve success criteria from REQUIREMENTS.md |

This is a strawman — the planner may split, merge, or reorder. The shape (W1 unblocks W2/W3, W4 is the checkpoint) is load-bearing.

## Verification gates (every plan must respect)

From `CLAUDE.md`:

1. `cargo build --workspace --lib` clean.
2. `cargo test --workspace` clean (1744+ existing tests + every new one added by the plan).
3. `cargo clippy --workspace --all-targets -- -D warnings` clean.
4. For any wiring change touching API routes, kernel state, or tool registration: live integration test via the eight-step curl workflow in `CLAUDE.md`. Unit tests alone are not enough.
