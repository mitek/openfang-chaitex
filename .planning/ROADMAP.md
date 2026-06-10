# ROADMAP — ChaiTex fork of OpenFang

**Created:** 2026-06-06
**Owner:** Dmitry Shilov

## Overview

Three sequential phases extend OpenFang with self-learning capabilities to close the gap with Hermes Agent System without losing the pc162-viable footprint. Phase 1 lands the core self-learning loop; Phases 2-3 expand tool count and orchestration polish.

## Progress

| # | Phase | Status | Started | Completed |
|---|-------|--------|---------|-----------|
| 1 | Self-Learning Core | ✅ shipped + signed off | 2026-06-06 | 2026-06-08 |
| 1.1 | Autonomous Skill Distillation Loop (INSERTED) | in progress (5/8 plans) | 2026-06-10 | — |
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

## Phase 1.1: Autonomous Skill Distillation Loop (INSERTED)

**Slug:** `01.1-autonomous-skill-distillation-loop`
**Estimated effort:** TBD (to be locked during planning)
**Status:** EXECUTING — inserted 2026-06-10 after competitive analysis vs. Hermes Agent.

### Goal

Close the self-learning loop: Phase 1 shipped every load-bearing mechanism (`skill_manage`, FTS5 session search, leveled `memory_reason` with budget tracking, `memory_conclude`) but nothing fires them autonomously. After this phase, an OpenFang agent distills reusable skills from its own completed work, improves skills that repeatedly fail-then-recover, and consolidates session knowledge into persistent memory on a schedule — all inside the existing security gates and reasoning budget.

### Scope (three components)

1. **Post-task reflection hook** — after an agent turn completes, a cheap heuristic scores the task (tool-call count, error-recovery events, novel tool sequences, duration). Above threshold, enqueue a distillation job that runs `memory_reason` (Medium) over the session to decide whether a reusable procedure was discovered; if yes, create a draft skill via the existing `skill_manage` path (prompt-injection scan, SHA256 audit, `allow_skill_mutation` capability gate).
2. **Skill self-improvement on use** — when `skill_execute` ends in failure-then-recovery, record the delta; after N occurrences, propose a `skill_manage` patch (approval gate applies for protected skills).
3. **Memory-consolidation nudges** — kernel cron job that periodically runs `memory_conclude` over recent FTS5-indexed sessions, charged against the existing `[reasoning]` monthly budget.

### Safety rails

- Distilled skills land as **drafts requiring approval** unless agent mode is Continuous/Proactive (mirrors `allow_skill_mutation` default-off posture).
- Dedupe candidate skills against existing skills via FTS5; daily cap on distillations.
- All LLM calls go through the existing `BudgetTracker`; `budget_exceeded_action` (warn-downgrade / block) applies.

### Dependencies

Phase 1 (shipped). Purely additive wiring over `skill_manage`, `session_search`/FTS5, `memory_reason`, `memory_conclude`, the kernel cron scheduler, and `BudgetTracker`.

### Rationale

This is Hermes Agent's headline feature ("the agent that grows with you" — autonomous skill creation after complex tasks, skills that self-improve during use, periodic knowledge-persistence nudges). OpenFang would be the only implementation running the loop behind real security gates (injection scanning, Merkle audit, capability gates, hard budget ceilings).

**Plans:** 8 plans in 4 waves

Plans:
- [x] 01.1-01-PLAN.md — [distillation] config section + Phase 1.1 requirements in REQUIREMENTS.md (X-01, X-02) [wave 1] — DONE (e48dfca, fca9136)
- [x] 01.1-02-PLAN.md — TurnStats + reflection_score heuristic + AgentLoopResult.error_recovery_count (SD-01, SD-02) [wave 1] — DONE (c812f7c, dd5612e)
- [x] 01.1-03-PLAN.md — SkillFailureTracker (bounded, TTL-decaying) (SI-01, SI-03) [wave 1] — DONE (cb59887)
- [x] 01.1-04-PLAN.md — Draft-skill lifecycle helpers + dedupe in SkillRegistry (SD-05, SD-06) [wave 1] — DONE (13a2e1c)
- [ ] 01.1-05-PLAN.md — DistillationQueue + worker + post-turn hook + daily-cap sidecar (SD-02, SD-03, SD-04) [wave 2]
- [x] 01.1-06-PLAN.md — Skill-failure recording + patch proposals + memory-consolidation nudge (SI-01, SI-02, MC-01, MC-02) [wave 2] — DONE (066b75c, 5c2a0ff, f05b240)
- [ ] 01.1-07-PLAN.md — /api/distillation/drafts list + approve endpoints (X-03) [wave 3]
- [ ] 01.1-08-PLAN.md — Workspace gates + CHANGELOG + live integration UAT (X-04, X-05, X-06) [wave 4]

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
