# Technology Stack

**Analysis Date:** 2026-06-06

## Languages

**Primary:**
- Rust (edition 2021, MSRV `rust-version = "1.75"`) — entire workspace, 14 crates under `crates/` plus `xtask/`. Declared in `Cargo.toml` workspace.package.

**Secondary:**
- HTML / Alpine.js / JavaScript — bundled SPA dashboard served by the API (`crates/openfang-api/src/webchat.rs`, `static/index_body.html` per `CLAUDE.md`).
- Python (3) — present in the runtime image (`Dockerfile`) and used for skill / migration helpers (`crates/openfang-runtime/src/python_runtime.rs`, `test_vertex_e2e.py`).
- Node.js / npm — present in the runtime image for MCP servers (`Dockerfile`) and packaged SDK (`sdk/`, `packages/`).
- Shell + PowerShell — installer scripts (`scripts/install.sh`, `scripts/install.ps1`, `start-vertex.bat`).
- Nix — reproducible dev/build via `flake.nix` (flake-parts + juspay/rust-flake).
- TOML / YAML / JSON5 — config and skill manifest formats (`openfang-migrate` depends on `serde_yaml`, `json5`, `toml`).

## Runtime

**Environment:**
- Tokio async runtime, version 1.x with `features = ["full"]` (declared workspace-wide in `Cargo.toml`).
- `tokio-stream` 0.1 for async stream adapters.
- Wasmtime 43 for WASM sandbox execution of skills (`openfang-runtime` -> `wasmtime`, see `crates/openfang-runtime/src/sandbox.rs`).

**Package Manager:**
- Cargo (workspace resolver = "2", declared in `Cargo.toml`).
- Lockfile: present (`Cargo.lock`, ~227 KB, committed).
- Toolchain pin: `rust-toolchain.toml` -> stable channel + `rustfmt`, `clippy` components.
- `xtask` crate for project task automation.

## Frameworks

**Core:**
- axum 0.8 with `ws` + `multipart` features — HTTP/WS API server (`crates/openfang-api`, `crates/openfang-desktop`). Mounted in `crates/openfang-api/src/server.rs`.
- tower 0.5 + tower-http 0.6 (cors, trace, compression-gzip, compression-br) — HTTP middleware stack.
- reqwest 0.12 (rustls-tls, json, stream, multipart, gzip/deflate/brotli; default-features off) — universal HTTP client used by every LLM driver in `crates/openfang-runtime/src/drivers/`.
- rustls 0.23 with ring backend (no native OpenSSL on the request path).
- rmcp 1.2 (Model Context Protocol Rust SDK) with `client`, `transport-child-process`, `transport-streamable-http-client-reqwest` features — MCP integration in `crates/openfang-runtime/src/mcp.rs` and `mcp_server.rs`.
- clap 4 + `clap_complete` — CLI binary (`crates/openfang-cli`, binary name `openfang`, path `crates/openfang-cli/src/main.rs`).
- Tauri 2 (+ plugins: notification, shell, single-instance, dialog, global-shortcut, autostart, updater) — native desktop shell (`crates/openfang-desktop`).
- ratatui 0.29 + colored 3 — interactive TUI for the CLI.
- prost 0.14 — Protobuf codec (used by channel adapters, e.g. WhatsApp `crates/openfang-channels/src/whatsapp.rs`).

**Testing:**
- Built-in `cargo test` runner (workspace-wide, ~1744+ tests per `CLAUDE.md`).
- `tokio-test` 0.4 (dev-dep across most crates) — async test helpers.
- `tempfile` 3 (dev-dep) — temp dir fixtures.

**Build/Dev:**
- Cargo profiles in `Cargo.toml`: `release` (LTO on, codegen-units = 1, strip, opt-level 3) and `release-fast` (thin LTO, codegen-units = 8, opt-level 2, no strip).
- `Dockerfile` accepts `LTO` / `CODEGEN_UNITS` build args to speed up dev builds.
- `Cross.toml` — cross-compile settings for `aarch64-unknown-linux-gnu` and `armv7-unknown-linux-gnueabihf` (ARM targets pull in `libssl-dev:<arch>`).
- GitHub Actions: `.github/workflows/ci.yml` (check/test on Ubuntu+macOS+Windows, clippy `-D warnings`, fmt --check, cargo audit, TruffleHog secrets scan, install-script smoke test) and `.github/workflows/release.yml` (Tauri desktop bundles on 5 platforms + CLI binaries on 7 targets + multi-arch GHCR image).
- Dependabot config in `.github/dependabot.yml`.

## Key Dependencies

**Critical:**
- `tokio` 1 — async runtime, every async crate depends on it.
- `axum` 0.8 + `tower-http` 0.6 — HTTP/WS API surface at `127.0.0.1:4200` (default) / `127.0.0.1:50051` (legacy default in `KernelConfig::default`, `crates/openfang-types/src/config.rs:1499`).
- `reqwest` 0.12 + `rustls` 0.23 — LLM driver HTTP transport.
- `rmcp` 1.2 — Model Context Protocol client (filesystem MCP servers etc. — see `openfang.toml.example` `[[mcp_servers]]` example).
- `wasmtime` 43 — WASM skill sandbox (`crates/openfang-runtime/src/sandbox.rs`, `workspace_sandbox.rs`).
- `rusqlite` 0.31 with `bundled` + `serde_json` features — embedded SQLite (no external libsqlite needed). Used by `crates/openfang-memory` (substrate, session, semantic, structured, knowledge, usage, consolidation) and runtime.
- `serde` 1 + `serde_json` 1 + `toml` 0.9 + `rmp-serde` 1 + `serde_yaml` 0.9 + `json5` 0.4 — serialization across config, wire, MCP, and migration paths.
- `dashmap` 6 + `crossbeam` 0.8 — concurrent collections used in registries (`crates/openfang-wire/src/registry.rs`, `crates/openfang-kernel/src/registry.rs`).
- `tracing` 0.1 + `tracing-subscriber` 0.3 (env-filter + json) — structured logging (default level `info`, overridable via `RUST_LOG`).
- `clap` 4 — CLI parsing.

**Infrastructure:**
- `tokio-tungstenite` 0.24 (connect, rustls-tls-native-roots) — WebSocket client for Discord/Slack gateway and MCP streamable transports (`crates/openfang-channels`, `openfang-runtime`).
- `lettre` 0.11 (smtp-transport, tokio1-rustls-tls) + `imap` 2 + `mailparse` 0.16 + `native-tls` 0.2 (vendored) — email IMAP/SMTP adapter (`crates/openfang-channels/src/email.rs`).
- `rumqttc` 0.25 (use-native-tls) — MQTT client (`crates/openfang-channels/src/mqtt.rs`).
- `openssl` 0.10 with `vendored` feature — statically linked, no runtime libssl dep on Linux.
- `ed25519-dalek` 2 (with `rand_core`) — signing for OFP peer auth and skill manifest signatures (`crates/openfang-wire`, `crates/openfang-types/src/manifest_signing.rs`).
- `sha2` 0.10 + `sha1` 0.10 + `hmac` 0.12 + `hex` 0.4 + `subtle` 2 — hashing & constant-time comparisons (peer auth, HMAC for Slack/Discord signatures).
- `aes` 0.8 + `cbc` 0.1 + `aes-gcm` 0.10 + `argon2` 0.5 — encrypted credential vault in `crates/openfang-extensions/src/vault.rs` (PBKDF via argon2, AEAD via aes-gcm).
- `zeroize` 1 (derive) — secret zeroization across runtime / extensions / CLI / channels / wire.
- `governor` 0.10 — rate limiting (`crates/openfang-api/src/rate_limiter.rs`).
- `chrono` 0.4 (serde) + `chrono-tz` 0.10 — timestamps and scheduler timezones.
- `uuid` 1 (v4, v5, serde) — entity IDs.
- `cron` 0.16 (kernel-only) — cron-expression parsing for `crates/openfang-kernel/src/cron.rs`.
- `walkdir` 2 — skill and migration directory traversal.
- `zip` 4 (deflate) — skill package extraction (`crates/openfang-skills`).
- `dirs` 6 — home directory resolution (`~/.openfang`).
- `bitflags` 2 — capability flags in `crates/openfang-types`.
- `regex-lite` 0.1 — lightweight regex (no full `regex` crate; kept small for ARM targets).
- `shlex` 1 — shell-style tokenization for tool args.
- `socket2` 0.5 — SO_REUSEADDR on the API listener.
- `governor`, `tokio-stream`, `bytes` 1, `futures` 0.3, `async-trait` 0.1, `thiserror` 2, `anyhow` 1 — async/error infra.
- `tauri-build` 2 (build-dep on `openfang-desktop`); `open` 5 (open URLs in default browser).

## Configuration

**Environment:**
- Configured primarily via `~/.openfang/config.toml` (override with `--config <path>`). Loader in `crates/openfang-kernel/src/config.rs` deep-merges `include = [...]` files first then root, with TOCTOU-safe canonicalization and path-traversal rejection.
- `KernelConfig` struct in `crates/openfang-types/src/config.rs:1145` declares all config fields. Default `api_listen` is `127.0.0.1:50051`; runtime/docs default is `127.0.0.1:4200` (Docker `EXPOSE 4200`, `OPENFANG_LISTEN` env var override).
- Auto-migration: misplaced `[api]` section keys (`api_key`, `api_listen`, `log_level`) are lifted to root by the loader.
- `[default_model]` selects the active LLM provider (`provider`, `model`, `api_key_env`, optional `base_url`, `subprocess_timeout_secs`).
- `[memory]` (decay rate, optional `sqlite_path`), `[network]` (OFP `listen_addr`, `shared_secret`), `[compaction]` (threshold, keep_recent, max_summary_tokens), `[telegram]/[discord]/[slack]/[matrix]/[email]` channel sections, `[[mcp_servers]]` array, `[[bindings]]` (with lenient pre-validation in `lenient_extract_bindings`), `usage_footer` mode.
- `.env`-style environment variables documented in `.env.example`: LLM keys (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`/`GOOGLE_API_KEY`, `GROQ_API_KEY`, `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `TOGETHER_API_KEY`, `MISTRAL_API_KEY`, `FIREWORKS_API_KEY`, `NOVITA_API_KEY`), local providers (`OLLAMA_BASE_URL`/`OLLAMA_HOST`, `VLLM_BASE_URL`/`VLLM_HOST`, `LMSTUDIO_BASE_URL`/`LMSTUDIO_HOST`, `LEMONADE_BASE_URL`/`LEMONADE_HOST`), channel tokens (`TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, `SLACK_BOT_TOKEN`, `SLACK_APP_TOKEN`, `WHATSAPP_TOKEN`/`WHATSAPP_PHONE_ID`, `SIGNAL_CLI_PATH`/`SIGNAL_PHONE_NUMBER`, `MATRIX_HOMESERVER`/`MATRIX_ACCESS_TOKEN`, `EMAIL_*`), OpenFang knobs (`OPENFANG_LISTEN`, `OPENFANG_API_KEY`, `OPENFANG_HOME`, `RUST_LOG`).
- Subprocess timeout override: `OPENFANG_SUBPROCESS_TIMEOUT_SECS` (read in `crates/openfang-runtime/src/drivers/mod.rs:403`).
- Vertex AI uses `GOOGLE_APPLICATION_CREDENTIALS` + `GOOGLE_CLOUD_PROJECT` / `GCLOUD_PROJECT` / `GCP_PROJECT` and `GOOGLE_CLOUD_REGION` / `VERTEX_AI_REGION` (defaults `us-central1`); see `crates/openfang-runtime/src/drivers/mod.rs:466-494`.
- AWS Bedrock uses `AWS_REGION` / `AWS_DEFAULT_REGION` + Bedrock API key in `DriverConfig.api_key`.

**Build:**
- `Cargo.toml` (workspace root) — pins versions and feature sets for every workspace dependency.
- `rust-toolchain.toml` — stable channel + components.
- `rustfmt.toml` — single setting `max_width = 100`.
- `Cross.toml` — apt setup for ARM cross compiles.
- `Dockerfile` (multi-stage `rust:1-slim-bookworm` builder + runtime with python3, nodejs, npm, ca-certificates; `EXPOSE 4200`, `VOLUME /data`, `OPENFANG_HOME=/data`, `ENTRYPOINT ["openfang"]`, `CMD ["start"]`).
- `docker-compose.yml` — single `openfang` service; passes through `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GROQ_API_KEY`, `TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, `SLACK_BOT_TOKEN`, `SLACK_APP_TOKEN`; volume `openfang-data:/data`.
- `flake.nix` — Nix flake; defines packages `openfang-cli` and `openfang-desktop` with GTK/WebKit/AppIndicator system deps for the latter.
- `.github/workflows/ci.yml` enforces `RUSTFLAGS=-D warnings`, runs `cargo check`, `cargo test`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --all --check`, `cargo audit`, TruffleHog secrets scan, and a smoke test of `openfang.sh` installers.
- `.github/workflows/release.yml` builds 5 desktop bundles (Tauri-signed/notarized on macOS, msi/exe on Windows, deb/AppImage on Linux) + 7 CLI binary targets (incl. armv7) + multi-arch `ghcr.io/rightnow-ai/openfang` Docker image.

## Platform Requirements

**Development:**
- Stable Rust toolchain (auto-installed via `rust-toolchain.toml`).
- For Tauri desktop on Linux: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf` (see CI install steps).
- For cross-compilation: `cross` CLI + Docker (used in release pipeline for `aarch64`/`armv7` Linux).
- Optional Nix: `nix develop` via `flake.nix`.
- API daemon binds (default Docker/runtime) to `127.0.0.1:4200`; legacy/struct default in `KernelConfig::default` is `127.0.0.1:50051`.

**Production:**
- Single static binary `openfang` (CLI bin in `crates/openfang-cli`, path `crates/openfang-cli/src/main.rs`). Daemon command is `start` (per `CLAUDE.md`, not `daemon`).
- Distribution channels: GitHub Releases (CLI tarballs/zips + desktop installers + auto-update `latest.json`), GHCR multi-arch Docker (`ghcr.io/rightnow-ai/openfang:<version>`), curl|sh installer at `https://openfang.sh/install` (+ PowerShell `install.ps1`).
- Targets supported by release pipeline: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `armv7-unknown-linux-gnueabihf`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`.
- Resource floor: project explicitly targets ARM Cortex-A7 with ~1.9 GB RAM (`CHAITEX.md`); regex-lite + statically vendored OpenSSL + `release-fast` profile exist to support this.
- Persistent state: `~/.openfang/` (or `OPENFANG_HOME`); inside Docker mapped to `/data`. SQLite DB default `~/.openfang/data/openfang.db`.

---

*Stack analysis: 2026-06-06*
