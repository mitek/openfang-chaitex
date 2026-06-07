# 01-05 — SkillRegistry mutation methods — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** 3d49155

## One-liner

Six mutation methods (`create_skill`, `patch_skill`, `edit_skill`,
`write_skill_file`, `reload_skill`, `set_skill_enabled`) landed on
`SkillRegistry`, each driving the SP-02 pipeline (scan → SHA256 + audit
append → TOML validate → config resolve → atomic write → reload →
SkillUpdated event), plus two trait seams (`AuditAppend`, `SkillEventBus`)
that kernel-side adapters in plan 01-08 will implement without inverting
the crate DAG.

## Files changed

- `crates/openfang-skills/src/lib.rs` — `SkillError::Protected` /
  `SkillError::Immutable` variants (carry `name`, `action`, `hint`).
- `crates/openfang-skills/src/registry.rs` — trait declarations, new
  registry fields + setters, six mutation methods, helpers, and 16 tests.

## How audit/event-bus wiring is exposed to the registry

Per CONVENTIONS.md the registry can't depend on `openfang-runtime`'s
audit log without inverting the crate DAG, so this plan introduces two
local trait seams:

- `pub trait AuditAppend: Send + Sync` —
  `fn append(&self, event_type: &str, payload: serde_json::Value) -> Result<(), SkillError>`.
- `pub trait SkillEventBus: Send + Sync` —
  `fn publish_skill_updated(&self, name: &str)`.

Both stored on the registry as
`Option<Arc<dyn Trait>>`. The kernel boot path (plan 01-08) implements
adapters: the audit one forwards into the existing Merkle
`audit_entries` table; the event-bus one publishes onto whatever bus the
agent loop is already using to listen for snapshot refresh (plan 01-09).

Setters: `set_audit_appender(Arc<dyn AuditAppend>)` and
`set_event_bus(Arc<dyn SkillEventBus>)`. Called once during kernel boot.
Tests pass `None` (the default) and the mutation methods fall back to a
debug log so unit tests can run without wiring anything.

## Tests added

16 new tests in `registry::tests`:

1. `create_then_list_shows_skill` — happy path, asserts `mutable = true`
   is patched in on disk, prompt_context.md exists, audit + event
   counters incremented.
2. `create_rejects_duplicate_name` — `AlreadyInstalled`.
3. `patch_replaces_string_in_skill_toml` — single replacement, audit +
   events recorded.
4. `patch_rejects_multiple_matches_without_replace_all` — count > 1
   without `replace_all` returns InvalidManifest mentioning the count
   and `replace_all=true`.
5. `patch_replace_all_replaces_every_occurrence`.
6. `patch_rejects_unknown_old_string` — count == 0 → not found.
7. `edit_replaces_entire_manifest`.
8. `edit_rejects_name_mismatch`.
9. `write_skill_file_writes_relative`.
10. `write_skill_file_rejects_traversal` — `../etc/passwd`.
11. `write_skill_file_rejects_absolute` — Windows-aware fixture so the
    test passes on both POSIX and Windows.
12. `reload_skill_picks_up_external_edit`.
13. `set_skill_enabled_false_hides_from_list` — also asserts `list_all`
    still surfaces it and the file remains on disk.
14. `set_skill_enabled_true_re_exposes`.
15. `every_mutation_emits_audit_and_event` — SP-05 contract.
16. `create_blocks_critical_prompt_injection` — happy-path security
    invariant: critical content stops the write.

All 102 `openfang-skills` tests pass (was 86); workspace gates clean
(build / test / clippy `-D warnings`).

## Deviations

**[Rule 1 — bug fix]** First clippy run flagged a `format!()` with no
arguments in `create_blocks_critical_prompt_injection`. Fixed inline by
turning it into a raw-string literal. No behavioral change.

**[Rule 3 — auto-fix blocking]** The original `SkillRegistry` had
`#[derive(Debug, Default)]`. Adding `Option<Arc<dyn Trait>>` fields
breaks the derive (trait objects aren't `Debug` derive-compatible). Added
hand-written `Debug` and `Clone` impls (printing `<dyn AuditAppend>` /
`<dyn SkillEventBus>` placeholders for the Arcs) and kept `#[derive(Default)]`
— `Option<Arc<...>>` defaults to `None` cleanly.

**[Rule 2 — added a critical helper]** Added `list_all()` so the
dashboard can still show disabled skills (the plan only mandated
filtering `list()`, but disabled skills need to be discoverable for the
re-enable UX). Both `list_all` and the existing `list` share the same
HashMap; cost is one extra trivial accessor.

## Decisions made during execution

1. **`check_mutable` is a stub.** The plan said "always returns Ok for
   now"; the doc comment on the method explicitly cross-references plan
   01-07 where the body lands. Wiring it as a stub means plan 01-08 can
   register the `skill_manage` tool today against the stable shape.
2. **`mutable=true` patching at create-time.** The plan's last paragraph
   on `create_skill` says "Sets `mutable=Some(true)` if the manifest
   didn't specify". I implemented that — when the user-provided
   `skill.toml` omits `mutable`, the registry patches it in and
   re-serializes before writing to disk. That way a later `load_skill`
   reads the explicit value, not the registry-side default.
3. **Atomic write: same-directory tmp.** `path.with_extension("tmp")`
   would replace the extension (`skill.toml` → `skill.tmp`), which
   breaks for files without a parent directory or with multiple dots in
   their names. Used `path.with_file_name(name + ".tmp")` so the tmp
   file lives next to the destination (rename stays on the same volume
   — atomic on both POSIX and modern Windows).
4. **`write_skill_file` doesn't reload or emit.** It writes auxiliary
   files (`references/notes.md`, `examples/x.py`) that don't change the
   manifest, so the agent's tool view doesn't need to refresh. Audit
   still appends so a forensic trail exists.
5. **Path traversal rejection** uses `std::path::Component`-walking,
   not just `contains("..")`. Catches `foo/../bar/baz`,
   `./../../etc/passwd`, and Windows `C:\Windows\win.ini` (`Prefix`).
6. **Trait names.** Chose `AuditAppend` (verb-noun) and `SkillEventBus`
   (subject-noun) over generic `EventBus` — the registry will only ever
   publish skill events, so scoping the trait name keeps the
   responsibility crisp. Plan 01-08 can compose a kernel-side
   `KernelEventBus` that delegates to the right registries.

## Follow-ups for later plans

- **01-07 (mutable/protected defaults):**
  - Apply the load-time defaults in `load_bundled` / `load_skill`:
    `InstalledSkill` needs effective `mutable` / `protected` fields
    distinct from the manifest. Bundled → `mutable=false`;
    `SYSTEM_SKILLS` → `protected=true`; user-created → `mutable=true`.
  - Fill `check_mutable`'s body. Return `SkillError::Protected { ... }`
    or `SkillError::Immutable { ... }` with crisp `hint` strings the
    `skill_manage` tool can surface verbatim.
- **01-08 (`skill_manage` tool + kernel wiring):**
  - Implement an `Arc<dyn AuditAppend>` adapter that calls the kernel
    audit log; pass it to `SkillRegistry::set_audit_appender(...)`
    during boot.
  - Implement an `Arc<dyn SkillEventBus>` adapter that publishes onto
    the existing snapshot-refresh channel; pass to
    `SkillRegistry::set_event_bus(...)`.
  - Register the `skill_manage` tool. Its result must include
    `skill_refresh_required: true` (SP-04) so the agent loop snapshots
    the registry before the next tool dispatch.
- **01-09 (snapshot-refresh signal):** the bus from this plan is half
  the wire; 01-09 wires the consumer side in the agent loop so
  `SkillUpdated` triggers a fresh `snapshot()`.
- **CONCERNS.md:** the `list()` semantic change (now filters on
  `enabled`) is a minor behavior change for any caller that previously
  relied on disabled-skill visibility. None found in the workspace
  today, but worth surfacing if the dashboard ever cares — `list_all`
  is the explicit replacement.
