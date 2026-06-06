---
phase: 01-self-learning-core
plan: 08
type: execute
wave: 3
depends_on: [01-07]
files_modified:
  - crates/openfang-runtime/src/tool_runner.rs
  - crates/openfang-types/src/config.rs
autonomous: true
must_haves:
  truths:
    - "Agent calls `skill_manage(action=\"create\", name=\"my-skill\", ...)`; new skill written to disk + registered in registry (SP-01, success-criterion 1)"
    - "Agent calls `skill_manage(action=\"patch\", name=\"my-skill\", old_string, new_string)` on a mutable skill → file diff verified on disk (SP-01, success-criterion 2)"
    - "Agent calls `skill_manage(action=\"patch\", name=\"memory-core\", ...)` returns a structured error mentioning `ProtectedSkill`; no disk mutation (SP-03, success-criterion 3)"
    - "Every mutation action returns a JSON result that contains `\"skill_refresh_required\": true` (consumed by plan 01-09) (SP-04)"
    - "When `capabilities.allow_skill_mutation == false`, mutation actions return a `CapabilityDenied` JSON error; `list` always works (X-02)"
  artifacts:
    - "`async fn tool_skill_manage(input: serde_json::Value, kernel: &dyn KernelHandle) -> Result<String, String>` in tool_runner.rs"
    - "`pub allow_skill_mutation: bool` (default `false`) field on the `[capabilities]` config struct"
  key_links:
    - "Anchor comments `// === PHASE 1 PLAN 01-08 skill_manage ===` and `// === END PHASE 1 PLAN 01-08 ===` wrap every addition to tool_runner.rs so parallel-wave plans 01-09/01-13/01-14 do not merge-conflict"
    - "`skill_refresh_required: true` sentinel is the contract plan 01-09 reads to trigger snapshot refresh in the agent loop"
---

<objective>
Wire `skill_manage` as a built-in tool dispatched by the agent loop. This is the user-visible top of the skill self-patching feature.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-runtime/src/tool_runner.rs
@crates/openfang-types/src/config.rs
@crates/openfang-runtime/src/kernel_handle.rs
@crates/openfang-skills/src/registry.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Capability flag in KernelConfig</name>
  <files>crates/openfang-types/src/config.rs</files>
  <action>
Locate the capabilities sub-struct on `KernelConfig` (or wherever existing `capabilities.*` bools live — search for `pub struct.*Capabilit`). Add field:
```
#[serde(default)]
pub allow_skill_mutation: bool,
```
Add the matching `Default` entry (Default impl is mandatory per CLAUDE.md Common Gotchas — build breaks otherwise). Default is `false`. Serialize/Deserialize derives already in place; nothing else to add. This addition is line-disjoint from plan 01-12's `[reasoning]` block and plan 01-15's `api_key` change.
  </action>
  <verify>
`cargo build -p openfang-types --lib` clean.
`cargo test -p openfang-types` clean.
A TOML literal `[capabilities]\nallow_skill_mutation = true` deserializes to the field being true.
  </verify>
  <done>
Field exists with `#[serde(default)]`; Default impl populated; round-trip test green.
  </done>
</task>

<task type="auto">
  <name>Task 2: tool_skill_manage dispatch + schema</name>
  <files>crates/openfang-runtime/src/tool_runner.rs</files>
  <action>
Around line 203 (dispatch `match`), insert with anchor comments verbatim:
```
        // === PHASE 1 PLAN 01-08 skill_manage ===
        "skill_manage" => tool_skill_manage(input, kernel).await,
        // === END PHASE 1 PLAN 01-08 ===
```
Implement `async fn tool_skill_manage(input: Value, kernel: &dyn KernelHandle) -> Result<String, String>`. Parse `input["action"]` as `String`; allowed values `create | patch | edit | delete | write_file | remove_file | list`. For every non-`list` action: read `kernel.config().capabilities.allow_skill_mutation`; if false return `Ok(json!({"error": "CapabilityDenied", "tool": "skill_manage", "hint": "Set capabilities.allow_skill_mutation = true in config.toml"}).to_string())`. Dispatch each action to the corresponding `SkillRegistry` method from plan 01-05 (call via a kernel-side accessor — add `KernelHandle::skill_registry_mut() -> Arc<RwLock<SkillRegistry>>` if missing, or use whatever pattern kernel.rs already exposes). On success, every mutation action returns:
```
json!({
    "action": "<action>",
    "name": "<name>",
    "ok": true,
    "skill_refresh_required": true
})
```
`list` returns `json!({"action":"list","skills":[...]})` with name+mutable+protected+enabled fields, no refresh sentinel. Errors from the registry (`SkillError::Protected{..}` etc) map to a JSON `{ "error": "<variant>", ... }` payload using the existing Display impl, NOT a tool-runner Err — agents need structured error info, not a Rust panic.

Around line 645 (schema list), insert:
```
        // === PHASE 1 PLAN 01-08 skill_manage schema ===
        json!({
            "name": "skill_manage",
            "description": "Manage skills (create, patch, edit, delete, write_file, remove_file, list). Skills are procedural memory — reusable approaches for recurring tasks.",
            "input_schema": { "type": "object", "properties": {
                "action": { "type": "string", "enum": ["create","patch","edit","delete","write_file","remove_file","list"] },
                "name": { "type": "string" },
                "content": { "type": "string" },
                "prompt_context": { "type": "string" },
                "old_string": { "type": "string" },
                "new_string": { "type": "string" },
                "replace_all": { "type": "boolean" },
                "file_path": { "type": "string" },
                "file_content": { "type": "string" },
                "category": { "type": "string" },
                "mutable": { "type": "boolean", "description": "For create: pin new skill as mutable (default true)" }
            }, "required": ["action"] }
        }),
        // === END PHASE 1 PLAN 01-08 skill_manage schema ===
```
(`name` is only required for non-list actions; the schema allows omission for `list`. Enforce in code.)
  </action>
  <verify>
`cargo build -p openfang-runtime --lib` clean.
`cargo clippy -p openfang-runtime --all-targets -- -D warnings` clean.
  </verify>
  <done>
Dispatch + schema landed inside anchors; capability gate enforced.
  </done>
</task>

<task type="auto">
  <name>Task 3: Tool-level unit tests via a fake KernelHandle</name>
  <files>crates/openfang-runtime/src/tool_runner.rs</files>
  <action>
Add to the existing tool_runner test module (or create one):
- `skill_manage_list_returns_skills` — fake kernel with a registry containing 1 user skill + 1 system skill; assert returned JSON has both names with correct protected flags.
- `skill_manage_create_calls_registry_create_skill` — fake kernel records the call; assert `create_skill` invoked with the right args.
- `skill_manage_create_sets_skill_refresh_required` — assert the result JSON contains `"skill_refresh_required": true`.
- `skill_manage_patch_on_protected_returns_structured_error` — fake registry returns `SkillError::Protected{..}`; result JSON contains `"error": "Protected"`.
- `skill_manage_blocked_when_capability_off` — `allow_skill_mutation=false`; result JSON contains `"error": "CapabilityDenied"`.
- `skill_manage_list_works_with_capability_off` — list still returns the registry contents.
- `skill_manage_create_accepts_optional_mutable_param` — `input["mutable"]=true`, assert the `create_skill` mock saw `Some(true)`.
Use hand-rolled trait fakes per TESTING.md "What to mock".
  </action>
  <verify>
`cargo test -p openfang-runtime skill_manage` runs ≥ 7 new tests.
`cargo clippy -p openfang-runtime --all-targets -- -D warnings` clean.
  </verify>
  <done>
≥ 7 unit tests proving dispatch correctness + capability gate.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Grep tool_runner.rs: both anchor blocks present.
- TOML probe: `[capabilities]\nallow_skill_mutation = true` parses cleanly.
</verification>

<success_criteria>
- [ ] `skill_manage` dispatchable from the agent's tool loop.
- [ ] `allow_skill_mutation` capability flag exists; default false.
- [ ] Every mutation result JSON contains `"skill_refresh_required": true`.
- [ ] Protected skill returns `Protected` JSON error with no disk write.
- [ ] `list` action works regardless of capability flag.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-08-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Any decisions made during execution that weren't pinned in this plan (kernel handle accessor shape, etc.)
- Any follow-ups for later plans (notably the snapshot refresh wiring in 01-09)
</output>
