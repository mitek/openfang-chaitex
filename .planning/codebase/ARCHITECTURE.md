# Architecture

**Analysis Date:** 2026-06-06

OpenFang is an Agent Operating System implemented as a Rust workspace of 14 crates (13 code crates + `xtask`). The ChaiTex fork extends this with self-learning features (skill self-patching, memory reasoning, FTS5 search) per `CHAITEX.md` and `docs/chaitex/phase1-self-learning-architecture.md`.

## Pattern Overview

**Overall:** Layered crate workspace with kernel-as-coordinator + trait-based handle inversion (to avoid circular deps) + async event bus + dependency-injected adapters.

**Key Characteristics:**
- **Strict downward layering**: 14 Rust crates form a DAG. Lower crates (`openfang-types`, `openfang-memory`) depend on nothing above them. Verified via `crates/*/Cargo.toml`.
- **Kernel-as-coordinator**: `OpenFangKernel` in `crates/openfang-kernel/src/kernel.rs` owns every subsystem and is the single thing wrapped in `Arc` and shared throughout.
- **Trait inversion for upward callbacks**: `KernelHandle` trait defined in `crates/openfang-runtime/src/kernel_handle.rs` lets the agent loop call back into the kernel without `openfang-runtime` depending on `openfang-kernel`.
- **`AppState` bridge**: `crates/openfang-api/src/routes.rs` (`pub struct AppState`) wraps `Arc<OpenFangKernel>` plus HTTP-only state (CORS, rate-limit caches, bridge manager). Axum routers are built in `crates/openfang-api/src/server.rs::build_router()`.
- **Async-first**: Tokio runtime everywhere. SQLite is wrapped in `Arc<Mutex<Connection>>` and bridged via `spawn_blocking`.
- **Channel-pluggable**: `ChannelAdapter` trait in `crates/openfang-channels/src/types.rs` (line 292) plus 40 adapter implementations.
- **Capability-based security**: every tool, network call, agent spawn, and shell exec is gated by `CapabilityManager` (DashMap-based) in `crates/openfang-kernel/src/capabilities.rs`.

## Layers

### Layer 0 — Types
- **Purpose:** Shared type definitions consumed by every other crate. Pure data + traits, no business logic.
- **Location:** `crates/openfang-types/`
- **Contains:** `agent.rs` (AgentManifest, AgentId, SessionId), `capability.rs`, `config.rs` (KernelConfig + `#[serde(default)]` defaults), `error.rs` (`OpenFangError`, `OpenFangResult`), `event.rs`, `memory.rs` (Entity/Relation/GraphPattern/Memory trait), `message.rs` (ContentBlock, Role, StopReason, TokenUsage), `tool.rs`, `tool_compat.rs` (21 OpenClaw mappings), `taint.rs`, `model_catalog.rs`, `manifest_signing.rs` (Ed25519).
- **Depends on:** crates.io only (`serde`, `chrono`, `uuid`, `thiserror`, `ed25519-dalek`, `dirs`, `toml`, `bitflags`).
- **Used by:** every other code crate.

### Layer 1 — Memory + Wire (siblings)
- **`openfang-memory`** (`crates/openfang-memory/`)
  - Purpose: SQLite-backed memory substrate (schema v5).
  - Key files: `substrate.rs` (`pub struct MemorySubstrate`), `structured.rs` (KV), `semantic.rs` (vector / LIKE search), `knowledge.rs` (entity-relation graph), `session.rs` (`Session`, `SessionStore`), `usage.rs` (`UsageStore`), `consolidation.rs`, `migration.rs`, `http_client.rs` (optional `http-memory` feature gateway).
  - Depends on: `openfang-types`, `rusqlite`, optional `reqwest`.
- **`openfang-wire`** (`crates/openfang-wire/`)
  - Purpose: OpenFang Protocol (OFP) — peer-to-peer agent networking over TCP, JSON-framed, HMAC-SHA256 mutual auth.
  - Key files: `peer.rs` (`PeerNode`), `registry.rs` (`PeerRegistry`), `message.rs` (`WireMessage`, request/response variants).
  - Depends on: `openfang-types`, `hmac`, `sha2`, `subtle`, `dashmap`.

### Layer 2 — Runtime + Skills + Hands + Extensions + Channels + Migrate (all rest on types/memory)
- **`openfang-runtime`** (`crates/openfang-runtime/`)
  - Purpose: agent execution. Contains the agent loop, LLM drivers, tool execution, WASM/Docker/subprocess sandboxes, MCP client+server, A2A protocol, web search/fetch, audit, session repair, compactor, embedding driver, and the `KernelHandle` trait that lets it call back into the kernel.
  - Key files: `agent_loop.rs` (`run_agent_loop`, `run_agent_loop_streaming`, `AgentLoopResult`), `tool_runner.rs` (built-in tools dispatch + capability filter), `kernel_handle.rs` (the `KernelHandle` trait), `llm_driver.rs` (`LlmDriver` trait + `DriverConfig` + `CompletionRequest/Response` + `StreamEvent`), `drivers/` (`anthropic.rs`, `gemini.rs`, `openai.rs`, `bedrock.rs`, `claude_code.rs`, `copilot.rs`, `vertex.rs`, `qwen_code.rs`, `fallback.rs`), `mcp.rs`, `mcp_server.rs`, `a2a.rs`, `web_search.rs`, `web_fetch.rs`, `loop_guard.rs`, `session_repair.rs`, `compactor.rs`, `audit.rs` (Merkle chain), `sandbox.rs` (Wasmtime dual fuel+epoch), `docker_sandbox.rs`, `subprocess_sandbox.rs`, `model_catalog.rs`, `routing.rs` (`ModelRouter`).
  - Depends on: `openfang-types`, `openfang-memory`, `openfang-skills`.
- **`openfang-skills`** (`crates/openfang-skills/`)
  - Purpose: skill registry + bundled skills + verification + marketplace clients.
  - Key files: `registry.rs` (`pub struct SkillRegistry`), `bundled.rs` (compile-time `include_str!` of 60 SKILL.md files from `crates/openfang-skills/bundled/<name>/SKILL.md`), `loader.rs`, `verify.rs` (`SkillVerifier`, `scan_prompt_content`), `openclaw_compat.rs` (SKILL.md → skill.toml), `marketplace.rs` (FangHub), `clawhub.rs`, `installer.rs`, `config_injection.rs` (`SkillConfigVar`, `resolve_skill_config`, `render_config_block`).
  - Depends on: `openfang-types`.
- **`openfang-hands`** (`crates/openfang-hands/`): curated autonomous capability packages. `registry.rs`, `bundled.rs`.
- **`openfang-extensions`** (`crates/openfang-extensions/`): one-click MCP server install, credential vault (AES-GCM + Argon2), OAuth2 PKCE. `vault.rs`, `oauth.rs`, `credentials.rs`, `installer.rs`, `registry.rs`, `health.rs`, `bundled.rs`.
- **`openfang-channels`** (`crates/openfang-channels/`): 40+ messaging adapters implementing the `ChannelAdapter` trait. Highlights: `bridge.rs` (`BridgeManager`), `router.rs` (`AgentRouter`), `formatter.rs` (Markdown→TelegramHTML/SlackMrkdwn/PlainText), `types.rs` (`ChannelMessage`, `ChannelContent`, `ChannelUser`, `ChannelAdapter`). One file per platform.
- **`openfang-migrate`** (`crates/openfang-migrate/`): OpenClaw YAML→TOML import engine. `openclaw.rs`, `report.rs`.

### Layer 3 — Kernel
- **`openfang-kernel`** (`crates/openfang-kernel/`)
  - Purpose: assembles every Layer-2 subsystem behind one `Arc<OpenFangKernel>`, implements `KernelHandle`, runs the scheduler, supervisor, workflow engine, trigger engine, background executor, RBAC, metering, and cron.
  - Key files: `kernel.rs` (the `OpenFangKernel` struct, 9.4K lines), `registry.rs` (`AgentRegistry` — DashMap), `capabilities.rs` (`CapabilityManager`), `event_bus.rs` (`EventBus` — Tokio broadcast), `scheduler.rs` (`AgentScheduler` — quota tracking), `supervisor.rs` (health), `workflow.rs` (`Workflow`, `WorkflowEngine`, `WorkflowId`, `StepAgent`), `triggers.rs` (`TriggerEngine`, `TriggerPattern`, `TriggerId`), `background.rs` (`BackgroundExecutor` — continuous + cron loops), `auth.rs` (RBAC `AuthManager`), `metering.rs` (`MeteringEngine`), `cron.rs` + `cron_delivery.rs`, `heartbeat.rs`, `approval.rs`, `auto_reply.rs`, `pairing.rs`, `config.rs`, `config_reload.rs`, `wizard.rs` (setup wizard), `whatsapp_gateway.rs`.
  - Depends on: every Layer-1/Layer-2 crate except `openfang-migrate` and `openfang-api`.

### Layer 4 — API
- **`openfang-api`** (`crates/openfang-api/`)
  - Purpose: HTTP/WebSocket/SSE daemon API on Axum 0.8. ~172 routes registered, ~242 handler functions.
  - Key files: `server.rs` (`build_router()`, `DaemonInfo`, CORS/auth wiring), `routes.rs` (12,975 lines — every REST handler + `pub struct AppState`), `ws.rs` (WebSocket chat), `webchat.rs` (HTML SPA serving), `channel_bridge.rs` (`start_channel_bridge`), `middleware.rs` (auth, request-id, security headers, redaction), `rate_limiter.rs` (GCRA), `session_auth.rs`, `openai_compat.rs` (`/v1/chat/completions`, `/v1/models`), `stream_chunker.rs`, `stream_dedup.rs`, `types.rs` (request/response DTOs).
  - Static dashboard files: `crates/openfang-api/static/index_body.html` (Alpine.js SPA), `index_head.html`, `js/app.js`, `js/api.js`, `css/*.css`, `vendor/alpine.min.js`.
  - Depends on: kernel + all Layer-2 crates.

### Layer 5 — Front-ends (siblings)
- **`openfang-cli`** (`crates/openfang-cli/`) — Clap CLI, in-process kernel fallback, MCP server, TUI. `main.rs` (7.5K lines, command dispatch), `launcher.rs`, `mcp.rs`, `tui/` (ratatui screens: `chat_runner.rs`, `event.rs`, `screens/`, `theme.rs`, `mod.rs`), `templates.rs`, `bundled_agents.rs`, `dotenv.rs`, `progress.rs`, `table.rs`, `ui.rs`.
- **`openfang-desktop`** (`crates/openfang-desktop/`) — Tauri 2.0 desktop shell. `main.rs`, `server.rs` (background Axum), `tray.rs`, `commands.rs`, `shortcuts.rs`, `updater.rs`.
- **`xtask`** — build automation (cargo-xtask pattern).

## Data Flow

### Message Flow (channel → response)

1. **Platform event** arrives at a `ChannelAdapter::start()` stream (e.g. `crates/openfang-channels/src/telegram.rs`).
2. Adapter converts platform message → `ChannelMessage` (`crates/openfang-channels/src/types.rs`).
3. `BridgeManager` (`crates/openfang-channels/src/bridge.rs`) forwards through `AgentRouter` (`router.rs`) which resolves the target agent.
4. `channel_bridge.rs` (in `openfang-api`) calls `kernel.send_message_to_agent()` (`crates/openfang-kernel/src/kernel.rs`).
5. Kernel runs **RBAC check** (`auth.rs`), **channel policy check** (DmPolicy/GroupPolicy), **quota check** (`scheduler.rs`), looks up `AgentEntry` in `registry.rs`.
6. Dispatch by `manifest.module`:
   - `builtin:chat` → `openfang_runtime::agent_loop::run_agent_loop` (`crates/openfang-runtime/src/agent_loop.rs:293`)
   - `wasm:...` → `WasmSandbox` (`crates/openfang-runtime/src/sandbox.rs`)
   - `python:...` → `python_runtime.rs` (subprocess with `env_clear()`)
7. **Agent loop** (`agent_loop.rs`):
   a. Load/create session from `MemorySubstrate`.
   b. Inject canonical context summary into system prompt + `STABILITY_GUIDELINES`.
   c. Resolve `LlmDriver` (kernel default or per-agent override).
   d. Gather tools = capability-filtered builtins + skill tools + MCP tools (`tool_runner::builtin_tool_definitions`).
   e. Init `LoopGuard` (`loop_guard.rs`), run `session_repair::validate_and_repair`.
   f. Iterate: `driver.complete()` → if `StopReason::ToolUse` → execute via `tool_runner.rs` (with capability check + 120s `tokio::time::timeout` + truncation) → loop. Max 50 iterations.
   g. Auto-compact session if context budget exceeded (`compactor.rs`).
   h. Persist session + canonical session + audit entry (Merkle chain, `audit.rs`).
8. **Cost metering** via `MeteringEngine.estimate_cost_with_catalog()`; usage event persisted by `UsageStore`.
9. Response returned as `AgentLoopResult` (`.response`, `.usage`, `.cost_usd`).
10. Adapter `send()` formats and ships back to platform.

### HTTP Flow

1. Axum router built by `crates/openfang-api/src/server.rs::build_router()`.
2. Request hits middleware: auth (`middleware.rs::AuthState`), GCRA rate limiter (`rate_limiter.rs`), security headers, request-id, structured logging.
3. Handler in `routes.rs` extracts `State<Arc<AppState>>`, talks to `state.kernel`.
4. WebSocket chat in `ws.rs`; SSE/streaming via `stream_chunker.rs`; OpenAI-compatible endpoints in `openai_compat.rs`.

### Event Flow

- All lifecycle/trigger events flow through `EventBus` (`event_bus.rs`, Tokio broadcast).
- `TriggerEngine` (`triggers.rs`) pattern-matches events to proactive agent invocations.
- `BackgroundExecutor` runs continuous, periodic, and cron-scheduled agents.

**State Management:**
- All mutable subsystem state lives behind `dashmap::DashMap` (lock-free) or `tokio::sync::{Mutex,RwLock}` / `std::sync::RwLock` for read-mostly registries.
- `OpenFangKernel` is the **single owner** of every subsystem and is shared as `Arc<OpenFangKernel>`.
- Persisted state goes to SQLite via `MemorySubstrate`; in-memory caches use `DashMap` with TTL where relevant.

## Key Abstractions

### `KernelHandle` (trait)
- Purpose: lets `openfang-runtime` call back into the kernel for `spawn_agent`, `send_to_agent`, `task_post/claim/complete/list`, `memory_store/recall`, `publish_event`, `knowledge_*`, `cron_*`, `hand_*`, `request_approval`, `send_channel_*`, `touch_agent`, `spawn_agent_checked` — without a crate-level circular dep.
- Defined in: `crates/openfang-runtime/src/kernel_handle.rs` (line 27).
- Implemented in: `crates/openfang-kernel/src/kernel.rs` on `OpenFangKernel`.
- Pattern: dependency inversion. Most methods have default `Err("…not available")` impls so test stubs can implement a minimal subset.

### `OpenFangKernel` + `AppState`
- `OpenFangKernel` — `crates/openfang-kernel/src/kernel.rs:60`. Owns every subsystem; wrapped in `Arc` and given a `Weak<Self>` back-reference (`self_handle: OnceLock<Weak<OpenFangKernel>>`) for trigger dispatch.
- `AppState` — `crates/openfang-api/src/routes.rs:25`. Bridges `Arc<OpenFangKernel>` to HTTP. Holds `bridge_manager: Mutex<Option<BridgeManager>>` (swappable on hot-reload), `channels_config: RwLock<ChannelsConfig>`, `shutdown_notify`, caches, and `budget_config`.
- Pattern: composition root + Axum `State<Arc<AppState>>` extractor.

### `LlmDriver` (trait)
- Defined in `crates/openfang-runtime/src/llm_driver.rs:148`.
- Methods: `complete(CompletionRequest) -> Result<CompletionResponse, LlmError>` and a default `stream(...)` that wraps `complete()`.
- Implementations: `drivers/anthropic.rs`, `drivers/gemini.rs`, `drivers/openai.rs` (covers ~18 OpenAI-compatible providers via `DriverConfig.base_url`), `drivers/bedrock.rs`, `drivers/vertex.rs`, `drivers/claude_code.rs`, `drivers/copilot.rs`, `drivers/qwen_code.rs`, `drivers/fallback.rs`.
- Pattern: trait-object polymorphism, all instances stored as `Arc<dyn LlmDriver>`. API keys wrapped in `Zeroizing<String>`.

### `ChannelAdapter` (trait)
- Defined in `crates/openfang-channels/src/types.rs:292`.
- Required: `name()`, `channel_type()`, `start()` (stream of `ChannelMessage`), `send()`, `stop()`.
- Optional: `send_typing`, `send_reaction`, `send_in_thread`, `should_auto_thread`, `create_thread`, `status()`.
- 40+ implementations, one per platform file in `crates/openfang-channels/src/`.

### `MemorySubstrate`
- Defined in `crates/openfang-memory/src/substrate.rs:30`.
- Composes `StructuredStore`, `SemanticStore`, `KnowledgeStore`, `SessionStore`, `UsageStore`, `ConsolidationEngine` over a shared `Arc<Mutex<rusqlite::Connection>>`.
- Implements the `Memory` trait declared in `openfang-types::memory`.
- Async API via `spawn_blocking`; supports optional HTTP backend (`http_client.rs`, feature `http-memory`).

### `SkillRegistry`
- Defined in `crates/openfang-skills/src/registry.rs:14`.
- Holds `HashMap<String, InstalledSkill>` + freeze flag + per-skill config overrides.
- Loads bundled skills via `bundled.rs::bundled_skills()` (compile-time `include_str!`), workspace skills from disk, with prompt-injection scanning before activation.
- Held by kernel as `std::sync::RwLock<SkillRegistry>` for hot-reload.

### `EventBus`
- Defined in `crates/openfang-kernel/src/event_bus.rs:15`.
- Tokio broadcast channel wrapper. Publishes `openfang_types::event::Event` lifecycle/trigger/system events.

### `AgentRegistry`
- Defined in `crates/openfang-kernel/src/registry.rs:8`.
- DashMap of `AgentId → AgentEntry`. Concurrent agent storage.

### Tooling — `tool_runner` (function dispatch, not a Tool trait)
- `crates/openfang-runtime/src/tool_runner.rs` — there is no `Tool` trait; built-in tools are dispatched by name in a giant match. Tool definitions are returned by `builtin_tool_definitions()` and filtered against the agent's capability set.
- Skill-provided tools (PromptOnly, Python, WASM, Node) and MCP-provided tools (`mcp::*`) join the same list, namespaced.
- `tokio::task_local!` `AGENT_CALL_DEPTH` enforces `MAX_AGENT_CALL_DEPTH = 5`.

### `AgentLoopResult`
- Defined in `crates/openfang-runtime/src/agent_loop.rs:249`.
- Fields include `.response` (note: not `.response_text`, per `CLAUDE.md` gotcha), `.usage`, `.cost_usd`, iteration count.

## Entry Points

### `openfang start` daemon
- Location: `crates/openfang-cli/src/main.rs` → `cmd_start` → `crates/openfang-api/src/server.rs::run_daemon` → `OpenFangKernel::boot_with_config()`.
- Triggers: user runs `openfang start` (CLI command is `start`, not `daemon`, per `CLAUDE.md`).
- Responsibilities: boot kernel, write `~/.openfang/daemon.json`, start Axum on `127.0.0.1:4200` by default, start channel bridges, start cron broadcaster, start MCP background connections.

### CLI binary `openfang`
- Location: `crates/openfang-cli/src/main.rs` (binary `name = "openfang"`, line ~921 `fn main()`; commands enum at line ~108).
- Subcommands (per `main.rs`): `tui`, `init`, `start`, `stop`, `agent`, `workflow`, `trigger`, `migrate`, `skill`, `channel`, `hand`, plus `chat`, `status`, `doctor`, `mcp`, `config`.
- Auto-detect: reads `~/.openfang/daemon.json` and pings `/api/health`; uses HTTP if up, otherwise boots an in-process kernel.

### Desktop app
- Location: `crates/openfang-desktop/src/main.rs`. Tauri 2.0 shell spawns kernel in-process on a random port (`server.rs`), shows tray (`tray.rs`).

### Channel adapters (passive entry)
- Each adapter's `start()` returns a `Stream<ChannelMessage>`, which the `BridgeManager` (`crates/openfang-channels/src/bridge.rs`) drives into the kernel.

### Background executor
- `crates/openfang-kernel/src/background.rs::BackgroundExecutor` — drives continuous/periodic agent loops and cron-scheduled invocations.

### MCP server
- `crates/openfang-runtime/src/mcp_server.rs` exposes OpenFang's built-in tools to external clients via JSON-RPC 2.0 (stdio/SSE). Launched by `openfang mcp` CLI subcommand.

### OFP listener
- `crates/openfang-wire/src/peer.rs::PeerNode` listens for TCP peer connections after kernel boot.

## Error Handling

**Strategy:** typed errors via `thiserror`, surfaced as `OpenFangError` / `OpenFangResult` at boundaries.

**Patterns:**
- `OpenFangError` enum in `crates/openfang-types/src/error.rs` — the canonical cross-crate error type. Each crate uses it for public APIs.
- `LlmError` in `crates/openfang-runtime/src/llm_driver.rs` — driver-specific errors (`MissingApiKey`, etc.), often converted to `OpenFangError::LlmDriver`.
- `KernelError` / `KernelResult` in `crates/openfang-kernel/src/error.rs` — kernel-internal errors.
- `SkillError` in `openfang-skills`, `MigrationReport` warnings in `openfang-migrate`.
- HTTP layer: handlers in `routes.rs` return `impl IntoResponse` and map errors to `(StatusCode, Json<...>)` tuples — never panic out.
- `KernelHandle` defaults return `Err("…not available".to_string())` strings for optional capability methods.
- Tool runner: every tool wrapped in `tokio::time::timeout`; results truncated to ≤50 KB; orphan `ToolResult` messages dropped by `session_repair::validate_and_repair`.
- LLM retries: exponential backoff for 429/529 inside `crates/openfang-runtime/src/retry.rs`; circuit-broken by `auth_cooldown.rs` (`ProviderCooldown`).
- Graceful shutdown: `shutdown_notify: Arc<Notify>` on `AppState` + `graceful_shutdown.rs` in runtime.

## Cross-Cutting Concerns

**Logging:** `tracing` crate workspace-wide; `tracing-subscriber` configured in `openfang-cli` and `openfang-desktop`. Structured fields, no raw `println!`. Secrets redacted via custom `Debug` impls and `Zeroizing<String>` wrappers in `crates/openfang-runtime/src/llm_driver.rs`.

**Validation:** TOML config parsed with `#[serde(default)]` on every field of `KernelConfig` (`crates/openfang-types/src/config.rs`) for forward compatibility — per `CLAUDE.md` you MUST also add new fields to the `Default` impl or the build breaks. Manifests validated by parsing into `AgentManifest` (`crates/openfang-types/src/agent.rs`). Capability inheritance checked by `validate_capability_inheritance()` in `crates/openfang-kernel/src/capabilities.rs`.

**Authentication / Authorization:**
- **API auth**: Bearer token (`api_key` config / `OPENFANG_API_KEY`) or dashboard session (Argon2id password hash) in `crates/openfang-api/src/middleware.rs` and `session_auth.rs`. Non-loopback binds without auth refuse with 401 unless `OPENFANG_ALLOW_NO_AUTH=1`.
- **RBAC**: `AuthManager` in `crates/openfang-kernel/src/auth.rs` resolves channel identity → role → permissions.
- **Capabilities**: per-agent `CapabilityManager` (`crates/openfang-kernel/src/capabilities.rs`) enforces tool/network/spawn/shell gates. Capabilities filtered into tool list before LLM sees them.
- **OFP**: HMAC-SHA256 mutual handshake in `crates/openfang-wire/src/peer.rs` with constant-time verify via `subtle`.

**Rate limiting:** GCRA limiter in `crates/openfang-api/src/rate_limiter.rs` (cost-aware, per-IP, stale entry cleanup). Per-channel rate limiter in `openfang-channels` using `DashMap` per-user tracking.

**Security headers:** CSP, X-Frame-Options, X-Content-Type-Options, X-XSS-Protection, Referrer-Policy, Permissions-Policy — injected by `middleware.rs`.

**Audit:** Merkle hash chain in `crates/openfang-runtime/src/audit.rs`. Each entry's hash includes the previous entry's hash → tamper-evident log.

**Taint tracking:** `crates/openfang-types/src/taint.rs` (`TaintLabel`, `TaintSet`, `TaintedValue`, `TaintSink`). Applied at tool boundaries (`tool_runner.rs::check_taint_shell_exec`, `check_taint_net_fetch`).

**Sandboxing:** WASM via Wasmtime with dual fuel + epoch metering (`sandbox.rs`). Docker via `docker_sandbox.rs`. Subprocess via `subprocess_sandbox.rs` with `env_clear()` then selective env injection. Workspace isolation in `workspace_sandbox.rs`.

**SSRF:** `web_fetch.rs::is_ssrf_target` blocks private IPs + cloud metadata (`169.254.169.254`) with DNS rebinding protection. Applied in both `host_net_fetch` and `web_fetch`.

**Loop stability:** `LoopGuard` (`loop_guard.rs`), `session_repair`, tool result truncation, `MAX_ITERATIONS = 50`, `MAX_CONTINUATIONS = 5`, `MAX_AGENT_CALL_DEPTH = 5`, universal 120 s tool timeout (env-overridable).

**Hot reload:** `crates/openfang-kernel/src/config_reload.rs`. `RwLock` fields on `OpenFangKernel` (`skill_registry`, `skill_config_overrides`, `default_model_override`, `fallback_providers_override`, `effective_mcp_servers`, `extension_registry`) allow live updates without restart.

**Embedded dashboard:** Alpine.js SPA in `crates/openfang-api/static/index_body.html` + `static/index_head.html` + `static/js/app.js`. Per `CLAUDE.md`: new dashboard tabs need both HTML + JS data/methods.

---

*Architecture analysis: 2026-06-06*
