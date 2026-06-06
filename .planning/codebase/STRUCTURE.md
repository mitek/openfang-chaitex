# Codebase Structure

**Analysis Date:** 2026-06-06

## Directory Layout

```
openfang-chaitex/
├── Cargo.toml                # Workspace manifest (14 members + xtask)
├── Cargo.lock                # Locked dependency tree
├── rust-toolchain.toml       # Pinned Rust toolchain
├── rustfmt.toml              # Formatter config
├── Cross.toml                # Cross-compile config
├── Dockerfile                # Container image
├── docker-compose.yml        # Multi-service compose
├── flake.nix                 # Nix dev shell
├── openfang.toml.example     # Example user config (copied to ~/.openfang/config.toml)
├── .env.example              # Env var template (API keys etc.)
├── .cargo/                   # Cargo config (target dir, build flags)
├── .github/                  # CI workflows
├── README.md                 # Upstream README
├── CHAITEX.md                # ChaiTex fork rationale + roadmap (Phase 1 = self-learning)
├── CLAUDE.md                 # MANDATORY agent instructions (build/test/verify)
├── AGENTS.md                 # Brief notes
├── CHANGELOG.md
├── MIGRATION.md
├── CONTRIBUTING.md
├── LICENSE-APACHE / LICENSE-MIT
├── SECURITY.md
├── crates/                   # All 13 code crates + nothing else
│   ├── openfang-types/       # Layer 0 — shared types, no business logic
│   ├── openfang-memory/      # Layer 1 — SQLite memory substrate
│   ├── openfang-wire/        # Layer 1 — OFP P2P protocol (sibling of memory)
│   ├── openfang-runtime/     # Layer 2 — agent loop, LLM drivers, tools, sandbox
│   ├── openfang-skills/      # Layer 2 — skill registry + 60 bundled skills
│   ├── openfang-hands/       # Layer 2 — Hands (autonomous capability packs)
│   ├── openfang-extensions/  # Layer 2 — MCP install, vault, OAuth2
│   ├── openfang-channels/    # Layer 2 — 40+ messaging adapters
│   ├── openfang-migrate/     # Layer 2 — OpenClaw import engine
│   ├── openfang-kernel/      # Layer 3 — assembles every subsystem
│   ├── openfang-api/         # Layer 4 — Axum HTTP/WS/SSE daemon + dashboard
│   ├── openfang-cli/         # Layer 5 — Clap CLI + TUI (binary: `openfang`)
│   └── openfang-desktop/     # Layer 5 — Tauri 2.0 desktop shell
├── xtask/                    # Build automation (cargo-xtask pattern)
├── agents/                   # 31 example/template agent manifests (one dir each, each holds agent.toml)
├── docs/                     # Project docs (architecture, providers, security, …)
│   └── chaitex/              # ChaiTex Phase 1 architecture (self-learning design)
├── scripts/                  # Utility scripts
├── deploy/                   # Deployment scaffolding
├── sdk/                      # External SDK files
├── packages/                 # Auxiliary packages
├── public/                   # Public static assets
├── test_vertex_e2e.py        # End-to-end Vertex test
├── start-vertex.bat          # Windows helper
└── .planning/                # GSD planning workspace (this directory)
```

## Directory Purposes

### `crates/openfang-types/src/`
- Purpose: every shared data structure and trait used cross-crate. No business logic; safe to depend on from anywhere.
- Contains: one file per concept.
- Key files: `agent.rs` (AgentManifest, AgentId, SessionId, DEFAULT_MAX_HISTORY_MESSAGES), `capability.rs`, `config.rs` (KernelConfig + every sub-config with `#[serde(default)]`), `error.rs` (OpenFangError), `event.rs`, `memory.rs` (Memory trait, Entity, Relation, GraphPattern, GraphMatch), `message.rs` (ContentBlock, Role, StopReason, TokenUsage), `tool.rs` (ToolDefinition, ToolCall, ToolResult), `tool_compat.rs` (normalize_tool_name, 21 OpenClaw mappings), `taint.rs` (TaintLabel/TaintSet/TaintedValue/TaintSink), `manifest_signing.rs` (Ed25519), `model_catalog.rs` (ModelCatalogEntry, ProviderInfo, ModelTier, AuthStatus), `media.rs`, `webhook.rs`, `approval.rs`, `commands.rs`, `comms.rs`, `scheduler.rs`, `serde_compat.rs`, `lib.rs` (`truncate_str` helper).

### `crates/openfang-memory/src/`
- Purpose: SQLite memory substrate that backs all per-agent and shared memory.
- Key files: `substrate.rs` (MemorySubstrate), `structured.rs` (StructuredStore — KV), `semantic.rs` (SemanticStore — vector / LIKE), `knowledge.rs` (KnowledgeStore — entity/relation graph), `session.rs` (Session, SessionStore), `usage.rs` (UsageStore), `consolidation.rs` (ConsolidationEngine + decay), `migration.rs` (schema v1→v5), `http_client.rs` (optional remote backend, feature `http-memory`), `lib.rs`.

### `crates/openfang-runtime/src/`
- Purpose: agent execution engine — everything that runs *inside* an agent.
- Key files: `agent_loop.rs` (`run_agent_loop`, `run_agent_loop_streaming`, `AgentLoopResult` — 5.5K lines), `tool_runner.rs` (5K lines, dispatch + capability filter), `kernel_handle.rs` (the trait), `llm_driver.rs` (LlmDriver trait, DriverConfig, CompletionRequest/Response, StreamEvent, LlmError), `model_catalog.rs`, `routing.rs` (ModelRouter), `prompt_builder.rs`, `compactor.rs`, `session_repair.rs`, `loop_guard.rs`, `context_budget.rs`, `context_overflow.rs`, `audit.rs` (Merkle chain), `embedding.rs`, `sandbox.rs` (Wasmtime), `docker_sandbox.rs`, `subprocess_sandbox.rs`, `workspace_sandbox.rs`, `workspace_context.rs`, `agent_context.rs`, `command_lane.rs`, `process_manager.rs`, `python_runtime.rs`, `apply_patch.rs`, `hooks.rs`, `retry.rs`, `auth_cooldown.rs`, `llm_errors.rs`, `graceful_shutdown.rs`, `host_functions.rs` (WASM host imports), `image_gen.rs`, `link_understanding.rs`, `media_understanding.rs`, `tts.rs`, `browser.rs`, `mcp.rs` (client), `mcp_server.rs`, `a2a.rs`, `provider_health.rs`, `tool_policy.rs`, `reply_directives.rs`, `shell_bleed.rs`, `think_filter.rs`, `web_cache.rs`, `web_content.rs`, `web_fetch.rs`, `web_search.rs`, `copilot_oauth.rs`, `str_utils.rs`.
- Sub-module: `drivers/` — `mod.rs`, `anthropic.rs`, `gemini.rs`, `openai.rs`, `bedrock.rs`, `vertex.rs`, `claude_code.rs`, `copilot.rs`, `qwen_code.rs`, `fallback.rs`.

### `crates/openfang-kernel/src/`
- Purpose: the single coordinator that owns every subsystem.
- Key files: `kernel.rs` (`OpenFangKernel`, 9.4K lines — assembles everything), `lib.rs`, `registry.rs` (AgentRegistry — DashMap), `capabilities.rs` (CapabilityManager + validate_capability_inheritance), `event_bus.rs` (EventBus — Tokio broadcast), `scheduler.rs` (AgentScheduler — quota tracking with hourly window), `supervisor.rs`, `workflow.rs` (Workflow, WorkflowEngine, WorkflowId, WorkflowRunId, WorkflowStep, StepAgent, StepMode, ErrorMode), `triggers.rs` (TriggerEngine, TriggerId, TriggerPattern), `background.rs` (BackgroundExecutor — continuous/periodic/cron loops), `auth.rs` (AuthManager + UserRole), `metering.rs` (MeteringEngine), `cron.rs` + `cron_delivery.rs` (CronScheduler), `heartbeat.rs` (HeartbeatMonitor), `approval.rs` (ApprovalManager), `auto_reply.rs` (AutoReplyEngine), `pairing.rs` (PairingManager), `config.rs` (`load_config`), `config_reload.rs` (hot-reload), `wizard.rs` (SetupWizard), `whatsapp_gateway.rs`, `error.rs` (KernelError/KernelResult).

### `crates/openfang-api/src/`
- Purpose: HTTP/WebSocket/SSE daemon, dashboard, OpenAI-compat, A2A endpoints.
- Key files: `server.rs` (`build_router()`, `DaemonInfo`, CORS + auth wiring, ~172 route registrations), `routes.rs` (12.9K lines — every REST handler + `pub struct AppState`), `lib.rs`, `ws.rs` (WebSocket chat + cron broadcaster), `webchat.rs` (HTML SPA serving + favicon + sw.js), `channel_bridge.rs` (`start_channel_bridge`), `middleware.rs` (AuthState, security headers, request-id, redaction), `rate_limiter.rs` (GCRA), `session_auth.rs` (dashboard sessions, Argon2id), `openai_compat.rs` (`/v1/chat/completions`, `/v1/models`), `stream_chunker.rs`, `stream_dedup.rs`, `types.rs` (DTOs).
- Sub-dir: `static/` — embedded SPA files:
  - `index_head.html`, `index_body.html` (Alpine.js dashboard SPA)
  - `js/app.js`, `js/api.js`, `js/katex.js`, `js/pages/`
  - `css/theme.css`, `css/layout.css`, `css/components.css`
  - `i18n/en.json`, `i18n/ru.json`, `i18n/i18n.js`
  - `vendor/alpine.min.js`, `vendor/marked.min.js`, `vendor/highlight.min.js`, `vendor/chart.umd.min.js`, `vendor/github-dark.min.css`
  - `favicon.ico`, `logo.png`, `manifest.json`, `sw.js` (PWA)

### `crates/openfang-channels/src/`
- Purpose: pluggable messaging adapters.
- Pattern: one file per platform, each implementing the `ChannelAdapter` trait from `types.rs`.
- Key files: `types.rs` (ChannelAdapter trait at line 292, ChannelMessage, ChannelContent, ChannelUser, ChannelType, ChannelStatus, LifecycleReaction), `bridge.rs` (BridgeManager), `router.rs` (AgentRouter), `formatter.rs` (Markdown → platform formats).
- Adapter files: `telegram.rs`, `discord.rs`, `slack.rs`, `whatsapp.rs`, `signal.rs`, `matrix.rs`, `email.rs`, `teams.rs`, `mattermost.rs`, `irc.rs`, `google_chat.rs`, `twitch.rs`, `rocketchat.rs`, `zulip.rs`, `xmpp.rs`, `line.rs`, `viber.rs`, `messenger.rs`, `reddit.rs`, `mastodon.rs`, `bluesky.rs`, `feishu.rs`, `revolt.rs`, `nextcloud.rs`, `guilded.rs`, `keybase.rs`, `threema.rs`, `nostr.rs`, `webex.rs`, `pumble.rs`, `flock.rs`, `twist.rs`, `mumble.rs`, `dingtalk.rs`, `dingtalk_stream.rs`, `discourse.rs`, `gitter.rs`, `ntfy.rs`, `gotify.rs`, `linkedin.rs`, `webhook.rs`, `wecom.rs`, `mqtt.rs`.

### `crates/openfang-skills/`
- Purpose: skill system (60 bundled, plus installed/workspace).
- `src/` key files: `registry.rs` (SkillRegistry), `bundled.rs` (compile-time `include_str!` of bundled SKILL.md files), `loader.rs`, `verify.rs` (SkillVerifier + `scan_prompt_content`), `installer.rs`, `marketplace.rs` (FangHubClient), `clawhub.rs` (ClawHubClient), `openclaw_compat.rs` (SKILL.md ↔ skill.toml), `config_injection.rs` (`SkillConfigVar`, `resolve_skill_config`, `render_config_block`), `lib.rs` (SkillManifest, InstalledSkill, SkillError, SkillToolDef).
- `bundled/<name>/SKILL.md` — 60 skill packages (one dir per skill, each with `SKILL.md`).

### `crates/openfang-hands/src/`
- Purpose: Hands — curated autonomous capability packages.
- Key files: `registry.rs` (HandRegistry), `bundled.rs`, `lib.rs`.

### `crates/openfang-extensions/src/`
- Purpose: one-click MCP-server install + credential vault + OAuth2 PKCE.
- Key files: `vault.rs` (AES-GCM + Argon2 vault), `credentials.rs` (CredentialResolver — vault → dotenv → env priority), `oauth.rs` (OAuth2 PKCE), `installer.rs`, `registry.rs` (IntegrationRegistry), `health.rs` (HealthMonitor), `bundled.rs` (bundled integration templates), `lib.rs`.

### `crates/openfang-wire/src/`
- Purpose: OpenFang Protocol (OFP) for peer-to-peer agent comms.
- Key files: `peer.rs` (PeerNode — TCP listener), `registry.rs` (PeerRegistry, PeerEntry, RemoteAgent), `message.rs` (WireMessage + request/response types), `lib.rs`.

### `crates/openfang-migrate/src/`
- Purpose: import other agent frameworks.
- Key files: `openclaw.rs` (OpenClaw YAML → TOML), `report.rs` (MigrationReport), `lib.rs`.

### `crates/openfang-cli/src/`
- Purpose: Clap-based CLI + ratatui TUI + MCP server mode.
- Key files: `main.rs` (7.5K lines — Commands enum at line 108, dispatch at line ~972), `launcher.rs` (daemon auto-detect), `mcp.rs` (MCP server CLI mode), `templates.rs`, `bundled_agents.rs`, `dotenv.rs`, `progress.rs`, `table.rs`, `ui.rs`.
- Sub-dir: `tui/` — `mod.rs`, `chat_runner.rs`, `event.rs`, `theme.rs`, `screens/`.

### `crates/openfang-desktop/src/`
- Purpose: Tauri 2.0 desktop wrapper.
- Key files: `main.rs`, `lib.rs`, `server.rs` (in-process axum on background thread), `commands.rs` (Tauri IPC: `get_port`, `get_status`), `tray.rs`, `shortcuts.rs`, `updater.rs`.

### `agents/`
- Purpose: example agent manifests shipped with the repo. Each is a directory containing one `agent.toml`.
- Examples: `coder/agent.toml`, `orchestrator/agent.toml`, `researcher/agent.toml`, `code-reviewer/agent.toml`, `customer-support/agent.toml`, `email-assistant/agent.toml`, `meeting-assistant/agent.toml`, etc. (31 templates total).

### `docs/`
- Purpose: human-readable project documentation.
- Key files: `architecture.md` (deep architecture description — source of truth), `api-reference.md`, `cli-reference.md`, `configuration.md`, `providers.md`, `security.md`, `skill-development.md`, `channel-adapters.md`, `mcp-a2a.md`, `agent-templates.md`, `workflows.md`, `getting-started.md`, `troubleshooting.md`, `production-checklist.md`, `launch-roadmap.md`, `desktop.md`, `VERTEX_AI_LOCAL_TESTING.md`.
- Sub-dir: `chaitex/` — fork-specific docs:
  - `phase1-self-learning-architecture.md` (design for skill self-patching, memory reasoning, FTS5)
  - `openfang-vs-hermes-analysis.md`
  - `hermes-on-pc162-rust-port.md`
- Sub-dir: `benchmarks/`.

### `xtask/`
- Purpose: cargo-xtask build automation crate.

## Key File Locations

**Entry Points:**
- `crates/openfang-cli/src/main.rs` — `openfang` binary, all CLI subcommands. `fn main()` at line ~921.
- `crates/openfang-api/src/server.rs` — `build_router()` (router assembly), `run_daemon()` (daemon entry).
- `crates/openfang-desktop/src/main.rs` — desktop entry.
- `crates/openfang-runtime/src/mcp_server.rs` — MCP server endpoint logic.

**Configuration:**
- `~/.openfang/config.toml` — runtime config (per `CLAUDE.md`).
- `openfang.toml.example` — template config.
- `.env.example` — env var template.
- `Cargo.toml` (root) — workspace + `[workspace.dependencies]` (all version pins live here).
- `crates/openfang-types/src/config.rs` — `KernelConfig` struct + `Default` impl (you MUST add new fields to both per `CLAUDE.md`).
- `crates/openfang-kernel/src/config.rs` — `load_config()` from disk.
- `crates/openfang-kernel/src/config_reload.rs` — hot-reload pipeline.
- `~/.openfang/data/openfang.db` — SQLite memory store (schema v5).
- `~/.openfang/daemon.json` — daemon PID/port/version (written by `run_daemon`).
- `rust-toolchain.toml` — pinned toolchain.
- `rustfmt.toml` — formatter config.
- `.cargo/config.toml` — cargo build settings.

**Core Logic:**
- `crates/openfang-kernel/src/kernel.rs` — `OpenFangKernel` struct (line 60). Single composition root.
- `crates/openfang-runtime/src/agent_loop.rs` — `run_agent_loop` (line 293), `run_agent_loop_streaming` (line 1520), `AgentLoopResult` (line 249).
- `crates/openfang-runtime/src/tool_runner.rs` — built-in tool dispatch.
- `crates/openfang-runtime/src/kernel_handle.rs` — `KernelHandle` trait (line 27) — kernel↔runtime decoupling.
- `crates/openfang-api/src/routes.rs` — `pub struct AppState` (line 25) + every HTTP handler.
- `crates/openfang-api/src/server.rs` — router assembly; every new route MUST be registered here AND implemented in `routes.rs` (per `CLAUDE.md`).
- `crates/openfang-memory/src/substrate.rs` — `MemorySubstrate` (line 30) composition.
- `crates/openfang-skills/src/registry.rs` — `SkillRegistry` (line 14).
- `crates/openfang-channels/src/types.rs` — `ChannelAdapter` trait (line 292) + core message types.
- `crates/openfang-channels/src/bridge.rs` — `BridgeManager` lifecycle.

**Testing:**
- Co-located unit tests inside each source file under `#[cfg(test)] mod tests`.
- Integration tests: `crates/<crate>/tests/*.rs` (per crate, where present).
- Live integration testing via the API daemon — procedure mandated by `CLAUDE.md` (curl-based, requires real LLM keys).
- E2E: `test_vertex_e2e.py` at workspace root.

## Naming Conventions

**Files:**
- Snake_case module files: `agent_loop.rs`, `tool_runner.rs`, `kernel_handle.rs`, `channel_bridge.rs`.
- One concept per file when feasible (e.g. each channel adapter in its own file).
- Tests co-located inside the same file under `#[cfg(test)] mod tests`.
- Compile-time bundled assets in `bundled/` next to `src/` (e.g. `crates/openfang-skills/bundled/<skill>/SKILL.md`).
- Static web assets in `crates/openfang-api/static/`.

**Directories:**
- Crate dirs prefixed `openfang-` under `crates/` (e.g. `openfang-runtime/`).
- Sub-modules use plain folder names (e.g. `crates/openfang-runtime/src/drivers/`).

**Types and traits:**
- Types: `UpperCamelCase` (`OpenFangKernel`, `MemorySubstrate`, `AgentRegistry`, `ChannelAdapter`).
- Traits often end in `-er` / `-able` / role name: `LlmDriver`, `ChannelAdapter`, `KernelHandle`, `EmbeddingDriver`, `Memory`.
- Errors: `XxxError` (`OpenFangError`, `LlmError`, `KernelError`, `SkillError`).
- IDs: `AgentId`, `SessionId`, `PeerId`, `WorkflowId`, `TriggerId` — newtype wrappers.

**Functions:**
- snake_case throughout (`run_agent_loop`, `validate_capability_inheritance`, `safe_resolve_path`).
- Async functions return `Result<T, ErrType>`; the workspace error is `OpenFangError` and the alias is `OpenFangResult<T>`.
- Boot/start verbs: `boot_with_config`, `start_background_agents`, `start_channel_bridge`, `start_ws_cron_broadcaster`.

**Tools:**
- Built-in tool names are snake_case strings (`file_read`, `web_fetch`, `agent_send`, `memory_store`, `task_post`, `shell_exec`).
- MCP tools are namespaced as `mcp_{server}_{tool}`.
- Capabilities mirror tool names: `Capability::ToolInvoke("file_read")`.

## Where to Add New Code

### New API route (most common)
- HTTP handler: implement in `crates/openfang-api/src/routes.rs` (one async fn per endpoint, takes `State<Arc<AppState>>`).
- Route registration: add a `.route(...)` line in `crates/openfang-api/src/server.rs::build_router()`. **Per `CLAUDE.md` you MUST do both** — compile-only smoke tests will silently pass on a missing route.
- DTOs go in `crates/openfang-api/src/types.rs`.
- If exposing a kernel-level operation, add a method on `OpenFangKernel` (`crates/openfang-kernel/src/kernel.rs`) and call from the handler.
- Live-test the endpoint with `curl` per the `CLAUDE.md` MANDATORY workflow.

### New LLM provider
- Add `crates/openfang-runtime/src/drivers/<name>.rs` implementing `LlmDriver`.
- Register in `crates/openfang-runtime/src/drivers/mod.rs` and in the driver-resolution match inside `openfang-kernel/src/kernel.rs::resolve_driver` (and the catalog in `model_catalog.rs`).
- Add provider entry to `crates/openfang-runtime/src/model_catalog.rs` (so auth detection + pricing apply).

### New built-in tool
- Add a dispatch arm to `crates/openfang-runtime/src/tool_runner.rs` (there is no `Tool` trait — it's a string-dispatched match).
- Register definition in `builtin_tool_definitions()` so the LLM sees it.
- If it needs kernel access, add the operation to `KernelHandle` in `crates/openfang-runtime/src/kernel_handle.rs` (provide a default `Err(...)` impl), then implement it on `OpenFangKernel`.
- Add a `Capability::ToolInvoke("name")` reference; the capability filter in `tool_runner.rs` will pick it up automatically.
- Live-test by sending a message that triggers the tool via `/api/agents/{id}/message`.

### New channel adapter
- Add `crates/openfang-channels/src/<platform>.rs` implementing the `ChannelAdapter` trait from `crates/openfang-channels/src/types.rs`.
- Add `pub mod <platform>;` in `crates/openfang-channels/src/lib.rs`.
- Register in `crates/openfang-channels/src/bridge.rs` adapter list and in `crates/openfang-api/src/channel_bridge.rs::start_channel_bridge`.
- Add config struct field with `#[serde(default)]` to `crates/openfang-types/src/config.rs` (and to its `Default` impl).
- Add CLI plumbing in `crates/openfang-cli/src/main.rs` if interactive setup is desired.

### New skill (bundled)
- Create `crates/openfang-skills/bundled/<name>/SKILL.md` (YAML frontmatter + Markdown body, per OpenClaw format).
- Register in `crates/openfang-skills/src/bundled.rs::bundled_skills()` with `include_str!("../bundled/<name>/SKILL.md")`.
- Prompt-injection scan runs automatically at registration via `scan_prompt_content`.

### Phase-1 (ChaiTex) self-learning hooks
Per `docs/chaitex/phase1-self-learning-architecture.md`:
- **Skill self-patching**: extend `crates/openfang-skills/src/registry.rs` (`SkillRegistry`) with `create_skill`, `patch_skill`, `edit_skill`, `write_skill_file`, `reload_skill`, `set_skill_enabled`. Add `skill_manage` built-in to `crates/openfang-runtime/src/tool_runner.rs`. Emit `SkillUpdated` events through `crates/openfang-kernel/src/event_bus.rs`.
- **Memory reasoning**: extend `crates/openfang-memory/src/substrate.rs` and `crates/openfang-memory/src/consolidation.rs`; expose reasoning APIs on `KernelHandle` (`crates/openfang-runtime/src/kernel_handle.rs`).
- **FTS5 session search**: add a SQLite FTS5 virtual table in `crates/openfang-memory/src/migration.rs` (new schema version), backed by `crates/openfang-memory/src/session.rs`. Expose `session_search` as a tool in `tool_runner.rs` and via REST in `crates/openfang-api/src/routes.rs`.

### New kernel subsystem
- Add file under `crates/openfang-kernel/src/`.
- Register field on `OpenFangKernel` (`kernel.rs:60`) and initialize in `boot_with_config()`.
- Expose as `pub` if used from `openfang-api`.

### New memory store / schema change
- Add or modify file in `crates/openfang-memory/src/` (e.g. `session.rs`, `usage.rs`, new module).
- Bump schema version + add migration step in `crates/openfang-memory/src/migration.rs`.
- Wire into `MemorySubstrate` in `crates/openfang-memory/src/substrate.rs::open`.
- Add `Memory` trait methods in `crates/openfang-types/src/memory.rs` if cross-crate API.

### New configuration field
- Add field to the relevant struct in `crates/openfang-types/src/config.rs` with `#[serde(default)]`.
- Add matching line to its `Default` impl (or the build breaks — per `CLAUDE.md` gotcha).
- Read it where consumed (most often `crates/openfang-kernel/src/kernel.rs::boot_with_config`).
- If hot-reloadable, add `RwLock` override field to `OpenFangKernel` and handle in `config_reload.rs`.

### Dashboard UI change
- HTML: `crates/openfang-api/static/index_body.html` + `index_head.html` (Alpine.js).
- JS: `crates/openfang-api/static/js/app.js` (Alpine data + methods) and `static/js/api.js` (REST wrappers).
- CSS: `crates/openfang-api/static/css/{theme,layout,components}.css`.
- i18n: `static/i18n/en.json`, `static/i18n/ru.json`.
- Per `CLAUDE.md`: new tabs need both HTML markup AND Alpine.js data/methods.

### Utilities / helpers
- Cross-crate helpers: `crates/openfang-types/src/lib.rs` (e.g. `truncate_str`).
- Runtime-internal: `crates/openfang-runtime/src/str_utils.rs`.
- Tests: `tempfile` + `tokio-test` are workspace dev-deps; co-locate unit tests with the code.

## Special Directories

**`crates/openfang-skills/bundled/`:**
- Purpose: source-of-truth for 60 bundled `SKILL.md` files.
- Generated: No.
- Committed: Yes.
- Compiled into the binary via `include_str!` in `crates/openfang-skills/src/bundled.rs`.

**`crates/openfang-api/static/`:**
- Purpose: dashboard SPA assets (HTML, JS, CSS, i18n, vendor libs, PWA manifest).
- Generated: No.
- Committed: Yes.
- Served by `crates/openfang-api/src/webchat.rs`.

**`agents/`:**
- Purpose: bundled example/template agents (each `<name>/agent.toml`).
- Generated: No.
- Committed: Yes.
- Referenced by `crates/openfang-cli/src/bundled_agents.rs` and `routes.rs::spawn_agent` (template path resolution).

**`docs/chaitex/`:**
- Purpose: ChaiTex fork's design docs — Phase 1 self-learning is canonical input for `.planning/` work here.
- Generated: No.
- Committed: Yes.

**`.planning/`:**
- Purpose: GSD planning workspace (this directory).
- `.planning/codebase/` holds the structured analyses (this file + `ARCHITECTURE.md`).
- Generated: Yes (by GSD agents).
- Committed: depends on project policy.

**`target/`:**
- Purpose: cargo build artifacts.
- Generated: Yes.
- Committed: No (`.gitignore`).
- Binary location: `target/release/openfang(.exe)` (or `target/debug/openfang`).

**`~/.openfang/`** (user data, not in repo):
- `config.toml`, `data/openfang.db` (SQLite), `daemon.json`, `agents/<name>/agent.toml` (user templates), installed skills.

---

*Structure analysis: 2026-06-06*
