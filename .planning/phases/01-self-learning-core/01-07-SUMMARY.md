# 01-07 — protected/mutable defaults — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** e951c52

## One-liner

Exported `SYSTEM_SKILLS: &[&str]` with the 10 names; added a pure
`apply_load_time_defaults(manifest, is_bundled)` mapping that lands on
load (bundled → mutable=false / protected iff system; user → mutable=true,
protected=false); promoted the plan-01-05 `check_mutable` stub to a real
guard that returns the structured `Protected` / `Immutable` errors with
on-disk-fix hints; removed the incorrect `check_mutable` call from
`create_skill` per the plan's explicit exemption.

## Files changed

- `crates/openfang-skills/src/lib.rs`:
  - `pub const SYSTEM_SKILLS: &[&str]` — 10 names.
  - `pub fn is_system_skill(name: &str) -> bool`.
  - `pub fn apply_load_time_defaults(manifest: &mut SkillManifest, is_bundled: bool)`.
- `crates/openfang-skills/src/registry.rs`:
  - `load_bundled`: `apply_load_time_defaults(_, true)` after parse,
    before scan + insert.
  - `load_skill`: `apply_load_time_defaults(_, false)`. Covers
    `load_workspace_skills` transitively (which dispatches through
    `load_skill`).
  - `check_mutable`: real body — `Protected` wins over `Immutable`,
    `NotFound` for unknown skills, fix hints point at the on-disk
    `skill.toml` field.
  - `create_skill`: removed the `self.check_mutable(name, "create")?`
    line that 01-05 left there as a stub-era convenience — the plan
    explicitly exempts create. The duplicate-name check downstream
    catches the already-installed case.
  - 8 new tests.

## Tests added

8 new tests in `registry::tests` (total now 110 — was 102):

1. `bundled_system_skill_loads_protected` — loads bundled set, picks
   the first SYSTEM_SKILLS entry present, asserts `protected==Some(true)
   && mutable==Some(false)`. Gracefully skips the assertion if no
   SYSTEM_SKILLS skill is bundled in the current build.
2. `bundled_non_system_skill_loads_immutable_not_protected` — same
   shape, picks a non-system bundled skill, asserts `protected==Some(false)
   && mutable==Some(false)`.
3. `patch_protected_skill_returns_protected_error_no_disk_write` —
   creates a user skill with explicit `protected=true`, captures
   SHA256 of `skill.toml`, attempts patch, asserts `Protected` with
   the expected `name`/`action`/`hint`, asserts the post-SHA matches
   the pre-SHA (no disk write happened).
4. `patch_immutable_skill_returns_immutable_error_no_disk_write` —
   same shape with `protected=false, mutable=false`; asserts
   `Immutable` + byte-equal disk file.
5. `patch_user_skill_succeeds_after_create_skill` — happy-path: a
   user skill created without explicit flags gets `mutable=true` from
   `create_skill`, and `patch` succeeds.
6. `protected_field_in_user_skill_toml_honored` — user can opt into
   `protected=true` on their own skill; `protected` wins over `mutable`.
7. `check_mutable_unknown_skill_returns_not_found` — calling
   `check_mutable("nope", "patch")` directly returns `NotFound("nope")`.
8. `apply_load_time_defaults_pure_mapping` — covers all four
   (bundled-system, bundled-non-system, user-with-system-name,
   explicit-values-win) branches of the helper.

Workspace gates clean: 2765 tests pass (was 2757 → +8 new); build
clean; clippy `-D warnings` clean.

## Deviations

**[Rule 1 — bug in upstream stub code]** Plan 01-05's `create_skill`
contained `self.check_mutable(name, "create")?;` as the first line.
That was fine while `check_mutable` was a stub returning `Ok(())` but
broke once 01-07's real body lands: the skill does not exist yet, so
the lookup returns `NotFound`. The plan explicitly exempts
`create_skill` from the check. Removed the call and replaced with a
doc-comment explaining the exemption. Fixed 16 pre-existing 01-05
tests in the same commit — they were all failing on `NotFound("x")`
before this fix.

**[Rule 2 — added a missing-critical test]** Added
`check_mutable_unknown_skill_returns_not_found` (test #7) on top of
the 6 the plan requested. The NotFound branch of `check_mutable`
needs a positive regression test because the `create_skill` fix above
relies on it returning a typed error rather than panicking.

**[Rule 2 — added a missing-critical test]** Added
`apply_load_time_defaults_pure_mapping` (test #8). The plan asked
for tests of the registry-side load behavior; the pure mapping has
four logical branches and deserves direct coverage so a future
refactor can't quietly invert one of them.

## Decisions made during execution

1. **`apply_load_time_defaults` lives in `lib.rs`, not `registry.rs`.**
   Keeps it next to `SYSTEM_SKILLS` (the constant the function reads),
   so a future change to the constant doesn't require a hunt across
   files. It's a pure function with no `self` dependency — registry
   only calls it.
2. **`is_bundled` is a bool parameter, not a `SkillSource` enum.**
   The plan's SkillSource-based dispatch is essentially "is this a
   bundled-skill loader call?" — a bool says the same thing in one
   bit. The bundled-vs-user dispatch is the caller's choice (the load
   method's identity), not something the helper has to discover.
3. **`Protected` wins over `Immutable`.** The plan listed the checks
   without ordering. I went `protected first → immutable second` so
   the user always sees the right unlock instructions (the
   `protected = false` hint, not the `mutable = true` one) for a
   skill that's both.
4. **Hints are short imperative strings** that the `skill_manage`
   tool layer in 01-08 can surface verbatim. Phrasing matches the
   01-05 SUMMARY follow-up.
5. **`create_skill` keeps stamping `mutable=true`** on the manifest
   patch path (already in 01-05). That means a user-created skill
   loaded back via `load_skill` gets `mutable=true` from the on-disk
   file, NOT from the load-time default — the explicit value beats
   the default, which is what we want.
6. **Bundled set may be empty in some builds.** Tests #1 and #2
   gracefully skip if no SYSTEM_SKILLS / no non-SYSTEM bundled skill
   is present — production builds will have both, but a stripped
   dev build shouldn't fail the suite over a missing fixture.

## Follow-ups for later plans

- **01-08 (`skill_manage` tool):** the `Protected`/`Immutable` errors
  should be passed through to the agent as tool errors with their
  `name`/`action`/`hint` fields intact. The user-facing string is
  already adequate (`Skill 'memory-core' is protected — patch
  blocked. Set \`protected = false\` ...`); 01-08 can render this
  directly into the tool result.
- **01-09 (snapshot-refresh bus):** when a skill on disk has its
  `protected`/`mutable` flag flipped and `reload_skill` is called,
  the new values land via the same `load_skill` path → defaults are
  reapplied → new `Protected`/`Immutable` semantics take effect on
  the next mutation call. No extra wiring needed.
- **Marketplace install path:** when ClawHub adds a skill, the
  install code goes through `load_skill` (is_bundled=false) so the
  defaults give `mutable=true, protected=false`. If marketplace
  policy ever wants a different default (e.g. signed skills land
  `mutable=false`), that's a separate decision; mention in 01-09 or
  later.
- **CONCERNS.md drift note:** test #6
  (`protected_field_in_user_skill_toml_honored`) pins the
  "explicit `protected=true` in user TOML is honored" invariant —
  if anyone refactors `apply_load_time_defaults` to overwrite
  rather than fill, this test fails immediately.
