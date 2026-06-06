---
phase: 01-self-learning-core
plan: 05
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/openfang-skills/src/registry.rs
  - crates/openfang-skills/src/lib.rs
autonomous: true
must_haves:
  truths:
    - "All six mutation methods (`create_skill`, `patch_skill`, `edit_skill`, `write_skill_file`, `reload_skill`, `set_skill_enabled`) exist on `SkillRegistry` and complete the full pipeline: scan → SHA256 + audit append → TOML validate → config resolve → atomic write → reload → SkillUpdated event (SP-02, SP-05)"
    - "`patch_skill` rejects when `old_string` appears zero or more-than-once unless `replace_all=true` (SP-02)"
    - "`set_skill_enabled(name, false)` hides the skill from `list()` and dispatch but leaves the file on disk so it can be re-enabled (SP-02; decision in skeleton § Open decisions item 2)"
    - "Every mutation appends a Merkle entry to the existing `audit_entries` table — no new audit format (SP-05)"
  artifacts:
    - "`SkillError::Protected { name, action, hint }` and `SkillError::Immutable { name, action, hint }` variants in lib.rs"
    - "Six new methods on `impl SkillRegistry` in registry.rs"
    - "Atomic write helper (`tmp + rename`) used by every method that touches disk"
---

<objective>
Land the registry-side mutation surface that `skill_manage` (plan 01-08) calls. This plan does NOT register the agent-facing tool yet — it ships the library methods, the audit/event plumbing, and the error variants. Default-application (mutable=false, protected=true for SYSTEM_SKILLS) is plan 01-07's job; this plan assumes those checks may be added later and stubs them as a no-op `check_mutable` that always returns Ok for now.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-skills/src/registry.rs
@crates/openfang-skills/src/lib.rs
@crates/openfang-skills/src/verify.rs
@crates/openfang-skills/src/config_injection.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: SkillError variants + atomic write helper</name>
  <files>crates/openfang-skills/src/lib.rs, crates/openfang-skills/src/registry.rs</files>
  <action>
In `lib.rs` extend the `SkillError` enum with:
```
#[error("Skill '{name}' is protected — {action} blocked. {hint}")]
Protected { name: String, action: String, hint: String },
#[error("Skill '{name}' is immutable — {action} blocked. {hint}")]
Immutable { name: String, action: String, hint: String },
```
In `registry.rs` add a private helper `fn write_atomic(path: &Path, content: &[u8]) -> Result<(), SkillError>` that writes to `path.with_extension("tmp")` then `std::fs::rename` to the final path (POSIX atomicity, Windows-safe via rename-replace). Wrap I/O errors as `SkillError::Io(...)`. Add a stub `fn check_mutable(&self, _name: &str, _action: &str) -> Result<(), SkillError> { Ok(()) }` — plan 01-07 implements the real body.
  </action>
  <verify>
`cargo build -p openfang-skills --lib` clean.
  </verify>
  <done>
Two new SkillError variants; atomic helper compiles; stub `check_mutable` present.
  </done>
</task>

<task type="auto">
  <name>Task 2: Six mutation methods on SkillRegistry</name>
  <files>crates/openfang-skills/src/registry.rs</files>
  <action>
Implement per design § 1.2 + § 1.3 pipeline. Each method calls `check_mutable(name, "<action>")?` FIRST (a no-op until 01-07), then runs the pipeline:
1. `SkillVerifier::scan_prompt_content(&content)` (existing fn in `verify.rs`) — CRITICAL → return `SecurityBlocked`; WARNING → log and accept.
2. Compute `sha2::Sha256` over the new bytes. Append a Merkle audit entry via the existing `AuditAppender` infrastructure — call the audit API on the kernel-side appender if the registry holds one, else expose a hook so the caller can append. (Concretely: the registry already has access to an audit logger via `set_skill_configs`/event bus? If not, defer the audit append to a callback set via a new setter `set_audit_appender(Arc<dyn AuditAppend>)` so kernel can wire it. Define the trait minimal: `fn append(&self, event_type: &str, payload: serde_json::Value) -> Result<(), SkillError>`.)
3. TOML validation: `toml::from_str::<SkillManifest>(&toml_content)`.
4. `apply_skill_config(&mut manifest, &self.skill_configs)` (existing fn).
5. `write_atomic(&path, content.as_bytes())`.
6. Re-load via existing `load_skill(skill_dir)` to refresh the in-memory HashMap.
7. Emit `SkillUpdated { name }` via the event bus (set via a similar `set_event_bus(Arc<EventBus>)` if not already wired).

Method-specific:
- `create_skill(name, toml_content, prompt_context, category)`: rejects if name exists (`SkillError::AlreadyInstalled`). Creates `~/.openfang/skills/<name>/skill.toml` and `prompt_context.md`. Sets `mutable=Some(true)` if the manifest didn't specify (per addendum § B.4 last paragraph).
- `patch_skill(name, old_string, new_string, replace_all)`: reads current `skill.toml`, counts occurrences of `old_string`; if count == 0 → `SkillError::InvalidManifest("old_string not found".into())`; if count > 1 && !replace_all → `SkillError::InvalidManifest("old_string matches multiple — pass replace_all=true".into())`; else perform the replacement, run pipeline.
- `edit_skill(name, toml_content)`: replace whole skill.toml.
- `write_skill_file(name, file_path, content)`: file_path is relative; reject `..` traversal; write atomically; NO pipeline step beyond audit append (it's not the skill manifest).
- `reload_skill(name)`: just step 6 + 7. Used after external edits.
- `set_skill_enabled(name, enabled)`: flip an in-memory `enabled` flag on `InstalledSkill`. When `enabled=false`, the skill stays loaded but `list()`, `all_tool_definitions()`, and `find_tool_provider()` skip it (modify the existing methods to filter on the flag). File on disk untouched.
  </action>
  <verify>
`cargo build -p openfang-skills --lib` clean.
`cargo clippy -p openfang-skills --all-targets -- -D warnings` clean.
  </verify>
  <done>
All six methods compile; pipeline steps invoked in the documented order; `list()`/dispatch filter on `enabled`.
  </done>
</task>

<task type="auto">
  <name>Task 3: Per-method unit tests with tempdir</name>
  <files>crates/openfang-skills/src/registry.rs</files>
  <action>
In `#[cfg(test)] mod tests` add at least one happy-path test per method using `tempfile::tempdir()` and a fresh `SkillRegistry::new(tmp.path())`:
- `create_then_list_shows_skill` — create, then `registry.list()` returns it.
- `patch_replaces_string_in_skill_toml` — create, patch with old/new, read file, assert new content present and old absent.
- `patch_rejects_multiple_matches_without_replace_all` — patch with old_string appearing twice → expects `InvalidManifest`.
- `patch_replace_all_replaces_every_occurrence` — same, with replace_all=true.
- `edit_replaces_entire_manifest` — edit_skill, assert file contents == new content.
- `write_skill_file_writes_relative` — assert file at `<skill>/references/notes.md` exists.
- `write_skill_file_rejects_traversal` — file_path="../etc/passwd" returns error.
- `reload_skill_picks_up_external_edit` — manually edit `skill.toml`, call reload, assert in-memory state matches.
- `set_skill_enabled_false_hides_from_list` — disable, assert `registry.list().iter().find(|s| s.name == name)` returns None.
- `set_skill_enabled_true_re_exposes` — re-enable, assert visible again.
Test fakes for audit/event bus use a hand-rolled `Arc<Mutex<Vec<_>>>` recorder (per CONVENTIONS.md / TESTING.md — no mockall).
  </action>
  <verify>
`cargo test -p openfang-skills registry` runs ≥ 10 new tests, all green.
`cargo clippy -p openfang-skills --all-targets -- -D warnings` clean.
  </verify>
  <done>
≥ 10 new unit tests covering every method's happy path and key error paths.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (≥ 10 new tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Grep: every mutation method calls `check_mutable(...)` as its FIRST statement.
</verification>

<success_criteria>
- [ ] Six methods exist and match the design signatures.
- [ ] Each mutation produces an audit entry (recorder in test counts ≥ 1).
- [ ] `set_skill_enabled(false)` hides from `list()`/dispatch but file remains.
- [ ] Atomic write uses `tmp + rename`.
- [ ] No mockall dependency added.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-05-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- How the audit/event bus wiring was exposed to the registry (callback trait? Arc setter?)
- Any follow-ups for later plans (kernel-side wiring of audit appender + event bus in 01-08)
</output>
