# 01-06 — skill manifest flags — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** 1d74203

## One-liner

Added `mutable: Option<bool>` and `protected: Option<bool>` to `SkillMeta`
(the `[skill]` section of the manifest), with serde defaults so the 60
bundled skill.toml files parse unchanged and the values round-trip cleanly.

## Files changed

- `crates/openfang-skills/src/lib.rs` — added two fields to `SkillMeta` + 4
  tests.
- `crates/openfang-skills/src/openclaw_compat.rs` — 3 SkillMeta construction
  sites filled in `mutable: None, protected: None`.
- `crates/openfang-skills/src/loader.rs` — 1 test-side SkillMeta
  construction site filled in `mutable: None, protected: None`.

## Manifest type location (recorded per plan's request)

The plan skeleton referenced `crates/openfang-skills/src/manifest.rs`. The
manifest type actually lives in **`crates/openfang-skills/src/lib.rs`** —
`SkillManifest` at line 109, `SkillMeta` at line 137. No `manifest.rs` file
exists. The plan's instruction to update lib.rs is correct.

## Tests added

4 unit tests in `lib.rs::tests`:

1. `skill_toml_without_mutable_protected_parses`
2. `skill_toml_with_mutable_true_protected_false`
3. `skill_toml_round_trip_preserves_fields`
4. `skill_toml_serialize_omits_none_fields`

All 86 tests in `openfang-skills` pass (was 82 before the plan).
Workspace gates clean (build / test / clippy -D warnings).

## Deviations

**[Rule 1 — bug fix]** First version of `skill_toml_serialize_omits_none_fields`
used `out.contains("mutable")` for the negative assertion, and the test fixture's
description string literally contained the word "mutable" (`"no mutable/protected
fields"`). The assertion failed because the description text matched. Fixed by:
- Renaming the fixture description to `"no extra flags present"`.
- Tightening the assertion to a per-line `starts_with("mutable")` /
  `starts_with("protected")` check so future descriptive text doesn't
  accidentally trip the test.

Regression coverage is the same test — now correct.

## Decisions made during execution

1. **Field placement.** Both flags go on `SkillMeta` (the `[skill]` table),
   not on `SkillManifest` (the top level). This matches design § 1.7.1's TOML
   shape `[skill] mutable = false`.
2. **Default applied at load time, not at parse time.** Per SP-03, the
   defaults (bundled → `mutable=false`; SYSTEM_SKILLS subset → `protected=true`)
   are applied by `SkillRegistry::load_bundled` in plan 01-07. This plan keeps
   the schema layer dumb on purpose: parse `Some(x)` exactly as written, leave
   `None` to be resolved later. The doc-comment on each field makes that
   contract explicit.
3. **Construction sites in `openclaw_compat.rs` use `None`.** OpenClaw imports
   are user-initiated (`Native` source), so the load-time defaults will give
   them `mutable=true, protected=false` once 01-07 lands. Leaving `None` here
   preserves that future behavior — hardcoding `Some(true)` now would short
   circuit the load-time logic.

## Follow-ups for later plans

- **01-07 (load_bundled defaults):** must read `manifest.skill.mutable` /
  `manifest.skill.protected` after parsing. If `None`:
  - bundled skills → set effective `mutable = false`.
  - `SYSTEM_SKILLS` array members (including the future `skill-manage`) →
    set effective `protected = true`.
  - all others → `mutable = true, protected = false`.
  Effective values live on `InstalledSkill` (not back-written into the manifest)
  so the source `skill.toml` files stay byte-identical on disk.
- **01-05 (this wave, executed after this plan):** the `check_mutable` stub
  in `SkillRegistry` will eventually consult these flags. The stub itself is
  added by 01-05 and stays a no-op until 01-07 fills the body.
- **Marketplace/clawhub install path:** when a skill is downloaded from
  ClawHub, the install code should probably default `mutable = true`,
  `protected = false` to match the user-created-skill semantics. Worth
  re-verifying in 01-07 / 01-09.
