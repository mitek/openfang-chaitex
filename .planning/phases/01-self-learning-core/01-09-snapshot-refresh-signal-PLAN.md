---
phase: 01-self-learning-core
plan: 09
type: execute
wave: 4
depends_on: [01-08]
files_modified:
  - crates/openfang-runtime/src/agent_loop.rs
  - crates/openfang-runtime/src/tool_runner.rs
autonomous: true
must_haves:
  truths:
    - "After a tool result containing `\"skill_refresh_required\": true`, the agent loop re-snapshots the `SkillRegistry` before the next tool dispatch in the same turn (SP-04, success-criterion 1)"
    - "An agent that calls `skill_manage(patch=…)` and then immediately calls another tool from the patched skill sees the patched content, not the stale snapshot (SP-04, success-criterion 2)"
    - "Read-only tool results (no sentinel) do NOT trigger a re-snapshot (no perf regression on the common path)"
  artifacts:
    - "`fn fresh_skill_snapshot(&self) -> SkillRegistry` on `KernelHandle` trait + impl"
    - "Post-process step in agent_loop.rs that parses each tool result and re-snapshots conditionally"
  key_links:
    - "Anchor comments `// === PHASE 1 PLAN 01-09 snapshot refresh ===` and `// === END PHASE 1 PLAN 01-09 ===` wrap every addition to tool_runner.rs"
    - "Sentinel parsed from `serde_json::from_str::<Value>(&result)` — must handle non-JSON results gracefully (treat as no-refresh)"
---

<objective>
Make mid-turn `skill_manage` mutations visible to the same agent turn by re-snapshotting the registry whenever a tool result carries the `skill_refresh_required: true` sentinel. This closes the registry-snapshot loop noted in addendum § B.1 and makes plan 01-08's contract complete.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-runtime/src/agent_loop.rs
@crates/openfang-runtime/src/tool_runner.rs
@crates/openfang-runtime/src/kernel_handle.rs
@crates/openfang-skills/src/registry.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: KernelHandle::fresh_skill_snapshot</name>
  <files>crates/openfang-runtime/src/kernel_handle.rs, crates/openfang-kernel/src/kernel.rs</files>
  <action>
Add to the `KernelHandle` trait:
```
/// Take a fresh snapshot of the SkillRegistry (deep-clone). Used by the
/// agent loop after a `skill_manage` mutation surfaces a
/// `skill_refresh_required` sentinel — see Phase 1 plan 01-09.
fn fresh_skill_snapshot(&self) -> openfang_skills::registry::SkillRegistry;
```
Implement on `OpenFangKernel` in `crates/openfang-kernel/src/kernel.rs` by acquiring the registry's read lock and calling its existing `snapshot()` method (CONTEXT.md anchor `registry.rs:55`). Must NOT hold the lock across an `.await` — the snapshot is a synchronous clone. If the kernel stores the registry as `Arc<RwLock<SkillRegistry>>`, lock briefly, snapshot, drop the guard, return.
  </action>
  <verify>
`cargo build -p openfang-runtime -p openfang-kernel --lib` clean.
  </verify>
  <done>
Trait method present; kernel impl returns a fresh `SkillRegistry`.
  </done>
</task>

<task type="auto">
  <name>Task 2: Post-process tool result in tool_runner.rs</name>
  <files>crates/openfang-runtime/src/tool_runner.rs</files>
  <action>
The agent loop calls into `tool_runner` per tool invocation. Inspect the return path after `result = match tool_name {...}` (around the same neighborhood as line 203). Right BEFORE the function returns, parse the result JSON to check the sentinel. Wrap inside anchor:
```
        // === PHASE 1 PLAN 01-09 snapshot refresh ===
        let skill_refresh_required = serde_json::from_str::<serde_json::Value>(&result)
            .ok()
            .and_then(|v| v.get("skill_refresh_required").and_then(|x| x.as_bool()))
            .unwrap_or(false);
        // === END PHASE 1 PLAN 01-09 ===
```
Return value: extend the function's return type to a struct or tuple `(String, bool)` where the bool is `skill_refresh_required`. If the existing signature is `Result<String, String>`, change to `Result<ToolOutcome, String>` where `ToolOutcome { result: String, skill_refresh_required: bool }`. Update every call site in `agent_loop.rs` to destructure. Keep the change minimal — non-mutation tools return `skill_refresh_required: false` and behave unchanged. Document the new type with `///` per CONVENTIONS.md.
  </action>
  <verify>
`cargo build -p openfang-runtime --lib` clean.
`cargo clippy -p openfang-runtime --all-targets -- -D warnings` clean.
  </verify>
  <done>
Tool result carries refresh sentinel out of tool_runner; type change propagated to agent_loop call sites.
  </done>
</task>

<task type="auto">
  <name>Task 3: Refresh snapshot in agent_loop + end-to-end test</name>
  <files>crates/openfang-runtime/src/agent_loop.rs</files>
  <action>
In `run_agent_loop` (signature at `agent_loop.rs:301` per CONTEXT.md), the loop holds an `Option<&SkillRegistry>`. Restructure to hold `Option<SkillRegistry>` (owned) so it can be reassigned. After each tool call: if `outcome.skill_refresh_required && let Some(kh) = kernel`, call `kh.fresh_skill_snapshot()` and replace the owned snapshot. Pass `Some(&snapshot)` to the next tool dispatch. The common-path cost is one boolean check.

End-to-end test in `mod tests`:
- `fn agent_sees_patched_skill_in_same_turn` — set up a fake kernel whose registry contains a user skill `"foo"` with content `"v1"`. Run a synthetic loop with two tool calls: (1) `skill_manage(patch, name="foo", old="v1", new="v2")` (2) a read tool that returns the skill's current content. Assert the second tool sees `"v2"`. This requires the fake kernel's `fresh_skill_snapshot()` to reflect the patch — wire it to read from the same `Arc<RwLock<SkillRegistry>>` the fake mutation writes to.
- `fn read_only_tool_does_not_refresh` — counter on the fake kernel's `fresh_skill_snapshot`; after a single read tool call, counter remains 0.
  </action>
  <verify>
`cargo test -p openfang-runtime agent_sees_patched_skill read_only_tool_does_not_refresh` runs both, green.
`cargo clippy -p openfang-runtime --all-targets -- -D warnings` clean.
  </verify>
  <done>
Mid-turn patch visible to next tool call; no-op for read-only tools.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (2+ new agent-loop tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Grep tool_runner.rs: both 01-09 anchor blocks present and disjoint from 01-08 anchors.
- Live integration (combine with 01-08): agent calls patch then list within one turn — list shows the new content.
</verification>

<success_criteria>
- [ ] `KernelHandle::fresh_skill_snapshot` exists and is implemented by `OpenFangKernel`.
- [ ] tool_runner returns the refresh sentinel via a typed outcome.
- [ ] agent_loop re-snapshots only when the sentinel is true.
- [ ] End-to-end test proves mid-turn visibility.
- [ ] No regression in existing agent_loop tests.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-09-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Type changes to `tool_runner` return type and how call sites were updated
- Any decisions made during execution that weren't pinned in this plan
- Any follow-ups for later plans
</output>
