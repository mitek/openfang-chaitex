# 01-15 — api_key zeroizing + CHANGELOG — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** f472d06, 17d1e1b

## One-liner

Migrated `KernelConfig.api_key` to `zeroize::Zeroizing<String>` via a small
serde adapter (no `zeroize/serde` feature flip), fixed three call-sites that
assigned a bare `String`, kept the existing `<redacted>` Debug behaviour, and
landed the Phase 1 `[Unreleased]` CHANGELOG entry.

## Files changed

- `crates/openfang-types/Cargo.toml` — `zeroize = { workspace = true }`.
- `crates/openfang-types/src/config.rs`:
  - `KernelConfig.api_key: String` → `zeroize::Zeroizing<String>` with
    `#[serde(with = "zeroizing_string", default = "default_api_key")]`.
  - New `default_api_key()` helper.
  - New `mod zeroizing_string` serde adapter (10 lines).
  - Default impl: `String::new()` → `Zeroizing::new(String::new())`.
  - 3 new tests.
- `crates/openfang-kernel/src/kernel.rs:613` — `OPENFANG_API_KEY` env-var
  path now wraps with `Zeroizing::new(...)`.
- `crates/openfang-kernel/src/config_reload.rs:368` — test fixture wrapped.
- `crates/openfang-api/Cargo.toml` — added `zeroize` as dev-dependency.
- `crates/openfang-api/tests/api_integration_test.rs:903` —
  `start_test_server_with_auth` wraps with `Zeroizing::new(...)`.
- `CHANGELOG.md` — `[Unreleased]` block with Added / Changed / Compatibility
  bullets covering schema v9, new crate, new tools, skill self-patching,
  capability flag, `[reasoning]` config, api_key zeroize, and the
  backward-compat note for v8 → v9 upgrade.
- `Cargo.lock` — picks up `zeroize` for `openfang-types`.

## Tests added

3 in `openfang-types::config::tests`:

1. `kernel_config_default_api_key_is_empty_zeroizing` — default is empty.
2. `kernel_config_debug_redacts_api_key` — both `<empty>` (no key set) and
   `<redacted>` (key set) paths covered; **asserts the plaintext does NOT
   appear in Debug output**.
3. `kernel_config_api_key_toml_round_trip` — `api_key = "..."` parses and
   re-serializes byte-identically.

All workspace tests pass. Workspace gates clean: build / test / clippy
`-D warnings`.

## Consumer call sites that needed Deref-style fixes

All KernelConfig.api_key reads on the workspace (greppable via
`config.api_key.` / `cfg.api_key.`) use `.trim()`, `.is_empty()`,
`.as_str()`, or `.clone()` — all available through `Deref<Target = String>`
on `Zeroizing<String>`. **Zero read-side changes were needed.**

The three WRITE-side fixes (assignment) are listed under Files changed.
Per the plan's allowance ("mechanical follow-up to compile errors in
openfang-cli IS allowed because it's plumbing"), openfang-cli was scanned
— it has no `KernelConfig.api_key = ...` assignment (CLI uses the
`openfang config set` path which goes through serde, not direct field
assignment). No CLI changes needed.

## Deviations

**[Rule 3 — auto-fix blocking]** Direct serde via `Zeroizing<String>`
needs `zeroize/serde` feature. Turning that on workspace-wide is a wider
behavioural change than this plan mandates (it adds a serde impl across
every `Zeroizing<T>` in the codebase). Instead, added a 10-line
`zeroizing_string` serde adapter local to `config.rs` so only the
`api_key` field uses the adapter. Future cross-cutting `Zeroizing`
adoption can flip the workspace feature if/when wanted.

**[Rule 2 — missing-critical Debug check]** The plan listed "Debug
formatting still redacted" as a manual verification step. Promoted it to
an automated test (`kernel_config_debug_redacts_api_key`) that fails
loudly if any future refactor regresses the redaction — the new test
positively asserts the plaintext is absent from the Debug output.

## Decisions made during execution

1. **Adapter vs feature.** Chose the local serde adapter (10 lines, scoped
   to one field) over `zeroize = { features = ["serde"] }` (turns on
   ser/deser for every `Zeroizing<T>` in the dep tree, which is a larger
   surface change). Decision rationale captured in the `mod
   zeroizing_string` doc comment so a future maintainer who wants the
   broader change knows where to start.
2. **`default = "default_api_key"`** on the field. The struct-level
   `#[serde(default)]` already exists, but combining `with` and the
   top-level default for `Zeroizing<String>` produced a type-mismatch
   path; pinning the field-level `default` fn is the simplest fix.
3. **CHANGELOG location.** Placed the `[Unreleased]` content underneath
   the existing `## [Unreleased]` header (which was empty). Preserved the
   chronological structure and the keepachangelog format.

## Follow-ups for later plans

- **01-12 (reasoning config block):** when adding `[reasoning]` to
  `KernelConfig`, mirror the `deny_unknown_fields` invariant noted in the
  CHANGELOG (and in REQ MR-05).
- **Marketplace + provider api_key fields:** the provider drivers
  (`AnthropicDriver`, `OpenAiDriver`, etc.) already wrap their per-driver
  `api_key` in `Zeroizing<String>` — confirmed via grep. The provider
  config struct (`AuthProfile.api_key_env`) still stores the env-var
  *name* in a plain `String`, which is fine (env-var names are not
  secrets).
- **Future workspace-wide `zeroize` serde:** the local adapter is a fine
  bridge. If multiple secret-bearing fields land (API tokens, OAuth
  secrets, etc.), consider flipping the workspace `zeroize` `serde`
  feature and deleting the adapter.
- **openfang-cli:** as noted in the plan, no CLI work was needed.
