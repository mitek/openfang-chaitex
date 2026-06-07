# Plan 01-08 — skill_manage tool + kernel adapters — SUMMARY

**One-liner.** The agent-facing `skill_manage` tool is wired into the runtime dispatch + schema (under W3 anchor comments), gated by a new `capabilities.allow_skill_mutation` config flag (off by default), and the kernel-side `AuditAppend` / `SkillEventBus` adapters from plan 01-05 are implemented and wired so every mutation lands in the Merkle audit chain and emits a `SkillUpdated` event on a broadcast channel plan 01-09 will subscribe to.

## What was implemented

- New `KernelCapabilities` struct on `KernelConfig` with `#[serde(default)]` and a single field `allow_skill_mutation: bool` (default `false`). TOML round-trip tests prove the empty-block + explicit-`true` paths both deserialize correctly.
- Two new `SkillRegistry` mutation wrappers (Rule 3 mechanical addition — the action set in the plan exceeds the six methods from 01-05): `delete_skill` (calls `check_mutable("delete")` then removes the in-memory entry + on-disk directory + audits + publishes `SkillUpdated`) and `remove_skill_file` (mirrors `write_skill_file`; gated by `check_mutable("remove_file")` + path-traversal rejection + audit). Both honour the Protected/Immutable invariants from plan 01-07.
- `KernelHandle` trait gains two new accessor methods with safe defaults (`kernel_config() -> Option<&KernelConfig>` and `skill_registry() -> Option<&RwLock<SkillRegistry>>`), overridden on `OpenFangKernel`. No test fakes needed updating because the defaults are `None`.
- `tool_skill_manage` async dispatcher in `openfang-runtime::tool_runner`, wrapped in `// === PHASE 1 PLAN 01-08 skill_manage ===` / `// === END PHASE 1 PLAN 01-08 ===` anchor blocks. Capability gate runs first for non-`list` actions. Each action is dispatched to the registry. Success returns `{action, name, ok:true, skill_refresh_required:true}` (the SP-04 sentinel plan 01-09 reads). Registry errors are mapped to structured JSON via `skill_error_to_json` (preserves `Protected`/`Immutable` hints verbatim). Schema added inside the matching `// === PHASE 1 PLAN 01-08 skill_manage schema ===` anchor block.
- New `openfang-kernel::skill_adapters` module with two adapters and a tiny `SkillUpdated { name }` payload type:
  - `KernelAuditAppender` serializes the registry-supplied payload and calls `audit_log.record("kernel", AuditAction::ConfigChange, ...)`.
  - `KernelSkillEventBus` wraps a `tokio::sync::broadcast::Sender<SkillUpdated>`. `publish_skill_updated(name)` is sync (non-blocking); subscriber-less channels degrade to a `tracing::debug!` so a mutation never fails because nobody is listening.
- New `pub skill_updated_tx: broadcast::Sender<SkillUpdated>` field on `OpenFangKernel`, initialized at boot. Plan 01-09's subscriber will call `kernel.skill_updated_tx.subscribe()`.
- Boot wiring: after the existing hand-registry audit callback installation in `boot_with_config`, the registry is given the two adapters via `set_audit_appender(...)` + `set_event_bus(...)`.

## Files changed

- `crates/openfang-types/src/config.rs` — `KernelCapabilities` + field on `KernelConfig` + 3 tests.
- `crates/openfang-skills/src/registry.rs` — `delete_skill` + `remove_skill_file`.
- `crates/openfang-runtime/src/kernel_handle.rs` — new `kernel_config()` and `skill_registry()` accessors with safe defaults.
- `crates/openfang-runtime/src/tool_runner.rs` — anchored dispatch arm + schema + `tool_skill_manage` + `mutation_ok` + `skill_error_to_json` + `SkillFakeKernel` test fake + 9 tests.
- `crates/openfang-kernel/src/lib.rs` — `pub mod skill_adapters`.
- `crates/openfang-kernel/src/skill_adapters.rs` — new file (adapters + tests).
- `crates/openfang-kernel/src/kernel.rs` — new `skill_updated_tx` field + struct-literal entry + boot-time wiring of the two adapters + `KernelHandle` overrides.

## Tests added (15)

In `openfang-types::config::tests`:
- `kernel_capabilities_default_allow_skill_mutation_false`
- `kernel_config_without_capabilities_block_defaults_off`
- `kernel_config_with_capabilities_block_deserializes`

In `openfang-kernel::skill_adapters::tests`:
- `audit_appender_serializes_payload_into_detail`
- `event_bus_send_with_subscriber_succeeds`
- `event_bus_send_without_subscriber_is_a_silent_debug`

In `openfang-runtime::tool_runner::tests`:
- `skill_manage_list_returns_empty_when_registry_empty`
- `skill_manage_list_returns_skills_with_flags`
- `skill_manage_create_invokes_registry_and_sets_refresh_required`
- `skill_manage_patch_modifies_skill_and_signals_refresh`
- `skill_manage_patch_on_protected_returns_structured_error`
- `skill_manage_blocked_when_capability_off`
- `skill_manage_list_works_when_capability_off`
- `skill_manage_delete_removes_and_signals_refresh`
- `skill_manage_unknown_action_returns_structured_error`

## Deviations vs plan

- **[Rule 3 — mechanical]** Plan lists 7 actions (`create | patch | edit | delete | write_file | remove_file | list`), but plan 01-05 only delivered 6 SkillRegistry mutation methods — there was no `delete_skill` and no `remove_skill_file`. Added both as thin wrappers in `openfang-skills::registry`: each goes through `check_mutable` (so Protected/Immutable skills cannot be deleted), audits the event, and for `delete_skill` publishes `SkillUpdated` (`remove_skill_file` doesn't, mirroring `write_skill_file`'s "manifest unchanged → no snapshot refresh" semantics). The existing `SkillRegistry::remove` is left unchanged (used by other call sites that don't want check_mutable).
- **[Rule 3 — mechanical]** Plan called for a `KernelCapabilities` sub-struct that didn't yet exist in `KernelConfig`. Created it (single field `allow_skill_mutation`) and added the matching `Default` impl entry on `KernelConfig`. Derived `Default` directly on `KernelCapabilities` after the first clippy run flagged `derivable_impls`.
- **[Decision — see below]** The plan's `tool_skill_manage` signature was `async fn tool_skill_manage(input, kernel: &dyn KernelHandle) -> Result<String, String>`; the actual signature uses `kernel: Option<&Arc<dyn KernelHandle>>` to match every other kernel-touching tool in tool_runner.rs (lines 306–307 etc.) and to allow the no-kernel branch to return a structured `KernelUnavailable` JSON error instead of panicking. Behaviorally identical for the happy path.
- **[Decision]** The schema uses `"required": ["action"]` per the plan. Per-action `name` requirement is enforced inside `tool_skill_manage` so `list` doesn't need a name. Each non-`list` arm validates with `?` and returns a Tool-runner `Err(...)` for missing-required-field cases (e.g. "Missing 'name' parameter"), matching the convention used by `tool_skill_describe`.

## Decisions made during execution

1. **Kernel registry exposure via accessor, not Arc field refactor.** Plan suggested adding `KernelHandle::skill_registry_mut() -> Arc<RwLock<SkillRegistry>>` "if missing, or use whatever pattern kernel.rs already exposes." The kernel currently holds `pub skill_registry: std::sync::RwLock<SkillRegistry>` (not Arc-wrapped). Rather than refactor every consumer in `kernel.rs` to use `Arc<RwLock<...>>`, I added `fn skill_registry(&self) -> Option<&std::sync::RwLock<SkillRegistry>>` to the trait with a `None` default and overrode it on `OpenFangKernel` returning `Some(&self.skill_registry)`. This keeps the kernel field shape unchanged and avoids a workspace-wide ripple.
2. **`write_skill_file`/`remove_skill_file` use a read lock.** Both methods take `&self` on the registry (file-system mutation happens through `Self::write_atomic`/`std::fs::remove_file`, not through registry-state mutation), so a `RwLock` read lock is sufficient and lets concurrent reads happen during a file-only write.
3. **Audit event tag.** Skill mutation events are tagged as `AuditAction::ConfigChange` in the Merkle chain (the closest existing variant). The structured payload is JSON-encoded into `detail` with a `skill_event=<event_type>` prefix so a later analyzer can grep. Adding a new `SkillMutation` variant would force every persisted audit row decoder to grow a fallback arm.
4. **`SkillUpdated` payload type lives in `skill_adapters`.** Stored on the kernel as `broadcast::Sender<SkillUpdated>`. Plan 01-09 will own the subscriber half. Keeping the payload in `openfang-kernel` (not `openfang-types`) avoids a cycle: the registry depends on the runtime indirectly via the trait seam, and the adapter is the only kernel-side reader/writer.
5. **`Protected` hint surfaced as both `message` (the Display string) and a top-level `hint` field** so JSON consumers don't have to scrape the Display body — they can read `hint` directly.

## Follow-ups for W4

- **01-09 (snapshot refresh signal):** subscribe to `kernel.skill_updated_tx` in the agent loop. On each `SkillUpdated { name }` event, refresh the agent's registry snapshot before the next tool dispatch. The `skill_refresh_required: true` sentinel inside `skill_manage`'s result JSON is the in-band hint the loop reads to know the next dispatch needs a snapshot.
- **01-09 (callsite update):** the plan-08 spec mentions a future `ToolOutcome { result, skill_refresh_required }` typed return shape. For W3, the sentinel is in-band in the JSON result. 01-09 should add the typed shape at the call sites in `agent_loop.rs` without changing the tool function signature.
- **Dashboard:** the `[capabilities]` section is brand-new; the dashboard's config editor (if any) will need a toggle. Not in W3 scope.
- **01-16 (live integration checkpoint):** validate end-to-end that an agent can call `skill_manage(action="create", ...)` against a daemon with `[capabilities] allow_skill_mutation = true`, the file lands in `~/.openfang/skills/`, and a follow-up `list` includes it.

## Workspace gates (after this plan)

- `cargo build --workspace --lib` — clean.
- `cargo test --workspace` — 2793 passed (was 2778 before this plan; +15).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.

## Commits

- `950e53a` — feat(01-08): skill_manage tool + kernel adapters
