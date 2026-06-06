# Testing Patterns

**Analysis Date:** 2026-06-06

OpenFang has 1744+ tests across the workspace (per `CLAUDE.md`). Tests are colocated unit tests in `#[cfg(test)] mod tests` blocks plus integration tests in per-crate `tests/` directories. There is also a documented **live integration testing** flow against a running daemon that every contributor must follow after wiring changes.

## Test Framework

**Runner:**
- Built-in `cargo test` (no external runner). Rust 2021 / MSRV 1.75 (`rust-toolchain.toml`).
- No `nextest` config detected.

**Async runner:**
- `#[tokio::test]` for async tests (default multi-thread runtime via `tokio = { features = ["full"] }` in workspace `Cargo.toml`).

**Assertion macros:**
- Std: `assert!`, `assert_eq!`, `assert_ne!`, `assert!(matches!(...))`, `panic!`.
- No `pretty_assertions`, no `proptest`, no `quickcheck` in workspace deps.

**Dev-dependencies (in `[workspace.dependencies]` of `Cargo.toml:155-157`):**
- `tokio-test = "0.4"` — pulled into `openfang-api`, `openfang-channels`, `openfang-extensions`, `openfang-hands`, `openfang-kernel`, `openfang-memory`, `openfang-runtime`, `openfang-skills`, `openfang-wire` (see each `Cargo.toml` `[dev-dependencies]`).
- `tempfile = "3"` — used everywhere a kernel boot needs an isolated `home_dir` / `data_dir`.

**Run Commands:**
```bash
cargo test --workspace                                    # Run all tests (CI command, .github/workflows/ci.yml:64)
cargo test -p openfang-kernel                             # Run one crate
cargo test -p openfang-api --test api_integration_test    # Run a single integration test file
cargo test -p openfang-kernel --test integration_test -- --nocapture  # Show println! output
cargo test test_cooldown_                                 # Filter by test name substring
GROQ_API_KEY=gsk_... cargo test -p openfang-kernel        # LLM-gated tests opt in via env var
```

**CI gates (`.github/workflows/ci.yml`):**
- `cargo check --workspace` on Ubuntu / macOS / Windows.
- `cargo test --workspace` on all three OSes (Tauri-display-dependent tests are skipped via `cfg` in headless CI).
- `cargo clippy --workspace -- -D warnings`.
- `cargo fmt --all -- --check`.
- `cargo audit`.
- `trufflehog` secrets scan.

## Test File Organization

**Two locations:**

1. **Co-located unit tests** — `#[cfg(test)] mod tests { ... }` at the bottom of the source file. Used by ~80% of source files. Example: `crates/openfang-runtime/src/web_cache.rs:78`, `crates/openfang-runtime/src/auth_cooldown.rs:471`, `crates/openfang-types/src/lib.rs:38`.

2. **Integration tests** — `crates/<crate>/tests/<feature>_test.rs`. Each file compiles as its own binary and only sees the crate's public API. Found via `find crates -path '*/tests/*.rs'`:

   ```
   crates/openfang-api/tests/api_integration_test.rs
   crates/openfang-api/tests/daemon_lifecycle_test.rs
   crates/openfang-api/tests/load_test.rs
   crates/openfang-api/tests/skill_config_api_test.rs
   crates/openfang-channels/tests/bridge_integration_test.rs
   crates/openfang-kernel/tests/integration_test.rs
   crates/openfang-kernel/tests/multi_agent_test.rs
   crates/openfang-kernel/tests/wasm_agent_integration_test.rs
   crates/openfang-kernel/tests/workflow_integration_test.rs
   crates/openfang-migrate/tests/provider_legacy_yaml.rs
   crates/openfang-migrate/tests/provider_json5_provider_catalog.rs
   crates/openfang-migrate/tests/provider_json5_default_model.rs
   crates/openfang-migrate/tests/provider_json5_agents.rs
   ```

**Naming:**
- Unit-test fns: `test_<feature>_<scenario>` (e.g. `test_put_and_get`, `test_cooldown_duration_escalates`, `test_billing_max_cap`). Some files use just `<scenario>` (e.g. `truncate_str_ascii`, `multibyte_emoji` in `crates/openfang-types/src/lib.rs`). The `test_` prefix is the dominant convention.
- Integration files: `<feature>_test.rs` or `<feature>_integration_test.rs`. Migration tests omit the suffix entirely (`provider_legacy_yaml.rs`).

**Counts:**
- ~2375 `#[test]` functions (sync).
- ~323 `#[tokio::test]` functions (async).

## Test Structure

**Standard co-located pattern:**
```rust
// crates/openfang-runtime/src/web_cache.rs
pub struct WebCache { /* ... */ }

impl WebCache { /* ... */ }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let cache = WebCache::new(Duration::from_secs(60));
        cache.put("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get("key1"), Some("value1".to_string()));
    }

    #[test]
    fn test_expired_entry() {
        let cache = WebCache::new(Duration::from_millis(1));
        cache.put("key1".to_string(), "value1".to_string());
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get("key1"), None);
    }
}
```

**Section banners** are common in larger test modules:
```rust
// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------
```
(`crates/openfang-api/tests/api_integration_test.rs:21`, `crates/openfang-runtime/src/auth_cooldown.rs:467`)

**Test helper functions** sit at the top of the `mod tests` block before any `#[test]` fn:
```rust
fn fast_config() -> CooldownConfig {
    CooldownConfig {
        base_cooldown_secs: 1,
        max_cooldown_secs: 10,
        // ...
        probe_interval_secs: 0, // instant probes for testing
    }
}
```
(`crates/openfang-runtime/src/auth_cooldown.rs:475`)

**Setup / teardown:**
- No `before_each` / fixtures framework — each test constructs its own state inline.
- Teardown handled by `Drop` impls. Notable: `TestServer` in `crates/openfang-api/tests/api_integration_test.rs:30-35` impls `Drop` to call `kernel.shutdown()` and clean the tempdir.
- Cleanup via `tempfile::TempDir` — when the handle drops, the directory is removed. The `_tmp: tempfile::TempDir` field on `TestServer` keeps the dir alive for the lifetime of the test.

**Assertion patterns:**
- `assert_eq!` is the default. `assert!(x.is_some())` / `assert!(result.is_ok())` for option/result.
- `assert!(matches!(event, StreamEvent::TextDelta { text } if text == "hello"))` for enum-variant checks (see `crates/openfang-runtime/src/llm_driver.rs:262`).
- Error tests: `assert!(result.is_err()); assert!(result.unwrap_err().contains("hash mismatch at seq 1"));` (`crates/openfang-runtime/src/audit.rs:356-357`).
- Failure messages explain expectations: `assert!(billing > general, "billing cooldown should be longer");` (`crates/openfang-runtime/src/auth_cooldown.rs:548`).

## Mocking

**Framework:** none. **All mocks are hand-rolled structs implementing the relevant trait.**

**Pattern — trait-based fake driver:**
```rust
// crates/openfang-runtime/src/agent_loop.rs:3688
struct EmptyAfterToolUseDriver {
    call_count: AtomicU32,
}

#[async_trait]
impl LlmDriver for EmptyAfterToolUseDriver {
    async fn complete(&self, _req: CompletionRequest)
        -> Result<CompletionResponse, LlmError>
    {
        let call = self.call_count.fetch_add(1, Ordering::Relaxed);
        if call == 0 {
            // First call returns ToolUse
            Ok(CompletionResponse { /* tool block */ })
        } else {
            // Second call returns empty EndTurn — reproduces the bug
            Ok(CompletionResponse { content: vec![], /* ... */ })
        }
    }
}
```

Many such mocks live in `crates/openfang-runtime/src/agent_loop.rs::tests` (e.g. `EmptyMaxTokensDriver`, `NormalDriver`, `EmptyAfterToolUseDriver`). State across calls uses `AtomicU32` to stay `Send + Sync`.

**Pattern — handle traits across crates:**
```rust
// crates/openfang-channels/src/bridge.rs:2197
struct MockHandle { /* ... */ }
impl ChannelBridgeHandle for MockHandle {
    fn spawn_agent(&self, _: AgentManifest) -> Result<AgentId, String> {
        Err("spawn not implemented in mock".to_string())
    }
    // ...
}
```

`KernelHandle` (in `crates/openfang-runtime/src/kernel_handle.rs`) and `ChannelBridgeHandle` (in `openfang-channels`) exist specifically so test fakes can replace the real kernel without pulling in the heavy dependency graph (see CLAUDE.md "Architecture Notes").

**What to mock:**
- LLM drivers (`LlmDriver`) — never call real APIs in unit tests; gate any test that needs a real call behind `GROQ_API_KEY`.
- Kernel handles for cross-crate testing.
- External I/O (HTTP, file-system roots): redirect to `tempfile::TempDir`.

**What NOT to mock:**
- `MemorySubstrate` — use the real implementation with `MemorySubstrate::open_in_memory(decay_rate)` (`crates/openfang-memory/src/substrate.rs:107`). The in-memory SQLite connection is fast enough for unit tests.
- Concurrency primitives (`DashMap`, `Arc`, `tokio::sync::*`) — use the real ones.
- `axum` routers in API integration tests — boot a real `Router` and bind on `127.0.0.1:0` so the OS picks a free port (`crates/openfang-api/tests/api_integration_test.rs:152`).

## Fixtures and Factories

**Inline TOML manifests** are the dominant agent-fixture pattern:
```rust
// crates/openfang-api/tests/api_integration_test.rs:169
const TEST_MANIFEST: &str = r#"
name = "test-agent"
version = "0.1.0"
description = "Integration test agent"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test-model"
system_prompt = "You are a test agent. Reply concisely."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#;

let manifest: AgentManifest = toml::from_str(TEST_MANIFEST).unwrap();
```

Two flavours co-exist in each test file: one uses `ollama` (no network) for boot-only tests, the other uses `groq` for tests gated behind `GROQ_API_KEY`. See `TEST_MANIFEST` vs `LLM_MANIFEST` in `crates/openfang-api/tests/api_integration_test.rs:169-204`.

**Config factories** are small helper fns in the test module:
```rust
// crates/openfang-kernel/tests/integration_test.rs:9
fn test_config() -> KernelConfig {
    let tmp = std::env::temp_dir().join("openfang-integration-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    KernelConfig {
        home_dir: tmp.clone(),
        data_dir: tmp.join("data"),
        default_model: DefaultModelConfig { /* ... */ },
        ..KernelConfig::default()
    }
}
```

Spawned across many integration files; `crates/openfang-kernel/tests/workflow_integration_test.rs:17` uses the cleaner `tempfile::tempdir()` form which auto-cleans.

**Location:**
- No `tests/fixtures/` directories in the repo. Everything is inline.
- Migration tests embed legacy YAML/JSON5 snippets as `const &str` and parse them — see `crates/openfang-migrate/tests/provider_json5_default_model.rs` (et al.).

**Builder shortcut:**
```rust
KernelConfig {
    home_dir: tmp.path().to_path_buf(),
    default_model: /* ... */,
    ..KernelConfig::default()
}
```
The `..Default::default()` struct-update syntax is the idiomatic "test builder" — no third-party `Default` macros.

## Coverage

**Requirements:** None enforced. No `cargo-tarpaulin` / `grcov` job in CI. The 1744+ test count (CLAUDE.md) is the de-facto signal.

**View Coverage:**
- Not configured. Manually: `cargo install cargo-llvm-cov && cargo llvm-cov --workspace`.

## Test Types

**Unit Tests (in-source `#[cfg(test)]`):**
- Scope: a single function or struct. Fast (<1s), no I/O beyond `tempfile`.
- Always live in the same file as the code under test.
- Examples: `crates/openfang-runtime/src/web_cache.rs:78` (cache TTL), `crates/openfang-runtime/src/auth_cooldown.rs:471` (circuit breaker math), `crates/openfang-runtime/src/str_utils.rs:21` (UTF-8 truncation safety), `crates/openfang-types/src/lib.rs:38` (em-dash truncation regression for issue #104).

**Integration Tests (`tests/<feature>_test.rs`):**
- Scope: cross-module flows, full HTTP server, multi-agent workflows.
- Boot a real `OpenFangKernel`, optionally start a real `axum::Router`, hit endpoints with `reqwest`.
- The API integration test (`crates/openfang-api/tests/api_integration_test.rs:1-10`) explicitly states: *"These tests boot a real kernel, start a real axum HTTP server on a random port, and hit actual endpoints with reqwest. No mocking."*
- LLM-dependent tests gate themselves:
  ```rust
  if std::env::var("GROQ_API_KEY").is_err() {
      eprintln!("GROQ_API_KEY not set, skipping integration test");
      return;
  }
  ```
  (`crates/openfang-kernel/tests/integration_test.rs:30`)

**E2E / Live Integration Tests:**
- A unique **live curl-based testing workflow** is documented in `CLAUDE.md` and is **mandatory after any new endpoint, feature, or wiring change.** Unit tests alone are insufficient — they can pass while the feature is dead code because of unregistered routes, undeserialized config fields, etc.
- Workflow (verbatim from `CLAUDE.md`):
  1. Stop any running daemon: `tasklist | grep -i openfang; taskkill //PID <pid> //F; sleep 3`.
  2. Build fresh: `cargo build --release -p openfang-cli`.
  3. Start daemon: `GROQ_API_KEY=<key> target/release/openfang.exe start &; sleep 6; curl -s http://127.0.0.1:4200/api/health`.
  4. Test every new endpoint with `curl` (GET, POST/PUT). Read back after writing to confirm persistence.
  5. Trigger a real LLM call: `curl -s -X POST "http://127.0.0.1:4200/api/agents/<id>/message" -H "Content-Type: application/json" -d '{"message": "Say hello in 5 words."}'`.
  6. Verify side effects (budget, metering) via `/api/budget` and `/api/budget/agents`.
  7. Verify dashboard HTML at `http://127.0.0.1:4200/` contains the new UI component.
  8. Kill the daemon.
- The daemon command is **`start`** (not `daemon`).
- Key endpoints documented in `CLAUDE.md`: `/api/health`, `/api/agents`, `/api/agents/{id}/message`, `/api/budget`, `/api/network/status`, `/api/peers`, `/api/a2a/agents`, etc.

**Load Tests:**
- `crates/openfang-api/tests/load_test.rs` — performance-oriented integration tests.

**Migration Tests:**
- `crates/openfang-migrate/tests/provider_*.rs` — round-trip parse legacy YAML / JSON5 provider configs.

**Workspace check + clippy** double as smoke tests; CI runs them on every PR on three OSes.

## Common Patterns

**Async testing:**
```rust
#[tokio::test]
async fn test_health_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}
```
(`crates/openfang-api/tests/api_integration_test.rs:210`)

**Error testing:**
```rust
let result = log.verify_integrity();
assert!(result.is_err());
assert!(result.unwrap_err().contains("hash mismatch at seq 1"));
```
(`crates/openfang-runtime/src/audit.rs:356-357`)

For `Result<T, KernelError>`:
```rust
let agent_id = kernel.spawn_agent(manifest).expect("Agent should spawn");
```
(`crates/openfang-kernel/tests/integration_test.rs:61`)
`.expect` with a descriptive panic message is preferred to bare `.unwrap()` in tests.

**Time-based tests:**
- Real `std::thread::sleep(Duration::from_millis(10))` is used (see `web_cache.rs:99`) rather than mocking time. Cooldown tests instead lower the config values (`probe_interval_secs: 0`) to keep tests instant.

**Environment-gated tests:**
- Pattern: early-return with `eprintln!` so the test still passes on machines without the credential. Used for `GROQ_API_KEY` across kernel integration tests.

**Random ports for HTTP:**
- Bind to `127.0.0.1:0` and read the assigned port from the listener:
  ```rust
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  ```
  (`crates/openfang-api/tests/api_integration_test.rs:152-155`)
- Avoids port collisions when tests run in parallel.

**Drop-based cleanup:**
- `Drop for TestServer` calls `kernel.shutdown()` so the supervisor's background tasks stop and the temp dir can be removed.

---

*Testing analysis: 2026-06-06*
