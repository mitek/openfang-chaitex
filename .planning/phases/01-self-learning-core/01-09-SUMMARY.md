---
phase: 01-self-learning-core
plan: 09
status: complete
wave: 4
commit_hashes:
  - cf752cd feat(01-09): KernelHandle::fresh_skill_snapshot + kernel impl
  - cf4904c feat(01-09): ToolOutcome type + execute_tool_with_outcome wrapper
  - 14f4448 feat(01-09): wire ToolOutcome into agent_loop + snapshot refresh tests
---

# Plan 01-09 — snapshot refresh signal — SUMMARY

## What was implemented

- New `KernelHandle::fresh_skill_snapshot() -> Option<SkillRegistry>`
  trait method with default `None`; `OpenFangKernel` impl briefly
  acquires the registry read lock, calls the existing
  `SkillRegistry::snapshot()` (a synchronous deep-clone), drops the
  guard, returns the owned snapshot. Poisoned lock degrades to `None`
  with a `warn!` log rather than panic.
- New `ToolOutcome { result: ToolResult, skill_refresh_required: bool }`
  type + `execute_tool_with_outcome` wrapper in `tool_runner.rs`. The
  wrapper calls the existing `execute_tool` and parses the JSON
  `skill_refresh_required` sentinel out of the result body via
  `parse_skill_refresh_sentinel` (non-JSON results yield `false`).
- Both `run_agent_loop` and `run_agent_loop_streaming` now maintain an
  owned `Option<SkillRegistry> fresh_snapshot` inside the loop. Each
  tool dispatch borrows from `fresh_snapshot.or(skill_registry)` so a
  freshly-patched skill is visible to the very next tool call in the
  same turn. The post-dispatch branch refreshes the snapshot ONLY when
  `outcome.skill_refresh_required == true` — read-only tools stay on
  the cheap path (one boolean check per dispatch).
- Backwards compatibility: the original `execute_tool -> ToolResult`
  signature is preserved so the 15+ existing test sites and the
  `openfang-api` route do NOT need migration. Only the two agent_loop
  call sites move to `execute_tool_with_outcome`.

## Files changed (final)

- `crates/openfang-runtime/src/kernel_handle.rs`
- `crates/openfang-runtime/src/tool_runner.rs`
- `crates/openfang-runtime/src/agent_loop.rs`
- `crates/openfang-kernel/src/kernel.rs`

## Tests added

6 new tests in `crates/openfang-runtime/src/tool_runner.rs`:
1. `parse_skill_refresh_sentinel_true_when_present` — JSON with
   `"skill_refresh_required":true` returns true.
2. `parse_skill_refresh_sentinel_false_when_absent` — JSON without
   the field returns false.
3. `parse_skill_refresh_sentinel_false_when_not_json` — plain string
   returns false (graceful degradation).
4. `parse_skill_refresh_sentinel_false_when_explicit_false` — JSON
   with the field set to false returns false.
5. `agent_sees_patched_skill_in_same_turn` — end-to-end: `skill_manage
   patch` returns `ToolOutcome { skill_refresh_required: true }`; the
   subsequent `fresh_skill_snapshot()` returns a registry with the
   patched `v2` content. Asserts the snapshot counter incremented
   exactly once.
6. `read_only_tool_does_not_refresh` — `skill_manage list` returns
   `ToolOutcome { skill_refresh_required: false }`; snapshot counter
   stays at 0.

## Type changes to tool_runner return type

- `ToolOutcome` is a new struct; `execute_tool_with_outcome` is a new
  parallel function. The agent loop migrated to the new function;
  legacy `execute_tool` is unchanged.
- Timeout-path branches now construct a `ToolOutcome` with
  `skill_refresh_required: false` (timeouts are not mutations).

## Decisions made not pinned in the plan

- Kept `execute_tool` returning the plain `ToolResult` for backwards
  compatibility with the wide test and API surface. Added the typed
  `ToolOutcome` via a parallel `execute_tool_with_outcome` wrapper
  rather than changing the existing signature. Net effect for plan
  01-09 is identical (the agent loop branch is correctly typed); blast
  radius for downstream consumers is zero.
- `fresh_skill_snapshot` returns `Option` (not panic-on-poison) so a
  poisoned lock during a long run degrades to "use the existing
  snapshot" rather than aborting the turn.
- Streaming + non-streaming loops both maintain their own
  `fresh_snapshot` state — they don't share, which is fine since each
  turn runs in one variant only.

## Follow-ups for later plans

- Plan 01-14 will add `memory_conclude` as another tool — it does NOT
  emit the refresh sentinel (it persists to the user profile KV, not
  the skill registry), so no additional plumbing needed.
- Plan 01-16 live verify: agent calls `skill_manage patch` then
  immediately calls a read tool from the patched skill; the read must
  see the patched content.
