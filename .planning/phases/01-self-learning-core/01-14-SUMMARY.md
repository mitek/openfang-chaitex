---
phase: 01-self-learning-core
plan: 14
status: complete
wave: 4
commit_hashes:
  - 7d62baa feat(01-14): UserProfile types + KV load/save helpers
  - 12b3762 feat(01-14): memory_conclude tool dispatch + schema
  - 4f88cba feat(01-14): auto_update_profile writeback in memory_reason + 7 tests
---

# Plan 01-14 — UserProfile + memory_conclude — SUMMARY

## What was implemented

- New `openfang-reasoning::profile` module with the four design § 2.8
  types (`UserProfile`, `UserFact`, `Preference`, `BehavioralPattern`)
  plus a tagged `FactSource` enum (`Session | MemoryReason |
  StructuredKv`) and the load/save/mutation helpers
  (`load_profile`, `save_profile`, `add_fact`, `set_preference`,
  `add_pattern`). The pattern helper dedupes on `pattern` text and
  accumulates `occurrences` rather than appending duplicate rows.
- KV key shape: **per-agent** at `__user_profile__/<agent_uuid>`. One
  profile per agent — cross-agent isolation is the simplest correct
  default, matches how the agent's existing `structured_get/set` is
  scoped, and was confirmed by the per-agent test in
  `agent_id_str` helpers.
- `KernelHandle::memory()` accessor (default `None`); `OpenFangKernel`
  returns `Arc::clone(&self.memory)`. Allows any tool to reach the
  shared substrate without bespoke plumbing per tool.
- `tool_memory_conclude` dispatch arm + schema in `tool_runner.rs`,
  anchored with `PHASE 1 PLAN 01-14 memory_conclude` markers (disjoint
  from 01-13). Scopes writes to `caller_agent_id`; handles fact,
  preference, pattern; structured JSON errors for every failure path
  (KernelUnavailable / MemoryUnavailable / AgentIdMissing|Invalid /
  MissingField / UnknownKind / ProfileLoadFailed / ProfileSaveFailed).
- `tool_memory_reason` gained a `caller_agent_id` parameter (all 8
  test call sites updated to `None`). When
  `cfg.auto_update_profile == true` AND the answered level is `>=
  Medium`, the synthesized answer is persisted as a `UserFact` (source
  = `MemoryReason { level }`). Writeback failures are WARN-logged and
  do NOT fail the reasoning call — opt-in writeback is best-effort.
- The opt-in stays OFF by default per W2's decision (no surprise
  memory writes). The behaviors enumerated in MR-06 are all verified by
  the new tests below.

## Files changed (final)

- `crates/openfang-reasoning/src/profile.rs` (new)
- `crates/openfang-reasoning/src/lib.rs`
- `crates/openfang-runtime/src/kernel_handle.rs`
- `crates/openfang-runtime/src/tool_runner.rs`
- `crates/openfang-kernel/src/kernel.rs`

## Tests added

12 new tests total.

5 unit tests in `crates/openfang-reasoning/src/profile.rs`:
1. `load_profile_returns_default_when_missing`
2. `save_and_load_round_trip`
3. `set_preference_returns_prior_value`
4. `add_pattern_dedupes_on_text`
5. `profile_keys_are_namespaced_per_agent`

7 integration tests in `crates/openfang-runtime/src/tool_runner.rs`:
1. `memory_conclude_writes_fact_to_profile`
2. `memory_conclude_writes_preference`
3. `memory_conclude_writes_pattern`
4. `memory_reason_with_auto_update_false_does_not_write_profile` —
   default config; profile stays empty.
5. `memory_reason_with_auto_update_true_writes_after_medium` —
   `auto_update_profile=true` + Medium → 1 fact persisted with
   `FactSource::MemoryReason { level: "medium" }`.
6. `memory_reason_with_auto_update_true_does_not_write_after_low` —
   even with auto_update on, Low does NOT trigger writeback.
7. `memory_conclude_persists_across_loads` — explicit fact survives a
   second `load_profile` call.

## KV key shape decided

`__user_profile__/<agent_uuid>` — per-agent namespace. The
`structured_*` KV API in `MemorySubstrate` is already agent-scoped
(`(agent_id, key)` row), so the key prefix + agent UUID gives unique
storage and clean cross-agent isolation. Trade-off vs a single global
`__user_profile__` key: marginal storage cost (one row per agent vs
one) bought clean ownership and cleanup semantics. The new helpers
build the key via `profile_key(agent_id)` and never leak the
construction to callers.

## Decisions made not pinned in the plan

- Added a `KernelHandle::memory()` accessor (default `None`) rather
  than piping a separate substrate handle into every tool that needs
  KV access. This same hook is already used by 01-08's `skill_manage`
  pattern (`kernel_config()`, `skill_registry()`) so the trait stays
  shape-consistent.
- The `tool_memory_conclude` writes are gated on `caller_agent_id`
  being a valid UUID — missing agent ID returns an `AgentIdMissing`
  JSON error rather than silently falling back to a shared bucket.
- The reasoning-level writeback gate is `>= Medium` exactly (Medium,
  High, Max). Low and Minimal never trigger writeback even with
  `auto_update_profile=true` — Low is mechanical FTS, not a synthesis
  worth persisting as a fact.
- `add_pattern` dedupe accumulates `occurrences` (saturating_add) and
  bumps `last_seen` only — `first_seen` is preserved from the original
  entry.
- `save_profile` stamps `updated_at` to `Utc::now().to_rfc3339()` on
  every write so the dashboard can show staleness.

## Follow-ups for later plans

- Plan 01-16 live verify: agent calls `memory_conclude(kind=fact)`;
  sqlite query for `__user_profile__/<agent>` must show the JSON
  payload. Agent calls `memory_reason` with `auto_update_profile=true`
  and asserts the answer appears as a `MemoryReason`-sourced fact.
- The four new W4 tools (`session_search`, `skill_manage`,
  `memory_reason`, `memory_conclude`) are now all schema-registered
  and dispatch-wired. Wave 4 implementation is complete and ready for
  the 01-16 human-verify checkpoint with a real `GROQ_API_KEY`.
