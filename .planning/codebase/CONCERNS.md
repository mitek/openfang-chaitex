# Codebase Concerns

**Analysis Date:** 2026-06-06
**Repo:** `openfang-chaitex` (fork of `RightNow-AI/openfang` v0.6.9)
**Scope:** 14 workspace crates, ~198K LOC of Rust, in preparation for Phase 1 self-learning (skill self-patching, memory reasoning, FTS5 session search).

This document captures debt, fragility, and gaps that Phase 1 work must respect or fix. Items prefixed with **[PHASE-1]** are direct blockers/risks for the planned self-learning additions per `docs/chaitex/phase1-self-learning-architecture.md`.

---

## Tech Debt

### [PHASE-1] SkillRegistry has no mutation API today
- Issue: `SkillRegistry` in `crates/openfang-skills/src/registry.rs` exposes only `load_bundled()`, `load_all()`, `load_skill()`, `load_workspace_skills()`, `remove()`, and read methods. It has **no** `create_skill`, `patch_skill`, `edit_skill`, `write_skill_file`, `reload_skill`, or `set_skill_enabled`. The whole struct is also `Debug, Default` — no `Clone` derive — but `snapshot()` does a deep `HashMap::clone()` manually.
- Files: `crates/openfang-skills/src/registry.rs`
- Impact: Phase 1 design assumes ~6 new mutating methods land here. The current code is also not thread-safe internally — concurrency is handled externally via `std::sync::RwLock<SkillRegistry>` in `crates/openfang-kernel/src/kernel.rs:94`.
- Fix approach: Add the methods called out in `docs/chaitex/phase1-self-learning-architecture.md` § 1.2. Each mutating method must (a) re-run `SkillVerifier::scan_prompt_content` (`crates/openfang-skills/src/verify.rs:109`), (b) honour `frozen` flag (Stable mode), (c) be atomic on disk (tmp+rename), (d) emit `SkillUpdated` via `event_bus` (`crates/openfang-kernel/src/kernel.rs:68`).

### [PHASE-1] `SkillRegistry::snapshot()` is taken once at the start of the agent turn — mid-turn mutations are invisible
- Issue: `crates/openfang-kernel/src/kernel.rs:2154-2169` (and `:2734`) takes a clone of the registry **before** the agent loop runs. Tool execution inside the loop uses that frozen snapshot via `tool_runner::execute_tool(skill_registry: Option<&SkillRegistry>, ...)` (`crates/openfang-runtime/src/tool_runner.rs:116`). If a future `skill_manage` tool patches a skill mid-turn, the next iteration of the **same** turn won't see the change. Subsequent turns will.
- Files: `crates/openfang-kernel/src/kernel.rs:2154`, `crates/openfang-kernel/src/kernel.rs:2734`, `crates/openfang-runtime/src/tool_runner.rs:116`
- Impact: Predictable but counter-intuitive for the agent. Documented in Phase 1 design but must be either accepted or fixed by routing `skill_manage` writes through the live `RwLock<SkillRegistry>` and re-snapshotting after every successful mutation.
- Fix approach: Either (a) document the per-turn freeze invariant, (b) re-snapshot inside the agent loop after a successful `skill_manage` write, or (c) hold an `Arc<RwLock<SkillRegistry>>` reference and re-read on each tool dispatch (loses Send-safety guarantees the current snapshot was built to provide — see comment at `registry.rs:53`).

### [PHASE-1] Tool registration is duplicated in three places
- Issue: Adding a new built-in tool (`skill_manage`, `memory_reason`, `session_search`) requires touching THREE files:
  1. Dispatch match arm in `crates/openfang-runtime/src/tool_runner.rs:203` (`execute_tool`).
  2. Schema entry in `crates/openfang-runtime/src/tool_runner.rs:567` (`builtin_tool_definitions`).
  3. Tool-profile allowlist in `crates/openfang-types/src/tool_profile.rs` (filtering for Chat / Full / Hand profiles).
  Plus capability gate (`available_tools()` in kernel) and possibly `requires_approval()` (`crates/openfang-kernel/src/approval.rs:47`).
- Files: `crates/openfang-runtime/src/tool_runner.rs` (5014 lines, single file), `crates/openfang-types/src/tool_profile.rs`, `crates/openfang-kernel/src/kernel.rs:7572`
- Impact: Easy to forget a step → tool compiles but is dead at runtime (the exact failure mode CLAUDE.md warns about). The `execute_tool` signature already has 17 parameters (`#[allow(clippy::too_many_arguments)]` at line 108) — adding another context (`Arc<RwLock<SkillRegistry>>` for skill_manage, `Arc<ReasoningEngine>` for memory_reason) will push it past 20.
- Fix approach: Phase 1 design proposes `crates/openfang-runtime/src/tools/` directory with one `Tool` trait impl per file. That refactor should land before adding the new tools, or at minimum a `ToolContext` struct should replace the long argument list.

### [PHASE-1] No FTS5 feature enabled on rusqlite
- Issue: `Cargo.toml:58` declares `rusqlite = { version = "0.31", features = ["bundled", "serde_json"] }`. The `"fts5"` feature is **not** enabled, so `CREATE VIRTUAL TABLE ... USING fts5(...)` will fail at runtime even though it compiles.
- Files: `Cargo.toml:58`, `crates/openfang-memory/Cargo.toml:18`
- Impact: Phase 1's Component 2 (`session_messages_fts` virtual table) and Component 3 (`session_search` tool) cannot work without this. Must be added in the same PR that introduces the v9 migration.
- Fix approach: `features = ["bundled", "serde_json", "fts5"]` in the workspace `Cargo.toml`. This pulls in the FTS5 module of bundled SQLite. Verify the bundled SQLite version is recent enough (>=3.20) — should be fine for rusqlite 0.31.

### [PHASE-1] Sessions stored as msgpack BLOB — incompatible with FTS5
- Issue: `crates/openfang-memory/src/session.rs:78-100` serializes `Vec<Message>` to `rmp_serde::to_vec_named` and stores into `sessions.messages BLOB`. FTS5 only indexes TEXT columns. Either a parallel TEXT-per-message table is needed, or a trigger that decodes the BLOB on insert (which FTS5 cannot do because it has no msgpack support).
- Files: `crates/openfang-memory/src/session.rs:51-97`, `crates/openfang-memory/src/migration.rs:88-96` (v1 schema), `crates/openfang-memory/src/migration.rs:255-271` (v5 `canonical_sessions`)
- Impact: The trigger sketch in Phase 1 architecture (§ 2.5) **cannot work as written** — it assumes a `messages` table with a `content` TEXT column per row, but today there's only a per-session blob. Will require schema change: either (a) new `messages(id, session_id, agent_id, role, content TEXT, created_at)` flat table with FTS5 mirroring, or (b) `session_messages_fts` populated by application code on `save_session`.
- Fix approach: Add a v9 migration that creates a row-per-message table (`session_messages` with TEXT content) plus the FTS5 virtual table, then backfill from existing BLOBs. Keep the BLOB writeback for compatibility but treat the flat table as source of truth for search.

### [PHASE-1] Migration tests only verify schema creation, not version transitions
- Issue: `crates/openfang-memory/src/migration.rs:335-362` has only `test_migration_creates_tables` (fresh DB) and `test_migration_idempotent` (runs twice). There is no test that bootstraps at v1, then runs `migrate_v5`, then `migrate_v6`, etc., on a DB populated with realistic data. The `column_exists` guard at line 56 was added specifically because v6 (`migrate_v6`) re-ran `ALTER TABLE ADD COLUMN` on already-migrated DBs and crashed.
- Files: `crates/openfang-memory/src/migration.rs`
- Impact: When Phase 1 adds v9 (FTS5 + flat messages table), upgrading existing user DBs may silently corrupt or fail. The existing user base for ChaiTex fork is small but the upstream fork rule still applies.
- Fix approach: Add per-version migration tests that (a) write some rows at version N, (b) run the migration to N+1, (c) verify data round-trips. Each new migrate_vN must come with such a test.

### Config error path silently falls back to defaults
- Issue: `crates/openfang-kernel/src/config.rs:84-97` has an explicit `TODO(GAP-012-Tier-2)`: when TOML parses but `try_into::<KernelConfig>()` fails (e.g. wrong type, missing required field that was a non-`#[serde(default)]`), the kernel logs a warn and proceeds with `KernelConfig::default()`. The user thinks their config is loaded but it isn't.
- Files: `crates/openfang-kernel/src/config.rs:78-115`
- Impact: A typo in `~/.openfang/config.toml` can silently disable reasoning budget limits, channels, MCP servers, etc. Especially severe for Phase 1: the new `[reasoning]` section in `config.toml` has a `monthly_budget_usd` field whose silent omission becomes a cost incident.
- Fix approach: Tier 2 fix mentioned in the TODO: surface deserialization failures via `/api/health/detail` and stderr banner. For the reasoning budget specifically, gate the feature on successful config load rather than fall back.

### `openfang-cli/src/main.rs` is 7478 lines in a single file
- Issue: One file holds the entire CLI: arg parsing, daemon control, init wizard, doctor, hand/skill/channel CRUD, MCP setup, etc.
- Files: `crates/openfang-cli/src/main.rs`
- Impact: Hard to navigate, hard to test handlers in isolation. The CHAITEX `CLAUDE.md` says "Don't touch openfang-cli — user is actively building the interactive CLI" — so Phase 1 must avoid adding things here.
- Fix approach: Not in scope for Phase 1. Note for future: split into `main.rs` + `commands/` subdirectory mirroring the clap subcommand tree.

### `openfang-api/src/routes.rs` is 12975 lines with 190 handlers
- Issue: Single file holds every HTTP handler. Adding new endpoints for skill_manage, memory_reason, reasoning budget, session_search compounds this.
- Files: `crates/openfang-api/src/routes.rs`
- Impact: Compile times, merge conflicts, cognitive load. The CHAITEX gotcha "New routes must be registered in server.rs router AND implemented in `routes.rs`" is a direct symptom of this monolith.
- Fix approach: Split by resource (`routes/agents.rs`, `routes/skills.rs`, `routes/memory.rs`, ...). Phase 1 should at least put new handlers in a new `routes/learning.rs` rather than appending to the existing file.

### Hand bundled parse panics on malformed manifest (test code)
- Issue: `crates/openfang-hands/src/bundled.rs:238` panics with `panic!("Failed to parse hand '{}': {}", id, e)` — but this is inside `#[test] all_bundled_hands_parse`, so it only affects CI.
- Files: `crates/openfang-hands/src/bundled.rs:234-245`
- Impact: None in production; just listed for completeness because the grep flagged it.
- Fix approach: Leave it. Tests should panic on invariant violation.

---

## Known Bugs

### Workspace skill auto-conversion does not run `apply_skill_config`
- Issue: `SkillRegistry::load_workspace_skills` at `crates/openfang-skills/src/registry.rs:378-467` runs the SKILL.md → skill.toml conversion path **without** calling `apply_skill_config` on the converted manifest. By contrast `load_all` (`:182-272`) does the same conversion and then falls through to `load_skill` which **does** apply config. As a result, workspace-scoped skills that declare a `config:` frontmatter section will be loaded but their config block is never injected into `prompt_context`.
- Files: `crates/openfang-skills/src/registry.rs:378-467` vs `crates/openfang-skills/src/registry.rs:275-304`
- Trigger: Drop a workspace-scoped SKILL.md (no skill.toml) with a `config:` section into `<workspace>/skills/`.
- Workaround: Convert the SKILL.md to skill.toml + prompt_context.md manually before placing it in the workspace.

### `Failed to deserialize merged config` silently falls back to defaults
- See "Config error path silently falls back to defaults" above. This is both debt and an active bug because there is no user-visible signal.

### `OPENFANG_ALLOW_NO_AUTH=1` previously bypassed auth (fixed but residual risk)
- Issue: Fixed in v0.5.10 per `CHANGELOG.md` and `crates/openfang-api/src/middleware.rs:148-170` (#1034). Empty `api_key` now returns 401 unless `OPENFANG_ALLOW_NO_AUTH=1`. But the env-var-based bypass exists and is not logged at WARN level on every request — only at startup.
- Files: `crates/openfang-api/src/middleware.rs:49-237`
- Trigger: Operator sets `OPENFANG_ALLOW_NO_AUTH=1` intentionally for local dev, forgets to unset for deployment.
- Workaround: Audit the env at startup; reject if both `OPENFANG_ALLOW_NO_AUTH=1` and a non-loopback bind address are set.

---

## Security Considerations

### `api_key` is stored as plain `String`, not `Zeroizing<String>`
- Risk: The dashboard auth bearer token lives in memory as a regular `String` and is cloned freely (`auth_state.api_key.trim().to_string()` at `crates/openfang-api/src/middleware.rs:154`). Crash dumps and `Debug` impls may leak it.
- Files: `crates/openfang-types/src/config.rs:1168`, `crates/openfang-api/src/middleware.rs:49`
- Current mitigation: Constant-time comparison via `subtle::ConstantTimeEq` at `:189` and `:206`. SECURITY.md claims "`Zeroizing<String>` on all API key fields" but this is the master API key — it's not zeroized.
- Recommendations: Wrap `api_key: String` with `Zeroizing<String>` like `crates/openfang-runtime/src/embedding.rs:66` does for embedding keys. Audit every `.clone()` that follows.

### Phase 1 expands the attack surface: agent-writable filesystem and patch tools
- Risk: `skill_manage` will let the agent write into `~/.openfang/skills/` (and indirectly into `~/.openfang/skills/<name>/skill.toml`, `prompt_context.md`, plus arbitrary subpaths via `write_skill_file`). A prompt-injected agent could (a) exfiltrate other skills' prompts, (b) plant a malicious tool that runs on next reload, (c) overwrite `protected = true` if checks have a TOCTOU window.
- Files: planned `crates/openfang-skills/src/registry.rs` additions; `crates/openfang-runtime/src/tool_runner.rs` new `skill_manage` arm.
- Current mitigation: Phase 1 design § 1.7 specifies `mutable`/`protected` flags and a `check_mutable` gate. `SkillVerifier::scan_prompt_content` (`crates/openfang-skills/src/verify.rs:109`) runs on every load.
- Recommendations:
  1. Enforce `check_mutable` **before** any disk I/O (no TOCTOU).
  2. Constrain `write_skill_file`'s `file_path` parameter with `safe_resolve_path` so the agent can't escape the skill dir via `..` or symlinks.
  3. Reject any patch that flips `protected = false` regardless of current `mutable` value.
  4. Run `SkillVerifier::scan_prompt_content` on the **new** content before write, not just on reload.
  5. Block `skill_manage` from operating on `skill-manage` itself (the patching tool's own skill, if any) — same self-protection logic Hermes uses.
  6. Treat the registry's `frozen` flag as authoritative: in Stable mode (`KernelMode::Stable`, `crates/openfang-kernel/src/kernel.rs:892`), all `skill_manage` writes must reject.

### Phase 1 Memory Reasoning: cost-driven prompt injection
- Risk: `memory_reason(level="max")` triggers 3–5 chained LLM calls costing up to $1+ per call per the design doc. A prompt-injected agent could call it in a loop to drain the user's monthly budget, or smuggle exfiltration intent into the chain-of-thought prompt.
- Files: planned `crates/openfang-reasoning/`; `crates/openfang-runtime/src/tool_runner.rs` new `memory_reason` arm.
- Current mitigation: Design § 2.4.1 specifies `monthly_budget_usd` config, `require_approval_for_max`, and `max_level` ceiling.
- Recommendations:
  1. Wire the budget enforcement into the existing `metering` / budget system (`/api/budget` endpoints from `CLAUDE.md`) rather than a parallel tracker.
  2. Rate-limit `memory_reason` per agent (e.g. 10/session for medium, 3/session for high) at the tool layer, not just suggest it in the system prompt.
  3. Any user-supplied text reaching the chain-of-thought prompt must be tagged with `TaintLabel::ExternalNetwork` / `Untrusted` and the synthesizer prompt should refuse to execute embedded "instructions".

### Bedrock/etc. drivers panic on malformed responses (test panics only — confirmed)
- All `panic!` calls in `crates/openfang-runtime/src/drivers/*.rs` are inside `#[cfg(test)]` modules (verified by grep on lines 917+, 962+, 1033+, 1066+, 1100+, 1112+, 1150+, 1174+ in anthropic.rs, 1054+, 1175+ in bedrock.rs, 1311+, 1446+, 1464+, 1532+, 1570+, 1620+, 1654+, 1692+ in gemini.rs). Production paths use `Result`. Listed here only to assert the safety check passed.

### Two `unsafe` blocks call libc and Win32 directly
- Risk: `crates/openfang-cli/src/main.rs:42-51` registers a Windows console Ctrl handler. `crates/openfang-kernel/src/kernel.rs:5241-5243` calls `libc::kill(pid as i32, libc::SIGTERM)` for the WhatsApp gateway. Other `unsafe` sites are in `crates/openfang-api/src/routes.rs:2755, 2839` (need inspection) and `crates/openfang-api/tests/skill_config_api_test.rs:142, 314` plus `crates/openfang-skills/src/config_injection.rs:205, 217` — all four are `std::env::set_var`/`remove_var` calls which became `unsafe` in Edition 2024.
- Files: `crates/openfang-cli/src/main.rs:38-51`, `crates/openfang-kernel/src/kernel.rs:5241`, `crates/openfang-api/src/routes.rs:2755, 2839`
- Current mitigation: Each unsafe block is small and well-scoped to specific syscall / env-var operations.
- Recommendations: Audit `crates/openfang-api/src/routes.rs:2755, 2839` (not yet read — verify they are also env-var manipulation, not raw FFI). Document why `set_var`/`remove_var` is needed in long-running multi-threaded code (it's racy and Edition 2024 marked it unsafe for that reason).

### `kernel.skill_registry.write().unwrap()` propagates lock poison as panic
- Risk: If a thread panics while holding the write lock, `unwrap()` re-panics in every subsequent caller. The kernel does use `.unwrap_or_else(|e| e.into_inner())` at `crates/openfang-kernel/src/kernel.rs:6335` for reload — but `:9242` (test) and other call sites use plain `.unwrap()`.
- Files: `crates/openfang-kernel/src/kernel.rs:6335`, others
- Recommendations: Standardize on `.unwrap_or_else(|e| e.into_inner())` for all `RwLock` accesses in production paths. Phase 1's new mutating methods on the registry will multiply these access sites.

---

## Performance Bottlenecks

### Single SQLite connection wrapped in `Arc<Mutex<Connection>>` serializes all memory operations
- Problem: Every read and write to memory substrate takes the same mutex.
- Files: `crates/openfang-memory/src/session.rs:29-43` (`SessionStore`), and 22 `conn.lock()` sites across `crates/openfang-memory/src/`.
- Cause: `rusqlite::Connection` is not `Send + Sync`; the workaround is a global mutex. WAL mode is enabled (`crates/openfang-memory/src/substrate.rs:52` — `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;`), which allows concurrent **readers** at the SQLite level, but the `Mutex<Connection>` still serializes everything at the Rust level.
- Improvement path: Use a connection pool (`r2d2` or `deadpool-sqlite`). Particularly important once FTS5 search is added — a long-running search query will block all other memory ops.

### Loading sessions deserializes the whole conversation BLOB
- Problem: `crates/openfang-memory/src/session.rs:62` `rmp_serde::from_slice(&messages_blob)` parses every message in the session into `Vec<Message>` even when only the last message is needed. As session length grows (and per `CHANGELOG.md` 0.1.0 default context window is now 200K tokens), this is O(session size) on every read.
- Files: `crates/openfang-memory/src/session.rs:40-75`
- Cause: BLOB storage chosen for write simplicity.
- Improvement path: The Phase 1 FTS5 schema redesign (flat `session_messages` table) gives us per-row reads for free. Migration path: keep BLOB as canonical for now, populate flat table for search; later flip authority.

### `crates/openfang-kernel/src/kernel.rs` is 9415 lines and holds the global hot path
- Problem: Compile times, especially incremental. Every Phase 1 wiring change (peer init, channel bridge init, hand init, skill init, ...) goes through this file.
- Cause: Organic growth across 16+ subsystems.
- Improvement path: Out of scope for Phase 1. Note: when `ReasoningEngine` is wired in, prefer a separate `crates/openfang-kernel/src/reasoning_init.rs` rather than inlining into `kernel.rs`.

### `crates/openfang-runtime/src/tool_runner.rs` does a string-match dispatch on every tool call
- Problem: 5014 lines, a large `match tool_name { ... }` at `:203` with one arm per tool. For Phase 1 the match grows by 3+ arms. Not a real CPU bottleneck (matches are jump tables) but a cognitive and merge-conflict bottleneck.
- Improvement path: A `HashMap<&'static str, Box<dyn Tool>>` keyed dispatch would let each tool live in its own module and register via `inventory` or a build-script. Speculative — only worth it if/when the count crosses 75 tools.

---

## Fragile Areas

### Tool registration split across three files
- Files: `crates/openfang-runtime/src/tool_runner.rs:203` (execute_tool match), `crates/openfang-runtime/src/tool_runner.rs:567` (builtin_tool_definitions schema list), `crates/openfang-types/src/tool_profile.rs` (profile allowlist).
- Why fragile: All three must be updated in lockstep. Live integration testing (per `CLAUDE.md` Step 4) is the only way to catch the "compiles but is dead" case. Phase 1 adds 3 tools → 9 edit sites.
- Safe modification: Run the live integration test (`CLAUDE.md` Step 5) after every new tool; verify the tool appears in `/api/agents/{id}/tools` AND that an actual LLM call invokes it.
- Test coverage: Tool dispatch is unit-tested per tool function but there is no test that asserts "every name in `builtin_tool_definitions` has a matching arm in `execute_tool`" or vice-versa. **Add such a test as part of Phase 1.**

### Snapshot semantics for skill registry
- Files: `crates/openfang-skills/src/registry.rs:55-63`, `crates/openfang-kernel/src/kernel.rs:2154-2169, 2734`.
- Why fragile: `snapshot()` is a manual `HashMap::clone()` for `Send` safety reasons (the `RwLockReadGuard` is `!Send` and the agent loop crosses `.await`). Adding any field to `SkillRegistry` requires updating `snapshot()` or the new field will be silently dropped on every agent turn.
- Safe modification: Mirror the `Default` impl + struct definition pattern (analogous to the `KernelConfig` gotcha in `CLAUDE.md`). When Phase 1 adds (e.g.) `mutation_journal: Vec<SkillMutation>`, remember to add it to `snapshot()`.
- Test coverage: One test (`test_snapshot_includes_global_skills` at `:638`) checks count, none check the full field set.

### `PeerRegistry` wrapping in `Option<Arc<PeerRegistry>>`
- Files: `crates/openfang-api/src/server.rs:48`, `crates/openfang-kernel/src/kernel.rs:154`.
- Why fragile: `kernel.peer_registry: OnceLock<PeerRegistry>` on the kernel side, but `AppState.peer_registry: Option<Arc<PeerRegistry>>` on the API side. The bridge is `kernel.peer_registry.get().map(|r| Arc::new(r.clone()))` — a clone of the whole registry per API boot, which is fine because boot happens once, but easy to subtly break.
- Safe modification: When adding new shared state across the kernel/API boundary (Phase 1 will likely need `ReasoningEngine` or `SkillMutationJournal` shared), follow the same `OnceLock<T>` on kernel → `Option<Arc<T>>` on AppState pattern, document in `CLAUDE.md`.
- Test coverage: None for the bridge specifically.

### Config struct + Default impl + serde defaults must stay in sync
- Files: `crates/openfang-types/src/config.rs` (4701 lines, 84 structs).
- Why fragile: `CLAUDE.md` explicitly calls this out: "Config fields added to `KernelConfig` struct MUST also be added to the `Default` impl or build fails". Adding `[reasoning]` section for Phase 1 will touch 4+ places in this file alone.
- Safe modification: Always add `#[serde(default)]` and an entry in the relevant `Default` impl in the same commit. Test by loading an empty `config.toml`.
- Test coverage: Some serde round-trip tests exist (e.g. `KernelMode` test at `:4099-4109`) but not for the whole `KernelConfig`.

### TOML config falls back to defaults on any deserialize error
- See "Config error path silently falls back to defaults" above. Once Phase 1's `[reasoning]` section is added with hard-required fields, this fallback could silently disable cost controls.

---

## Scaling Limits

### Sessions table: BLOB-per-session means no per-message indexes
- Current capacity: Tested with sessions up to ~200K tokens (CHANGELOG.md mentions 200K context window default).
- Limit: At ~1M tokens (Claude 200K, Gemini 1M context), the per-turn deserialization becomes hundreds of ms.
- Scaling path: Phase 1 redesign (per-row TEXT messages) addresses this transitively.

### Skill registry held in memory as a `HashMap<String, InstalledSkill>`
- Current capacity: 60 bundled + user skills. CHANGELOG references 60 bundled.
- Limit: A user with 1000+ skills in a workspace would clone the whole map on every agent turn (via `snapshot()`). At ~10 KB per skill manifest, that's a ~10 MB clone per turn — noticeable on the pc162 ARM target (1.9 GB RAM, per `CHAITEX.md`).
- Scaling path: `Arc<InstalledSkill>` inside the map so snapshot is a cheap Arc-clone-per-skill. Already the values are reasonably small.

### Single Tokio runtime, single SQLite connection
- Current capacity: Adequate for single-user local agent.
- Limit: When `memory_reason` (Phase 1) holds the DB mutex for a multi-second FTS5 + semantic search, all other agent ops block.
- Scaling path: Connection pool for memory substrate (see Performance Bottlenecks).

### pc162 ARM target (1.9 GB RAM, Allwinner Cortex-A7)
- Current capacity: OpenFang documented at 116 MB resident per `CHAITEX.md`.
- Limit: Adding `openfang-reasoning` crate, FTS5 index, larger skill set, and chain-of-thought caches could push past 500 MB.
- Scaling path: Phase 1 must benchmark resident memory on pc162 before/after. Disable `memory_reason(level=max)` by default on resource-constrained targets via config flag.

---

## Dependencies at Risk

### `rusqlite = "0.31"` is one major version behind (current is 0.32+)
- Risk: 0.31 → 0.32 had API churn around `params!` and connection types. Upgrading later (when forced by a security CVE) will be a larger lift.
- Impact: Memory crate, migrations.
- Migration plan: Bundle the upgrade with the Phase 1 FTS5 feature change since both touch the same `Cargo.toml` line.

### `wasmtime`, `tauri`, `axum 0.8`, `reqwest 0.12` — fast-moving
- Risk: Major-version churn every 6–9 months.
- Impact: Each major bump tends to cascade through the workspace.
- Migration plan: Track upstream OpenFang merges; let them absorb the dep upgrades and rebase ChaiTex on top.

### `rmp-serde` for message BLOB
- Risk: Schema change in `Message` (adding a non-`Option` field) breaks deserialization of all stored sessions.
- Impact: User loses session history.
- Migration plan: Always add new `Message` fields as `#[serde(default)] Option<T>`. Phase 1 might want to add `embedding: Option<Vec<f32>>` to messages for reasoning — must follow this rule.

---

## Missing Critical Features (for Phase 1)

### No `ReasoningEngine` exists today
- Problem: `MemorySubstrate` (`crates/openfang-memory/src/substrate.rs`) does passive storage + decay only. No active reasoning over accumulated facts.
- Blocks: `memory_reason` tool, user profile inference, Honcho-style features.
- Plan: New `crates/openfang-reasoning/` crate per design doc § 2.1.

### No mechanism to track per-feature LLM spend
- Problem: `/api/budget` is global per agent (`CLAUDE.md` endpoint table). There is no breakdown by feature (reasoning vs. agent turn vs. compactor summarization).
- Blocks: Phase 1's reasoning monthly budget (`monthly_budget_usd`).
- Plan: Extend the existing metering with a `cost_category: "reasoning" | "agent_loop" | "compactor" | "embedding"` tag.

### No "session message" row-level addressability
- Problem: Sessions are BLOBs. No way to reference "session X, message 42" except by deserializing the whole session.
- Blocks: `FactSource::Session { session_id, message_index }` from design doc § 2.2.
- Plan: Phase 1 v9 migration introduces per-message rows.

### `skill_manage` tool does not exist
- Problem: Agents have `skill_list`, `skill_describe`, `skill_execute` (`crates/openfang-runtime/src/tool_runner.rs:482-484`) but no way to create/modify skills.
- Blocks: All of Phase 1 Component 1.
- Plan: New tool per design doc § 1.4.

### `session_search` tool does not exist
- Problem: There is no full-text search over historical sessions today. The closest is `MemorySubstrate::semantic_recall` (vector search), but that requires an embedding model and may miss exact phrasing.
- Blocks: Hermes parity for `session_search`.
- Plan: Phase 1 Component 3 (depends on FTS5).

### Skill `mutable` / `protected` fields do not exist in `SkillManifest`
- Problem: `SkillRegistry` has no concept of immutability per skill.
- Blocks: Migration path in design doc § 1.7.
- Plan: Add fields to the `SkillManifest` struct in `crates/openfang-skills/src/lib.rs` (note: file not opened but referenced from registry imports). Update bundled skill TOMLs and the auto-marking script.

---

## Test Coverage Gaps

### SQLite migration v→v+1 transitions on populated DBs
- What's not tested: `migrate_v2` applied to a DB containing v1 data; `migrate_v6` applied to a DB with v5 sessions; etc.
- Files: `crates/openfang-memory/src/migration.rs:331-362` (only 2 tests).
- Risk: A future Phase 1 v9 migration could silently drop or corrupt user data.
- Priority: **High** — this is a Phase 1 prerequisite.

### Workspace skill loader with `config:` frontmatter
- What's not tested: `load_workspace_skills` with a SKILL.md that declares `config:` vars. Current tests use plain skill.toml.
- Files: `crates/openfang-skills/src/registry.rs:378-467` (the `apply_skill_config` omission bug above).
- Risk: Workspace skills with config silently render with raw `{{var}}` placeholders.
- Priority: Medium.

### `requires_approval` / approval-gate path
- What's not tested: End-to-end "tool requires approval → approval requested → denied → tool not executed". Only the `request_approval_deny` unit test exists at `crates/openfang-kernel/src/approval.rs:391`.
- Files: `crates/openfang-runtime/src/tool_runner.rs:166-200`, `crates/openfang-kernel/src/approval.rs`.
- Risk: A regression could let a Phase 1 `skill_manage` (which should require approval for `protected` skills) bypass the gate.
- Priority: **High** — Phase 1 will lean on this for `skill_manage` approval.

### Channels: live wire-level tests
- What's not tested: 53 tests in `crates/openfang-channels/src/telegram.rs`, 28 in `discord.rs`, 62 in `feishu.rs` — all unit tests. No live integration (matches CLAUDE.md's "Unit tests alone are not enough" warning).
- Files: `crates/openfang-channels/src/{telegram,discord,feishu,bridge}.rs`.
- Risk: Less acute for Phase 1 (not touching channels) but the broader concern from CLAUDE.md applies.
- Priority: Low for Phase 1, but document.

### `server.rs` (route wiring) has no `#[cfg(test)]` block
- What's not tested: The router registration step. The CLAUDE.md gotcha "Missing route registrations in server.rs" is exactly because there's no compile-time or unit-test guard.
- Files: `crates/openfang-api/src/server.rs` (37891 bytes, no tests).
- Risk: Phase 1 adds at least 4 new routes (skill_manage CRUD, memory_reason POST, session_search GET, reasoning budget GET/PUT). Any missed registration → endpoint returns 404.
- Priority: **High** — at minimum add a smoke test that hits each route by name expectation.

### `webchat.rs`, `lib.rs` in `openfang-api`
- What's not tested: No `#[cfg(test)]` blocks.
- Files: `crates/openfang-api/src/webchat.rs`, `crates/openfang-api/src/lib.rs`.
- Priority: Medium.

### Phase 1 components (forward-looking)
- `crates/openfang-reasoning/` — does not exist, so no tests.
- `skill_manage` tool — does not exist.
- `memory_reason` tool — does not exist.
- `session_search` tool — does not exist.
- Migration v9 (FTS5 + flat messages) — does not exist.
- All of the above must come with tests in their introducing PR; otherwise the live-integration step from `CLAUDE.md` is the only safety net.

---

*Concerns audit: 2026-06-06*
