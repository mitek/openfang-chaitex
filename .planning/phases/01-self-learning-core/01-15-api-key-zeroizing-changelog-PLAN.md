---
phase: 01-self-learning-core
plan: 15
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/openfang-types/src/config.rs
  - CHANGELOG.md
autonomous: true
must_haves:
  truths:
    - "`AuthProfile.api_key` type is `zeroize::Zeroizing<String>` — drops the inner String with `mem::zeroize` on drop (X-03)"
    - "Existing TOML configs still deserialize (Zeroizing<String> is `From<String>`; the existing TOML `api_key = \"...\"` parses unchanged) (X-03)"
    - "CHANGELOG.md has an `[Unreleased]` entry describing schema v9, new crate, new tools, api_key zeroize, and backward-compat (X-04, success-criterion 12)"
    - "No call site that previously consumed `&str` from `api_key` breaks — `Zeroizing<String>` derefs to `String` (X-03)"
  artifacts:
    - "Line ~1168 of crates/openfang-types/src/config.rs reads `pub api_key: zeroize::Zeroizing<String>`"
    - "`zeroize` declared in `[dependencies]` of crates/openfang-types/Cargo.toml with the `zeroize_derive` feature only if actually needed"
    - "CHANGELOG.md `[Unreleased]` section with the schema v9 / new tools / api_key zeroize bullets"
---

<objective>
Switch the provider `api_key` field to `Zeroizing<String>` so memory is zeroed on drop and add the Phase 1 CHANGELOG entry. This is the smallest of the wave-1 plans but is line-disjoint from 01-08 (`capabilities`) and 01-12 (`[reasoning]`) so it can run in parallel.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-types/src/config.rs
@CHANGELOG.md
</context>

<tasks>

<task type="auto">
  <name>Task 1: Switch api_key to Zeroizing<String></name>
  <files>crates/openfang-types/src/config.rs, crates/openfang-types/Cargo.toml</files>
  <action>
Add `zeroize = { version = "1", features = ["zeroize_derive"] }` to `crates/openfang-types/Cargo.toml [dependencies]` (also add to workspace deps in root Cargo.toml if not present). At `config.rs:1168` change `pub api_key: String,` to `pub api_key: zeroize::Zeroizing<String>,`. At line 1505 in the Default impl, change `api_key: String::new(),` to `api_key: zeroize::Zeroizing::new(String::new()),`. The existing custom `Debug` impl that prints `<redacted>` (line 1056) must not regress — keep that field redacted (the type implements Debug as the inner, so the custom redaction must still apply via the existing Debug override). If a manual `Debug` impl was previously naming this field as `&self.api_key`, change to `&"<redacted>"` to preserve the security invariant.

Find every consumer:
```
grep -rn "\.api_key" crates/ | grep -v test
```
Most consumers will treat `Zeroizing<String>` exactly like `String` via `Deref`. Where a call uses `api_key.clone()` (yielding `Zeroizing<String>`), replace with `(*api_key).clone()` (yielding `String`) only if the receiving API rejects `Zeroizing<String>` (e.g. a reqwest builder). Otherwise leave alone — `Deref` resolves it. Inline-comment any change with `// SECURITY: zeroize on drop`.

Add to the existing `mod tests`: `auth_profile_default_api_key_is_empty_zeroizing` — assert `Default::default().api_key.is_empty()`.
  </action>
  <verify>
`cargo build --workspace --lib` clean.
`cargo test --workspace` clean — call-site fixes are correct.
`cargo clippy --workspace --all-targets -- -D warnings` clean.
  </verify>
  <done>
Type change applied; all consumers compile; tests green.
  </done>
</task>

<task type="auto">
  <name>Task 2: CHANGELOG entry</name>
  <files>CHANGELOG.md</files>
  <action>
Open `CHANGELOG.md` and insert (or augment the existing) `## [Unreleased]` section near the top with bullets:
```
## [Unreleased]
### Added
- Schema v9: FTS5 session search via new `session_messages` flat companion table + external-content FTS5 index, with msgpack BLOB read path unchanged.
- New crate `openfang-reasoning` providing 5-level reasoning engine, persistent monthly budget tracker, and opt-in user profile.
- Built-in tools: `session_search`, `skill_manage`, `memory_reason`, `memory_conclude`. (Tool count increased by 4.)
- Skill self-patching with two-tier `protected`/`mutable` defense (defaults applied at load — bundled `skill.toml` files unchanged on disk).
- Capability flag `capabilities.allow_skill_mutation` (default `false`) gates `skill_manage` mutations.
- `[reasoning]` config block with `deny_unknown_fields` — typos now fail loud at startup.

### Changed
- Provider `api_key` is now stored as `zeroize::Zeroizing<String>` so memory is wiped on drop.

### Compatibility
- Backward compatible: existing v8 user databases auto-upgrade to v9 on first boot. Existing sessions are backfilled best-effort; corrupted BLOBs are logged WARN and skipped. Existing `skill.toml` files parse unchanged (the two new flags are optional).
```
Preserve the file's existing format (date header style, etc).
  </action>
  <verify>
`grep -A 25 '\[Unreleased\]' CHANGELOG.md` shows the new block.
  </verify>
  <done>
CHANGELOG entry present and matches REQ X-04 / success-criterion 12.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `grep -A 20 "Unreleased" CHANGELOG.md` shows the new entry.
- Manual: `Debug` formatting of an `AuthProfile` still prints `<redacted>` for `api_key`.
</verification>

<success_criteria>
- [ ] `api_key: Zeroizing<String>` in source.
- [ ] All consumers compile.
- [ ] `Debug` formatting still redacted.
- [ ] CHANGELOG `[Unreleased]` entry present with v9 / new crate / new tools / api_key / backward-compat bullets.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-15-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Any consumer call sites that needed Deref-style fixes
- Any follow-ups for later plans
</output>
