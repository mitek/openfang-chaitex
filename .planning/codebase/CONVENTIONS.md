# Coding Conventions

**Analysis Date:** 2026-06-06

OpenFang is a Rust workspace of 14 crates (Rust 2021 edition, MSRV 1.75). Conventions are enforced by `rustfmt`, `clippy -D warnings`, and `cargo-audit` in CI (see `.github/workflows/ci.yml`).

## Naming Patterns

**Crates:**
- All workspace members are prefixed `openfang-` (e.g. `openfang-types`, `openfang-runtime`, `openfang-kernel`). See `Cargo.toml` `members = [...]`.
- Crate names use kebab-case on disk (`crates/openfang-runtime/`) and underscores when imported in code (`use openfang_runtime::...`).
- Pattern: one crate per architectural layer (`types`, `memory`, `runtime`, `wire`, `api`, `kernel`, `cli`, `channels`, `migrate`, `skills`, `desktop`, `hands`, `extensions`, plus `xtask`).

**Files / Modules:**
- Snake_case `.rs` files, one feature per file. Examples: `auth_cooldown.rs`, `web_cache.rs`, `agent_loop.rs`, `prompt_builder.rs`, `subprocess_sandbox.rs` (all in `crates/openfang-runtime/src/`).
- Module declared in `lib.rs` via `pub mod <name>;` followed by selective re-exports (`pub use kernel::OpenFangKernel;` — see `crates/openfang-kernel/src/lib.rs:29`).
- Integration tests live in `crates/<crate>/tests/<feature>_test.rs` — note the trailing `_test.rs` suffix (e.g. `api_integration_test.rs`, `workflow_integration_test.rs`).

**Functions:**
- Snake_case verbs. Constructors are `new` / `new_with_X` / `open_in_memory` (`crates/openfang-memory/src/substrate.rs:107`).
- Async fns suffixed by behaviour, not by `_async`. Streaming variants use `_streaming` (`run_agent_loop` vs `run_agent_loop_streaming` in `crates/openfang-runtime/src/agent_loop.rs`).
- Predicates: `is_*`, `has_*`. Mutators on `&self` returning new state (e.g. `WebCache::put`, `WebCache::evict_expired`) are plain verbs.
- Test fn names start with `test_` (e.g. `test_cooldown_config_defaults`) — not enforced but consistent across `auth_cooldown.rs`, `web_cache.rs`, `kernel.rs`.

**Types:**
- UpperCamelCase. Suffixes carry semantics:
  - `Config` for plain data settings: `KernelConfig`, `CooldownConfig`, `DriverConfig`, `BudgetConfig`.
  - `Manager` for stateful coordinators: `AuthManager`, `CapabilityManager`.
  - `Engine` for subsystems: `WorkflowEngine`, `TriggerEngine`, `MeteringEngine`.
  - `Driver` for pluggable trait impls: `LlmDriver`, `StubDriver`, `EmbeddingDriver`.
  - `Handle` for cross-crate trait facades: `KernelHandle`, `ChannelBridgeHandle`.
  - `*Error` for thiserror enums: `KernelError`, `OpenFangError`, `LlmError`.
  - `*Result<T>` aliases sit next to the error: `KernelResult<T>` (`crates/openfang-kernel/src/error.rs:19`), `OpenFangResult<T>` (`crates/openfang-types/src/error.rs:104`).
- Newtype IDs wrap `Uuid`: `AgentId(Uuid)`, `SessionId(Uuid)`, `WorkflowId`, `TriggerId`, `WorkflowRunId` — see `crates/openfang-types/src/agent.rs` and `crates/openfang-kernel/src/triggers.rs`.

**Constants:**
- SCREAMING_SNAKE_CASE: `MAX_ITERATIONS`, `MAX_MANIFEST_SIZE` (`crates/openfang-api/src/routes.rs:98`).

## Code Style

**Formatting:**
- Tool: `rustfmt` (stable channel, pinned via `rust-toolchain.toml`).
- Only setting: `max_width = 100` in `rustfmt.toml`. Everything else uses defaults (4-space indent, trailing commas, block-style imports).
- CI gate: `cargo fmt --all -- --check` (`.github/workflows/ci.yml:96`). Repo issue #1121 ensures every crate is fmt-clean.

**Linting:**
- `cargo clippy --workspace -- -D warnings` runs in CI and is also baseline per `CLAUDE.md`. Zero warnings expected.
- `RUSTFLAGS: "-D warnings"` set in CI env (`.github/workflows/ci.yml:11`) — any warning fails the build.
- `cargo audit` runs on every PR.

**Edition / MSRV:**
- `edition = "2021"`, `rust-version = "1.75"` declared workspace-wide in `Cargo.toml` lines 22-25. Crates inherit via `edition.workspace = true`.

## Import Organization

Observed convention across `crates/openfang-runtime/src/agent_loop.rs:6-31`, `crates/openfang-kernel/src/kernel.rs:3-41`, `crates/openfang-api/src/routes.rs:1-19`:

**Order (rustfmt default + manual grouping):**
1. `use crate::...` — same-crate modules (often listed first when there are many).
2. `use openfang_<other>::...` — sibling workspace crates, alphabetised.
3. `use <external>::...` — third-party crates (`async_trait`, `serde`, `tokio`, `tracing`, `axum`, `std::...`).
4. `use std::...` — std last in many files, but `kernel.rs` mixes std with externals — rustfmt does not enforce groups, so each author groups visually with blank lines.

**No path aliases** (Rust does not have JS-style `@/foo`). Workspace crates referenced by their published name (`openfang-types = { path = "../openfang-types" }` in every crate's `Cargo.toml`).

**Re-exports:**
- `lib.rs` re-exports the small public surface: `pub use kernel::OpenFangKernel;` (`crates/openfang-kernel/src/lib.rs:30`).
- Modules are declared `pub mod` and consumers reach in via the full path.

## Error Handling

**Strategy:** typed errors with `thiserror` per crate; results never use `Box<dyn Error>`.

**Patterns:**
- Each crate defines its own `Error` enum + `Result` alias. Sub-crate errors compose with `#[from]`:
  ```rust
  // crates/openfang-kernel/src/error.rs
  #[derive(Error, Debug)]
  pub enum KernelError {
      #[error(transparent)]
      OpenFang(#[from] OpenFangError),
      #[error("Boot failed: {0}")]
      BootFailed(String),
  }
  pub type KernelResult<T> = Result<T, KernelError>;
  ```
- The shared `OpenFangError` (`crates/openfang-types/src/error.rs`) is a flat enum of cross-cutting cases (`AgentNotFound`, `CapabilityDenied`, `QuotaExceeded`, `LlmDriver`, `Sandbox`, `Network`, etc.). New cross-crate failures get added here, NOT to per-crate enums.
- IO errors auto-convert via `#[from] std::io::Error`.
- `String`-based variants for stringly-typed messages are accepted (`Memory(String)`, `Internal(String)`) — used when wrapping a foreign error type via `.map_err(|e| OpenFangError::Memory(e.to_string()))` (see `crates/openfang-memory/src/session.rs:44`).
- `anyhow` is listed in `Cargo.toml` workspace deps but is reserved for binary boundaries. Libraries return concrete `Result<T, *Error>`.
- `.unwrap()` and `.expect()` are acceptable in tests and in `main()`-style boot code (`crates/openfang-kernel/tests/integration_test.rs:37` uses `.expect("Kernel should boot")`). In library paths, prefer `.map_err(...)?` or `.ok_or(OpenFangError::*)?`.
- Construction of REST errors (`crates/openfang-api/src/routes.rs:62-92`): return tuples `(StatusCode, Json<serde_json::Value>)` with `{"error": "..."}` payloads.

## Logging

**Framework:** `tracing` + `tracing-subscriber` (workspace deps; `tracing-subscriber` has `env-filter` and `json` features).

**Initialization:**
- Daemon and CLI init in `crates/openfang-cli/src/main.rs:859-904` — two paths: `init_tracing_stderr()` for normal runs, `init_tracing_file()` redirects to `~/.openfang/tui.log` when the ratatui TUI is active (so logs don't corrupt the terminal).
- Filter resolved from `RUST_LOG` env, falling back to config-file `log_level` (default `info`).
- Desktop binary mirrors the pattern (`crates/openfang-desktop/src/lib.rs:34`).

**Patterns:**
- Per-module imports: `use tracing::{debug, info, warn};` at the top of every kernel module (see `crates/openfang-kernel/src/auto_reply.rs:7`, `background.rs:15`, `cron.rs:18`, etc.).
- Structured fields preferred over interpolation:
  ```rust
  warn!(
      provider,
      error_count = state.error_count,
      cooldown_secs = cooldown.as_secs(),
      "auth profile rotated: marking profile as failed"
  );
  ```
  (`crates/openfang-runtime/src/auth_cooldown.rs:457`)
- Log levels: `error!` on irrecoverable failure, `warn!` on degraded state / rotation, `info!` on lifecycle events, `debug!` for hot paths.
- Never log API keys: `DriverConfig`'s custom `Debug` impl prints `api_key: <redacted>` (`crates/openfang-runtime/src/llm_driver.rs:220-230`). `/api/health` returns minimal info; detailed fields are gated to `/api/status` (`crates/openfang-api/tests/api_integration_test.rs:228`).
- `eprintln!` only used in `main.rs` for CLI-facing errors that must appear on stderr even before tracing init.

## Comments

**Module-level docs:**
- Every file starts with `//!` describing its purpose, often spanning multiple lines:
  ```rust
  //! In-memory TTL cache for web search and fetch results.
  //!
  //! Thread-safe via `DashMap`. Lazy eviction on `get()` — expired entries
  //! are only cleaned up when accessed.
  ```
  (`crates/openfang-runtime/src/web_cache.rs:1-5`)

**Item docs:**
- All `pub` items have `///` doc comments. Every field of public structs is documented (see `crates/openfang-runtime/src/audit.rs:42-57` for an exemplary public struct).
- Field docs go on the line immediately above the field, no blank line.

**Inline comments:**
- Used to explain *why*, not *what*. Examples:
  - Security justifications: `// SECURITY: Reject oversized manifests to prevent parser memory exhaustion.` (`crates/openfang-api/src/routes.rs:97`)
  - Issue references: `// Issue #1098: when a response carries Thinking blocks...` (`crates/openfang-runtime/src/agent_loop.rs:3242`)
  - Concurrency notes: `// release read lock before removing` (`crates/openfang-runtime/src/web_cache.rs:39`)
- Em-dashes (`—`) appear frequently in docs and log messages; the project has explicit UTF-8 boundary helpers (`safe_truncate_str` in `crates/openfang-runtime/src/str_utils.rs`) to avoid panics on em-dash truncation.

**No `unsafe`, no `unimplemented!` in committed code.** Stubs return helpful errors instead — see `StubDriver` in `crates/openfang-kernel/src/kernel.rs:46-58` which returns a `LlmError::MissingApiKey` with actionable text.

## Function Design

**Size:**
- No hard cap, but most functions fit on a screen. Long functions (e.g. `crates/openfang-runtime/src/agent_loop.rs::run_agent_loop`) are reserved for orchestration code.
- Prefer many small `pub(crate)` helpers over one mega-function.

**Parameters:**
- `&self` for read methods, `&mut self` only when ownership truly mutates (rare — most state is behind `Arc<RwLock<_>>` or `DashMap` for interior mutability).
- Borrowed strings (`&str`) preferred over `String` for input. Owned `String` for stored fields.
- Config structs passed by value (`pub fn new(ttl: Duration) -> Self`), then cloned into the struct.
- For builder-like args use struct literals: see `KernelConfig { home_dir, data_dir, ..KernelConfig::default() }` pattern in `crates/openfang-kernel/tests/integration_test.rs:14`.

**Return Values:**
- Always `Result<T, *Error>` for fallible ops; never panic on user input.
- `Option<T>` for "missing" semantics (`WebCache::get -> Option<String>`).
- Constructors that can fail: `OpenFangResult<Self>` (see `MemorySubstrate::open_in_memory`).

## Module Design

**Exports:**
- `lib.rs` lists modules with `pub mod` declarations only; cross-crate API is the union of `pub` items in those modules.
- Selective re-exports at crate root (`pub use kernel::OpenFangKernel;`) keep call sites short.
- `pub(crate)` is used for internal helpers (`crates/openfang-api/src/lib.rs:8` `pub(crate) fn percent_decode`).
- No barrel `mod.rs` files — each module is a single `.rs` file at `crates/<crate>/src/<module>.rs`.

**Trait objects across crates:**
- `Arc<dyn Trait>` for pluggable behaviour (`Arc<dyn LlmDriver>` in `KernelHandle`).
- Circular-dependency-breaking traits live in the lower crate: `KernelHandle` is defined in `openfang-runtime` and implemented by `openfang-kernel::OpenFangKernel` (see CLAUDE.md "Architecture Notes").

**Async:**
- `tokio` (full features) is the only runtime. `#[async_trait]` for object-safe async traits (`crates/openfang-runtime/src/llm_driver.rs`).
- `tokio::sync::{Mutex, RwLock, Notify, mpsc}` for async-aware sync; `std::sync::{Mutex, RwLock}` only for short-lived non-await critical sections.

**Concurrency primitives:**
- `Arc<T>` ubiquitously for shared ownership (kernel held as `Arc<OpenFangKernel>` in `AppState`).
- `DashMap` for concurrent maps (`crates/openfang-runtime/src/web_cache.rs:18`, `crates/openfang-api/src/routes.rs:8`).
- `Weak<T>` for parent-child handles to avoid cycles (`crates/openfang-kernel/src/kernel.rs:40`).

**Config struct conventions:**
- Add field to `KernelConfig` struct AND its `Default` impl (build fails otherwise — CLAUDE.md Common Gotchas).
- All config structs derive `Serialize, Deserialize` and use `#[serde(default)]` on new fields for forward-compat TOML.

---

*Convention analysis: 2026-06-06*
