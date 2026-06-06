---
phase: 01-self-learning-core
plan: 07
type: execute
wave: 2
depends_on: [01-05, 01-06]
files_modified:
  - crates/openfang-skills/src/registry.rs
  - crates/openfang-skills/src/lib.rs
autonomous: true
must_haves:
  truths:
    - "All 60 bundled skills load with `mutable=false`; the subset listed in `SYSTEM_SKILLS` additionally loads with `protected=true` (SP-03, success-criterion 10)"
    - "Attempting `patch`/`edit`/`delete` on a protected skill returns `SkillError::Protected { name, action, hint }` with the unlock hint, and NO disk modification occurs (SP-03, success-criterion 3)"
    - "Attempting `patch`/`edit`/`delete` on an immutable-but-not-protected skill returns `SkillError::Immutable { name, action, hint }` (SP-03)"
    - "No `protected = true` line is ever added to a bundled `skill.toml` source file — defaults are computed at load time only (SP-03; no-build-script criterion)"
  artifacts:
    - "`pub const SYSTEM_SKILLS: &[&str] = &[\"skill-manage\", \"tool-dispatch\", \"memory-core\", \"memory-reason\", \"session-manager\", \"session-search\", \"event-bus\", \"kernel-api\", \"security-scanner\", \"prompt-injection\"];` exported from openfang-skills"
    - "`check_mutable(&self, name, action)` real body in registry.rs"
  key_links:
    - "Every mutation method from plan 01-05 calls `check_mutable` as its first statement — this plan replaces the no-op stub with the real implementation"
---

<objective>
Apply code-level defaults to the `mutable`/`protected` manifest fields at load time and turn the no-op `check_mutable` stub from plan 01-05 into a real guard. This is the addendum § B.4 mechanism — no build script ever mutates bundled `skill.toml` files.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-skills/src/registry.rs
@crates/openfang-skills/src/lib.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Export SYSTEM_SKILLS and apply load-time defaults</name>
  <files>crates/openfang-skills/src/lib.rs, crates/openfang-skills/src/registry.rs</files>
  <action>
In `lib.rs` add the exact constant (top-level, before SkillError):
```
pub const SYSTEM_SKILLS: &[&str] = &[
    "skill-manage", "tool-dispatch", "memory-core", "memory-reason",
    "session-manager", "session-search", "event-bus", "kernel-api",
    "security-scanner", "prompt-injection",
];
```
In `registry.rs`, modify `load_bundled` (line 125): immediately after a skill manifest is parsed, apply:
```
if manifest.skill.mutable.is_none()   { manifest.skill.mutable   = Some(false); }
if manifest.skill.protected.is_none() { manifest.skill.protected = Some(crate::SYSTEM_SKILLS.contains(&manifest.skill.name.as_str())); }
```
Adjust the path (`manifest.skill.<field>` vs `manifest.<field>`) to match whatever 01-06 settled on. Crucially: this MUST happen for every load path that bundled skills traverse — also check `load_workspace_skills` (line 378) and `load_skill` (line 275) and apply the same defaults BUT only mark `protected = true` from SYSTEM_SKILLS if `source == SkillSource::Bundled` (workspace/user skills default to `protected=false`). Bundled-vs-user dispatch already exists in code — grep for `SkillSource::` usages.
  </action>
  <verify>
`cargo build -p openfang-skills --lib` clean.
`cargo test -p openfang-skills load_bundled` — existing 60-skill load test still passes. Add a quick assertion: among the loaded bundled skills, `memory-core.protected == Some(true)` and `pdf-reader.protected == Some(false)`.
  </verify>
  <done>
SYSTEM_SKILLS exported; load-time defaults applied across all three load paths; bundled `skill.toml` files unchanged on disk.
  </done>
</task>

<task type="auto">
  <name>Task 2: Real check_mutable + plumb into mutation methods</name>
  <files>crates/openfang-skills/src/registry.rs</files>
  <action>
Replace the stub `check_mutable` from plan 01-05 with:
```
fn check_mutable(&self, name: &str, action: &str) -> Result<(), SkillError> {
    let skill = self.skills.get(name)
        .ok_or_else(|| SkillError::NotFound(name.to_string()))?;
    let protected = skill.manifest.skill.protected.unwrap_or(false);
    let mutable   = skill.manifest.skill.mutable.unwrap_or(false);
    if protected {
        return Err(SkillError::Protected {
            name: name.to_string(),
            action: action.to_string(),
            hint: "Set `protected = false` in the skill.toml on disk and reload the agent.".to_string(),
        });
    }
    if !mutable {
        return Err(SkillError::Immutable {
            name: name.to_string(),
            action: action.to_string(),
            hint: "Set `mutable = true` in the skill.toml on disk and reload the agent.".to_string(),
        });
    }
    Ok(())
}
```
Confirm it's invoked as the FIRST statement of `patch_skill`, `edit_skill`, `remove`/`delete`, `write_skill_file`, `reload_skill`, and `set_skill_enabled`. `create_skill` skips the check (skill does not exist yet). Match the exact `manifest.skill.<field>` access path to plan 01-06's structure.
  </action>
  <verify>
`cargo build -p openfang-skills --lib` clean.
  </verify>
  <done>
check_mutable returns Protected/Immutable; every mutation calls it; create_skill exempt.
  </done>
</task>

<task type="auto">
  <name>Task 3: Default-application + check_mutable tests</name>
  <files>crates/openfang-skills/src/registry.rs</files>
  <action>
Add to `mod tests`:
- `bundled_system_skill_loads_protected` — after `load_bundled`, fetch one SYSTEM_SKILLS entry, assert `protected == Some(true) && mutable == Some(false)`.
- `bundled_non_system_skill_loads_immutable_not_protected` — fetch a non-SYSTEM bundled skill (e.g. `pdf-reader` if present), assert `protected == Some(false) && mutable == Some(false)`.
- `patch_protected_skill_returns_protected_error` — try `registry.patch_skill("memory-core", "x", "y", false)`, assert `SkillError::Protected { .. }`, then assert the file on disk is byte-equal to its pre-state.
- `patch_immutable_skill_returns_immutable_error` — same with a bundled non-system skill.
- `patch_user_skill_succeeds_after_create_skill` — `create_skill("user-x", valid_toml, prompt, None)`, then `patch_skill("user-x", ...)` → `Ok(())`.
- `protected_field_in_user_skill_toml_honored` — create a user skill whose TOML literally sets `protected = true`, then attempt patch → `Protected`.
For "file unchanged" assertions, take a SHA256 before and after and compare. Use `tempfile::tempdir()`.
  </action>
  <verify>
`cargo test -p openfang-skills check_mutable patch_protected patch_immutable bundled_system` runs the new tests, all green.
`cargo clippy -p openfang-skills --all-targets -- -D warnings` clean.
  </verify>
  <done>
≥ 6 tests covering protected/immutable enforcement and load-time default application.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (≥ 6 new tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `git diff bundled/` — empty (no skill.toml mutated by code).
</verification>

<success_criteria>
- [ ] SYSTEM_SKILLS exported as `pub const &[&str]` with the 10 names exactly.
- [ ] Bundled SYSTEM skills load `protected=true`.
- [ ] Bundled non-SYSTEM skills load `protected=false, mutable=false`.
- [ ] User skills default to `mutable=true` (from plan 01-05 `create_skill`).
- [ ] `patch_skill` on a protected skill returns `Protected`, no disk write.
- [ ] No bundled `skill.toml` files modified on disk.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-07-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Any decisions made during execution that weren't pinned in this plan
- Any follow-ups for later plans
</output>
