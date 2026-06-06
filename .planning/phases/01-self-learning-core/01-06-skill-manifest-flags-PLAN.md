---
phase: 01-self-learning-core
plan: 06
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/openfang-skills/src/lib.rs
autonomous: true
must_haves:
  truths:
    - "Existing `skill.toml` files (60 bundled + any user-created) parse unchanged after adding the two optional fields (SP-03)"
    - "`mutable: Option<bool>` and `protected: Option<bool>` deserialize correctly when present and default to `None` when absent (SP-03)"
    - "Manifest serializes back the same fields with `#[serde(skip_serializing_if = \"Option::is_none\")]` so writes do not introduce noise"
  artifacts:
    - "`pub mutable: Option<bool>` field on the manifest's `[skill]` section struct"
    - "`pub protected: Option<bool>` field on the same struct"
---

<objective>
Add the two `Option<bool>` flags to the skill manifest type that plan 01-07 will read at load-time to compute defaults. Defaults are NOT applied here — this plan is the schema-side change only, deliberately separate so 01-07 can ship the runtime semantics atop a stable type.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-skills/src/lib.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Locate the manifest skill-section struct</name>
  <files>crates/openfang-skills/src/lib.rs</files>
  <action>
The skeleton names `crates/openfang-skills/src/manifest.rs` and `SkillManifestSkill`. After verification the manifest type actually lives in `crates/openfang-skills/src/lib.rs` as `pub struct SkillManifest { ... }` (around line 109). The `[skill]` section is the nested `pub skill: SkillManifestSkillSection` (or equivalent — confirm the exact field name on first read). Add to that struct (NOT to a top-level type — keep the addition under the `[skill]` table so TOML reads `[skill]\nmutable = false`):
```
#[serde(default, skip_serializing_if = "Option::is_none")]
pub mutable: Option<bool>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub protected: Option<bool>,
```
If `SkillManifest` is flat (no nested skill section), add the fields directly to it — the design example in § 1.7.1 shows `[skill] mutable = false`, so a nested section is expected; verify before placing. Do NOT create a new `manifest.rs` file; the existing structure in lib.rs is authoritative.
  </action>
  <verify>
`cargo build -p openfang-skills --lib` clean.
`cargo test -p openfang-skills` — every existing test still passes (60 bundled skills must still load).
  </verify>
  <done>
Two optional fields added; bundled skills unaffected.
  </done>
</task>

<task type="auto">
  <name>Task 2: Round-trip and absent-field tests</name>
  <files>crates/openfang-skills/src/lib.rs</files>
  <action>
Add to the existing `#[cfg(test)] mod tests` block (or create one):
- `skill_toml_without_mutable_protected_parses` — TOML literal lacking both fields parses to `Some(manifest)` with `mutable.is_none() && protected.is_none()`.
- `skill_toml_with_mutable_true_protected_false` — fields present, asserts `Some(true)` / `Some(false)`.
- `skill_toml_round_trip_preserves_fields` — parse → serialize → parse, fields survive.
- `skill_toml_serialize_omits_none_fields` — manifest with both `None` serializes WITHOUT `mutable` / `protected` lines.
Use minimal valid TOML so the rest of the manifest (name, version, runtime) satisfies existing required fields. Inline `const` strings per the pattern from TESTING.md.
  </action>
  <verify>
`cargo test -p openfang-skills mutable_protected` (or similar filter) runs the 4 new tests.
`cargo clippy -p openfang-skills --all-targets -- -D warnings` clean.
  </verify>
  <done>
4 new tests green; serde round-trip clean.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- All 60 bundled skills load: `cargo test -p openfang-skills bundled` (or comparable existing test) still passes.
</verification>

<success_criteria>
- [ ] `mutable: Option<bool>` and `protected: Option<bool>` exist on the manifest type.
- [ ] Bundled TOML files (no fields) parse unchanged.
- [ ] Manifest with both fields parses to `Some(true)` / `Some(false)`.
- [ ] Round-trip preserves the values; absent fields are not serialized.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-06-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Note on whether the manifest type was in `lib.rs` (as found) or `manifest.rs` (as the skeleton expected) — record the actual path
- Any follow-ups for later plans
</output>
