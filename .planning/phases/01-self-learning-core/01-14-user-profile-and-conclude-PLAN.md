---
phase: 01-self-learning-core
plan: 14
type: execute
wave: 4
depends_on: [01-13]
files_modified:
  - crates/openfang-runtime/src/tool_runner.rs
files_created:
  - crates/openfang-reasoning/src/profile.rs
autonomous: true
must_haves:
  truths:
    - "Agent calls `memory_conclude(kind=\"fact|preference|pattern\", ...)` and the entry persists to the `__user_profile__` key in structured KV (MR-06)"
    - "`memory_reason` with `auto_update_profile=false` (default) does NOT write to the profile (MR-06)"
    - "`memory_reason` with `auto_update_profile=true` writes after Medium+ calls (MR-06)"
    - "`UserProfile`, `UserFact`, `Preference`, `BehavioralPattern` types exist per design § 2.8 (MR-06)"
    - "`memory_conclude` is registered with anchor comments `// === PHASE 1 PLAN 01-14 memory_conclude ===` (X-01)"
  artifacts:
    - "crates/openfang-reasoning/src/profile.rs with UserProfile + 3 sub-structs"
    - "`async fn tool_memory_conclude(input, kernel) -> Result<String, String>` in tool_runner.rs"
  key_links:
    - "Profile stored at `__user_profile__` structured-KV key (one entry per agent_id, or single global — match the structured-KV access pattern in openfang-memory)"
    - "Anchors disjoint from plan 01-13's `memory_reason` anchors"
---

<objective>
Ship the user-profile read/write surface and the agent-facing `memory_conclude` tool that persists explicit conclusions. The opt-in `auto_update_profile` flag from plan 01-12 is wired into `memory_reason` so the contradiction between design § 2.8 and § 5 (per addendum § C.1) is resolved.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-runtime/src/tool_runner.rs
@crates/openfang-reasoning/src/lib.rs
@crates/openfang-memory/src/substrate.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Profile types + KV serialization helpers</name>
  <files>crates/openfang-reasoning/src/profile.rs, crates/openfang-reasoning/src/lib.rs</files>
  <action>
Create `profile.rs` with the four types per design § 2.8 verbatim:
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfile {
    pub agent_id: Option<AgentId>,
    pub facts: Vec<UserFact>,
    pub preferences: HashMap<String, Preference>,
    pub patterns: Vec<BehavioralPattern>,
    pub updated_at: String,
}
pub struct UserFact { pub fact: String, pub confidence: f32, pub source: FactSource, pub first_observed: String, pub last_confirmed: String }
pub struct Preference { pub key: String, pub value: String, pub confidence: f32 }
pub struct BehavioralPattern { pub pattern: String, pub occurrences: u32, pub first_seen: String, pub last_seen: String }
```
Add free fns:
- `pub fn load_profile(memory: &MemorySubstrate, agent_id: &AgentId) -> Result<UserProfile, ReasoningError>` — reads JSON from structured KV key `__user_profile__/{agent_id}` (use the substrate's existing KV `get(key)`); returns default if missing.
- `pub fn save_profile(memory: &MemorySubstrate, profile: &UserProfile) -> Result<(), ReasoningError>` — `set(__user_profile__/{agent_id}, serde_json::to_string(profile)?)`. Updates `updated_at` to `chrono::Utc::now().to_rfc3339()`.
- `pub fn add_fact(profile: &mut UserProfile, fact: UserFact)` / `set_preference` / `add_pattern` helpers.
Re-export from lib.rs: `pub mod profile; pub use profile::{UserProfile, UserFact, Preference, BehavioralPattern};`.
  </action>
  <verify>
`cargo build -p openfang-reasoning --lib` clean.
Unit test: `MemorySubstrate::open_in_memory(0.0)`, build a profile, save, load, assert round-trip equality.
  </verify>
  <done>
Types compile; load/save round-trips through structured KV.
  </done>
</task>

<task type="auto">
  <name>Task 2: tool_memory_conclude dispatch + schema</name>
  <files>crates/openfang-runtime/src/tool_runner.rs</files>
  <action>
Around line 203, anchored:
```
        // === PHASE 1 PLAN 01-14 memory_conclude ===
        "memory_conclude" => tool_memory_conclude(input, kernel).await,
        // === END PHASE 1 PLAN 01-14 ===
```
Body of `tool_memory_conclude`: parse `input["kind"]: String` one of `fact|preference|pattern`. For `fact`: parse `fact: String, confidence: f32, source: FactSource (default StructuredKv)`. For `preference`: parse `key, value, confidence`. For `pattern`: parse `pattern, occurrences (default 1)`. Resolve agent_id from `KernelHandle::current_agent_id()` (use whatever accessor exists — search for similar usage in other tools like memory_store). Call `profile::load_profile(memory, &agent_id)?`, mutate, call `profile::save_profile`. Return `json!({"ok": true, "kind": "<kind>"})`.

Around line 645, schema entry anchored `// === PHASE 1 PLAN 01-14 memory_conclude schema ===`:
```
json!({
  "name": "memory_conclude",
  "description": "Persist an explicit conclusion (fact/preference/behavioral pattern) about the user into the user profile.",
  "input_schema": { "type": "object", "properties": {
    "kind": { "type": "string", "enum": ["fact","preference","pattern"] },
    "fact":  { "type": "string" }, "confidence": { "type": "number" },
    "key":   { "type": "string" }, "value": { "type": "string" },
    "pattern": { "type": "string" }, "occurrences": { "type": "integer" }
  }, "required": ["kind"] }
}),
```
  </action>
  <verify>
`cargo build -p openfang-runtime --lib` clean.
`cargo clippy --workspace --all-targets -- -D warnings` clean.
  </verify>
  <done>
memory_conclude wired; schema in place; anchors disjoint from 01-13.
  </done>
</task>

<task type="auto">
  <name>Task 3: auto_update_profile wiring in memory_reason + tests</name>
  <files>crates/openfang-runtime/src/tool_runner.rs</files>
  <action>
Modify `tool_memory_reason` from plan 01-13: after the engine call succeeds AND `result.level >= ReasoningLevel::Medium` AND `kernel.reasoning_config().auto_update_profile == true`, derive a `UserFact` from the result (`fact: result.answer.clone(), confidence: result.confidence, source: FactSource::StructuredKv{key:"__memory_reason__".into()}, first/last = now`) and persist via `profile::add_fact + save_profile`. Failures here log WARN and DO NOT fail the tool call.

Unit tests:
- `memory_conclude_writes_fact_to_profile` — fake kernel + in-memory substrate, call `memory_conclude(kind=fact, fact="user likes rust", confidence=0.9)`, assert `load_profile` returns 1 fact.
- `memory_conclude_writes_preference` — same for preference.
- `memory_conclude_writes_pattern` — same for pattern.
- `memory_reason_with_auto_update_false_does_not_write_profile` — default config; after a Medium reason, profile facts still empty.
- `memory_reason_with_auto_update_true_writes_after_medium` — config `auto_update_profile=true`; after a Medium reason, profile has 1 fact.
- `memory_reason_with_auto_update_true_does_not_write_after_low` — config true but call at Low; profile still empty (only Medium+).
- `memory_conclude_persists_across_loads` — write, recreate the profile loader, read back, equal.
  </action>
  <verify>
`cargo test -p openfang-runtime memory_conclude memory_reason_with_auto` runs ≥ 7 tests.
`cargo clippy --workspace --all-targets -- -D warnings` clean.
  </verify>
  <done>
Opt-in profile auto-update enforced; explicit conclude works; ≥ 7 tests green.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Live (in 01-16): curl agent message, ask it to conclude a fact; query `sqlite3` for the `__user_profile__/<agent>` KV key and verify JSON.
</verification>

<success_criteria>
- [ ] `UserProfile` + 3 sub-types exist with the design § 2.8 fields.
- [ ] `memory_conclude` agent tool persists fact/preference/pattern.
- [ ] `auto_update_profile=false` default behavior: NO writes from memory_reason.
- [ ] `auto_update_profile=true`: writes after Medium+ only.
- [ ] Anchors present and disjoint from 01-13.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-14-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- KV key shape decided (`__user_profile__` global vs `__user_profile__/{agent}` per agent)
- Any follow-ups for later plans
</output>
