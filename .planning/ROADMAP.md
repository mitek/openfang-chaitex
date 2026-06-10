# ROADMAP — ChaiTex fork of OpenFang

**Created:** 2026-06-06
**Owner:** Dmitry Shilov

## Overview

Three sequential phases extend OpenFang with self-learning capabilities to close the gap with Hermes Agent System without losing the pc162-viable footprint. Phase 1 lands the core self-learning loop; Phases 2-3 expand tool count and orchestration polish.

## Progress

| # | Phase | Status | Started | Completed |
|---|-------|--------|---------|-----------|
| 1 | Self-Learning Core | ✅ shipped + signed off | 2026-06-06 | 2026-06-08 |
| 2 | Tool Expansion | not started | — | — |
| 3 | Coordination | not started | — | — |

---

## Phase 1 — Self-Learning Core

**Slug:** `01-self-learning-core`
**Estimated effort:** 6-8 weeks
**Actual:** 2 days of focused execution (2026-06-06 → 2026-06-08), aided by parallel planning + executor agents
**Status:** ✅ **SHIPPED + SIGNED OFF** 2026-06-08. UAT at [`phases/01-self-learning-core/01-16-UAT.md`](phases/01-self-learning-core/01-16-UAT.md). 12/12 success criteria addressed.

### Goal

An agent running on OpenFang can improve its own skills, search its entire conversation history full-text, and reason actively about its user — within a configured monetary budget — without compromising pc162 footprint, the 16 existing security subsystems, or any of the 60 bundled skills.

### Dependencies

None. Phase 1 is purely additive over current `main` (v0.6.9).

### Requirements covered

SP-01..05 (skill self-patching), FTS-01..06 (FTS5 session search), MR-01..08 (memory reasoning + budget), X-01..06 (cross-cutting tool registration, security, CHANGELOG, live integration). Full traceability in [`REQUIREMENTS.md`](REQUIREMENTS.md).

### Success criteria

The twelve goal-backward criteria in [`REQUIREMENTS.md`](REQUIREMENTS.md#mapping-to-phase-1-success-criteria-goal-backward) must all hold. Summary:

1. Skill creation visible in same turn (snapshot refresh works).
2. Skill patching produces correct file diff + dispatch updates.
3. Protected skills reject mutation with structured error.
4. Session search returns FTS5 results <500ms on pc162.
5. Medium-level reasoning returns synthesized answer + supporting facts.
6. Max-level reasoning gated behind approval flag.
7. Budget exceeded triggers configured downgrade or block.
8. `cargo build / test / clippy -D warnings` all pass, including v8→v9 migration test.
9. Live curl-based integration test per `CLAUDE.md` passes for all three tools.
10. Bundled skills load with correct `mutable`/`protected` defaults via code, not build scripts.
11. Reasoning config typo errors loud; default-load logged with explicit marker.
12. CHANGELOG entry with schema-v9 backward-compat note.

### Phase artifacts (target locations)

```
.planning/phases/01-self-learning-core/
├── 01-CONTEXT.md         # bootstrapped now (this commit)
├── 01-PLAN.md            # produced by /gsd:plan-phase 01
├── 01-RESEARCH.md        # optional, only if /gsd:research-phase 01 is invoked
├── 01-VERIFICATION.md    # produced by /gsd:verify-work 01 after execution
├── 01-UAT.md             # produced after live integration testing
└── 01-SUMMARY.md         # written on phase completion
```

### Risks / open decisions

Tracked in [`01-CONTEXT.md`](phases/01-self-learning-core/01-CONTEXT.md). Top three:
- FTS5 backfill performance on pc162 SD card (mitigation: `fts_backfill = "lazy"` config escape).
- LLM call latency for reasoning on a slow-connection pc162 (mitigation: Minimal/Low levels skip LLM entirely, Max requires explicit user approval).
- Single `Arc<Mutex<Connection>>` contention as session_search adds read traffic (mitigation: measure first; connection-pool refactor is Phase 1.5 or 2 candidate, not Phase 1).

---

## Phase 1.1 — Autonomous Skill Distillation Loop (INSERTED 2026-06-10)

**Slug:** `01.1-autonomous-skill-distillation-loop`
**Estimated effort:** 2-3 weeks
**Status:** EXECUTING — 1/8 plans complete (01.1-01 done: DistillationConfig + requirements)

### Goal

Close the self-learning loop with autonomy wiring: post-task reflection → skill distillation, skill self-improvement on failure-then-recovery, cron-driven memory consolidation nudge. All loops run behind existing security gates and budget ceilings. Motivated by competitive analysis vs. Hermes Agent ("closed learning loop" headline feature); OpenFang differentiator is running the loop on existing infrastructure without new deps.

### Requirements

Defined in `REQUIREMENTS.md § Phase 1.1`. IDs: SD-01..06, SI-01..03, MC-01..02, X-01..02.

### Plan progress

| Plan | Name | Status |
|------|------|--------|
| 01.1-01 | DistillationConfig + Phase 1.1 requirements | DONE (e48dfca, fca9136) |
| 01.1-02 | TurnStats + reflection scorer | not started |
| 01.1-03 | DistillationJob queue + worker scaffold | not started |
| 01.1-04 | Daily cap + distillation_state.json sidecar | not started |
| 01.1-05 | Draft skill creation via create_skill path | not started |
| 01.1-06 | SkillFailureTracker + SI loop | not started |
| 01.1-07 | Memory consolidation nudge background task | not started |
| 01.1-08 | Live integration + verification | not started |

### Dependencies

Phase 1 complete (ReasoningEngine, BudgetTracker, create_skill, ApprovalManager, memory_conclude all shipped).

---

## Phase 2 — Tool Expansion

**Slug:** `02-tool-expansion`
**Estimated effort:** 4-8 weeks
**Status:** not started — scope to be locked after Phase 1 lands.

### Goal

Increase built-in tool count from 23 to 50+, with auto-discovery so new tools register without touching three files. Add cron chaining (`context_from`) to enable multi-step scheduled workflows.

### Dependencies

Phase 1 (so we know which Hermes tools the agent actually needs first, informed by real reasoning usage).

### Open scoping question

Which 27+ tools to add is not yet specified. Pre-Phase-2 work: tool inventory of Hermes against current OpenFang built-ins to pick the high-value 27.

---

## Phase 3 — Coordination

**Slug:** `03-coordination`
**Estimated effort:** 4-6 weeks
**Status:** not started.

### Goal

Batch agent delegation (up to 3 parallel `agent_spawn`), orchestration polish — improvements to the existing workflow engine + trigger engine informed by Phase 1-2 usage.

### Dependencies

Phase 1 + Phase 2.
