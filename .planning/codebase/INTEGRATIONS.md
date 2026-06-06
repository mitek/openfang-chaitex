# External Integrations

**Analysis Date:** 2026-06-06

## APIs & External Services

### LLM Providers
All driver wiring lives in `crates/openfang-runtime/src/drivers/`. Provider dispatch + env-var resolution is in `crates/openfang-runtime/src/drivers/mod.rs` (`create_driver`, `provider_defaults`, `detect_available_provider`, `known_providers` lists 38 providers). Base URLs and model catalog live in `crates/openfang-types/src/model_catalog.rs`.

**Anthropic-family (native Anthropic Messages API):**
- Anthropic Claude (`anthropic`) — SDK: custom HTTP via `reqwest`; driver `crates/openfang-runtime/src/drivers/anthropic.rs`. Auth env: `ANTHROPIC_API_KEY`. Base URL: `ANTHROPIC_BASE_URL`.
- Kimi for Code (`kimi_coding`) — reuses `AnthropicDriver` against Moonshot's Anthropic-compatible endpoint. Env: `KIMI_API_KEY`.

**OpenAI-compatible (one unified driver):**
- OpenAI (`openai`) — env `OPENAI_API_KEY`.
- Groq (`groq`) — env `GROQ_API_KEY`.
- OpenRouter (`openrouter`) — env `OPENROUTER_API_KEY`.
- Requesty (`requesty`) — env `REQUESTY_API_KEY`.
- DeepSeek (`deepseek`) — env `DEEPSEEK_API_KEY`.
- Together AI (`together`) — env `TOGETHER_API_KEY`.
- Mistral AI (`mistral`) — env `MISTRAL_API_KEY`.
- Fireworks AI (`fireworks`) — env `FIREWORKS_API_KEY`.
- Perplexity (`perplexity`) — env `PERPLEXITY_API_KEY`.
- Cohere (`cohere`) — env `COHERE_API_KEY`.
- AI21 (`ai21`) — env `AI21_API_KEY`.
- Cerebras (`cerebras`) — env `CEREBRAS_API_KEY`.
- SambaNova (`sambanova`) — env `SAMBANOVA_API_KEY`.
- HuggingFace Inference (`huggingface`) — env `HF_API_KEY`.
- xAI Grok (`xai`) — env `XAI_API_KEY`.
- Replicate (`replicate`) — env `REPLICATE_API_TOKEN`.
- Moonshot / Kimi (`moonshot` / `kimi` / `kimi2`) — env `MOONSHOT_API_KEY`.
- Qwen / DashScope / ModelStudio (`qwen` / `dashscope` / `model_studio`) — env `DASHSCOPE_API_KEY`.
- MiniMax (`minimax`) — env `MINIMAX_API_KEY`.
- Zhipu / GLM / Z.ai (`zhipu`, `glm`, `zai`, `z.ai`, `zhipu_coding`, `zai_coding`, `codegeex`) — env `ZHIPU_API_KEY`.
- Qianfan / Baidu (`qianfan`, `baidu`) — env `QIANFAN_API_KEY`.
- Volcengine / Doubao (`volcengine`, `doubao`, `volcengine_coding`) — env `VOLCENGINE_API_KEY`.
- Chutes.ai (`chutes`) — env `CHUTES_API_KEY`.
- Venice (`venice`) — env `VENICE_API_KEY`.
- NVIDIA NIM (`nvidia`, `nvidia-nim`) — env `NVIDIA_API_KEY`.
- Novita AI (`novita`, `novita-ai`) — env `NOVITA_API_KEY`.

**Google ecosystem (custom drivers):**
- Google Gemini (`gemini`, `google`) — custom format. Driver `crates/openfang-runtime/src/drivers/gemini.rs`. Env: `GEMINI_API_KEY` (or `GOOGLE_API_KEY` alias).
- Google Vertex AI (`vertex-ai`, `vertex`, `google-vertex`) — service-account OAuth. Driver `crates/openfang-runtime/src/drivers/vertex.rs`. Env: `GOOGLE_APPLICATION_CREDENTIALS` (service account JSON), `GOOGLE_CLOUD_PROJECT` / `GCLOUD_PROJECT` / `GCP_PROJECT` (auto-discovered from JSON if unset), `GOOGLE_CLOUD_REGION` / `VERTEX_AI_REGION` (default `us-central1`). Smoke-tested by `test_vertex_e2e.py` + `start-vertex.bat`.

**Cloud OAuth / signed:**
- Azure OpenAI (`azure`, `azure-openai`) — deployment-scoped URL + `api-key` header. Env: `AZURE_OPENAI_API_KEY` + mandatory `base_url` of form `https://{resource}.openai.azure.com/openai/deployments`.
- AWS Bedrock (`bedrock`) — Bedrock Converse API with Bearer API key. Driver `crates/openfang-runtime/src/drivers/bedrock.rs`. Env: `AWS_REGION` / `AWS_DEFAULT_REGION` + Bedrock API key passed via config `api_key`.
- GitHub Copilot (`github-copilot`, `copilot`) — OAuth device flow, tokens persisted under `~/.openfang/`. Driver `crates/openfang-runtime/src/drivers/copilot.rs` + `crates/openfang-runtime/src/copilot_oauth.rs`. Bootstrap: `openfang config set-key github-copilot`. Env: `COPILOT_CLIENT_ID` (optional override).

**Subprocess-based:**
- Claude Code CLI (`claude-code`) — subprocess-driven. Driver `crates/openfang-runtime/src/drivers/claude_code.rs`. Timeout via `subprocess_timeout_secs` or `OPENFANG_SUBPROCESS_TIMEOUT_SECS`.
- OpenAI Codex CLI (`codex`, `openai-codex`) — reuses OpenAI driver; reads `OPENAI_API_KEY` or pulls token via `read_codex_credential`.
- Qwen Code CLI (`qwen-code`) — driver `crates/openfang-runtime/src/drivers/qwen_code.rs`; uses Qwen OAuth (free tier).

**Local / self-hosted (OpenAI-compatible, no key required):**
- Ollama (`ollama`) — env override `OLLAMA_BASE_URL` then `OLLAMA_HOST`; default `http://localhost:11434/v1`.
- vLLM (`vllm`) — `VLLM_BASE_URL` / `VLLM_HOST`; default `http://localhost:8000/v1`.
- LM Studio (`lmstudio`) — `LMSTUDIO_BASE_URL` / `LMSTUDIO_HOST`; default `http://localhost:1234/v1`.
- Lemonade (`lemonade`) — `LEMONADE_BASE_URL` / `LEMONADE_HOST`.
- URL normalisation logic (auto `http://` prefix, auto `/v1` suffix) in `local_provider_url_from_env` at `crates/openfang-runtime/src/drivers/mod.rs:52-94`.

**Provider fallback:**
- `crates/openfang-runtime/src/drivers/fallback.rs` + `crates/openfang-runtime/src/provider_health.rs` implement multi-provider failover (`[[fallback_providers]]` in config).

### Messaging / Channel Adapters
All under `crates/openfang-channels/src/` (one file per adapter, registered via `crates/openfang-channels/src/router.rs`):
- Telegram (`telegram.rs`) — env `TELEGRAM_BOT_TOKEN`, optional `allowed_users` list.
- Discord (`discord.rs`) — env `DISCORD_BOT_TOKEN`, optional `guild_ids` filter; uses `tokio-tungstenite` gateway.
- Slack (`slack.rs`) — env `SLACK_BOT_TOKEN` + `SLACK_APP_TOKEN`; HMAC signature via `hmac`+`sha256`.
- Microsoft Teams (`teams.rs`).
- WhatsApp Cloud API (`whatsapp.rs`) — env `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_ID`; webhook port default 8443 (`crates/openfang-types/src/config.rs:2069`).
- Signal CLI (`signal.rs`) — env `SIGNAL_CLI_PATH`, `SIGNAL_PHONE_NUMBER`.
- Matrix (`matrix.rs`) — env `MATRIX_HOMESERVER`, `MATRIX_ACCESS_TOKEN`; optional room ID filter.
- Email IMAP+SMTP (`email.rs`) — env `EMAIL_IMAP_HOST`, `EMAIL_SMTP_HOST`, `EMAIL_USERNAME`, `EMAIL_PASSWORD`; uses `lettre` + `imap` + `mailparse`.
- MQTT broker (`mqtt.rs`) — via `rumqttc`.
- Webhook ingress/egress (`webhook.rs`).
- Feishu / Lark (`feishu.rs`), DingTalk + DingTalk Stream (`dingtalk.rs`, `dingtalk_stream.rs`), WeCom (`wecom.rs`).
- IRC (`irc.rs`), XMPP (`xmpp.rs`), Matrix bridge (`bridge.rs`).
- Discourse forum (`discourse.rs`), Reddit (`reddit.rs`).
- Bluesky (`bluesky.rs`), Mastodon (`mastodon.rs`), Nostr (`nostr.rs`), LinkedIn (`linkedin.rs`), Twitch (`twitch.rs`), Threema (`threema.rs`), Viber (`viber.rs`), LINE (`line.rs`), Messenger (`messenger.rs`).
- Rocket.Chat (`rocketchat.rs`), Mattermost (`mattermost.rs`), Zulip (`zulip.rs`), Twist (`twist.rs`), Pumble (`pumble.rs`), Gitter (`gitter.rs`), Flock (`flock.rs`), Webex (`webex.rs`), Google Chat (`google_chat.rs`), Keybase (`keybase.rs`), Revolt (`revolt.rs`), Guilded (`guilded.rs`), Mumble (`mumble.rs`), Nextcloud Talk (`nextcloud.rs`), Gotify (`gotify.rs`), Ntfy (`ntfy.rs`).
- Per-adapter formatter helper in `crates/openfang-channels/src/formatter.rs` (markdown vs plain text rendering rules — see `crates/openfang-types/src/comms.rs:74`).

### Agent-to-Agent Protocols
- **A2A protocol (Google standard)** — `crates/openfang-runtime/src/a2a.rs`. Agent card served at `/.well-known/agent.json`. `A2aClient` discovers and dispatches tasks to external A2A agents. API endpoints: `/api/a2a/agents`, `/api/a2a/discover`, `/api/a2a/send`, `/api/a2a/tasks/{id}/status` (per `CLAUDE.md`).
- **OFP (OpenFang Protocol)** — custom peer-to-peer agent protocol in `crates/openfang-wire/` (`message.rs`, `peer.rs`, `registry.rs`). HMAC-authenticated; shared secret via config `network.shared_secret`. Listen on `127.0.0.1:4200` by default per `openfang.toml.example`; libp2p-style multiaddrs default `/ip4/0.0.0.0/tcp/0` (`crates/openfang-types/src/config.rs:1790`). API: `/api/network/status`, `/api/peers`.
- **MCP (Model Context Protocol)** — `rmcp` 1.2 client + server. Config: `[[mcp_servers]]` in `~/.openfang/config.toml` with `command`, `args` (e.g. `npx -y @modelcontextprotocol/server-filesystem /tmp`). Transports: child-process (stdio) and Streamable-HTTP. Code: `crates/openfang-runtime/src/mcp.rs` + `mcp_server.rs`.

### Other External
- Web search and content extraction — `crates/openfang-runtime/src/web_search.rs`, `web_fetch.rs`, `web_content.rs`, `web_cache.rs`.
- Browser automation — `crates/openfang-runtime/src/browser.rs`.
- Embeddings / TTS / image gen / media understanding — `crates/openfang-runtime/src/embedding.rs`, `tts.rs`, `image_gen.rs`, `media_understanding.rs` (call configured LLM providers).
- Docker / subprocess sandboxing — `crates/openfang-runtime/src/docker_sandbox.rs`, `subprocess_sandbox.rs`, `workspace_sandbox.rs`.

## Data Storage

**Databases:**
- SQLite (rusqlite 0.31, `bundled` — no system libsqlite required). All persistence flows through `crates/openfang-memory/src/substrate.rs` plus topic modules (`session.rs`, `semantic.rs`, `structured.rs`, `knowledge.rs`, `usage.rs`, `consolidation.rs`).
  - Connection: file at `~/.openfang/data/openfang.db` (override via `memory.sqlite_path` in `~/.openfang/config.toml`).
  - Schema migrations: `crates/openfang-memory/src/migration.rs`.
- HTTP-memory backend (feature `http-memory`, default on) — optional remote memory substrate via blocking `reqwest` in `crates/openfang-memory/src/http_client.rs`.
- Skills/extensions runtime tables also stored in the same SQLite DB (`crates/openfang-runtime` depends on `rusqlite`).

**File Storage:**
- Local filesystem rooted at `~/.openfang/` (configurable via `OPENFANG_HOME`; Docker maps to `/data` via `VOLUME /data`).
- Skills marketplace: zipped skill bundles downloaded via `reqwest` and unpacked with `zip` (`crates/openfang-skills`).
- Encrypted credential vault: `crates/openfang-extensions/src/vault.rs` using `aes-gcm` AEAD with `argon2` key derivation; secrets zeroized via `zeroize`.

**Caching:**
- In-memory web fetch cache (`crates/openfang-runtime/src/web_cache.rs`).
- In-memory model catalog / provider health caches (`crates/openfang-runtime/src/model_catalog.rs`, `provider_health.rs`).

## Authentication & Identity

**Inbound API auth:**
- Optional Bearer auth on the HTTP API. Set `OPENFANG_API_KEY` env (or `api_key` in config); when unset, the daemon binds localhost-only. Hash stored in `KernelConfig.api_key_hash` (`crates/openfang-types/src/config.rs:188`); constant-time compare via `subtle` (`crates/openfang-api/src/middleware.rs`).
- Session auth + WebSocket auth: `crates/openfang-api/src/session_auth.rs`, `ws.rs`.
- Rate limiting on API: `crates/openfang-api/src/rate_limiter.rs` (token-bucket via `governor`).
- Multi-user RBAC: `users` config section in `KernelConfig` (`crates/openfang-types/src/config.rs:1175`).

**LLM provider auth:**
- API keys from `~/.openfang/config.toml` (via `api_key_env`) or directly from process env. Subprocess CLIs (Claude Code, Qwen Code, Codex) bypass keys and reuse the local CLI's own credential store.
- GitHub Copilot OAuth device flow tokens at `~/.openfang/`.
- Google Vertex AI service-account JSON pointed to by `GOOGLE_APPLICATION_CREDENTIALS`.

**Peer / channel signing:**
- OFP peer authentication: HMAC + shared secret (`crates/openfang-wire`). Skill manifests are Ed25519-signed (`crates/openfang-types/src/manifest_signing.rs`).
- Slack request verification via HMAC-SHA256 over signing secret (`crates/openfang-channels/src/slack.rs`).
- Channel pairing flow (e.g. WhatsApp gateway): `crates/openfang-kernel/src/pairing.rs`, `whatsapp_gateway.rs`.

**Sandboxing & approval:**
- WASM (Wasmtime 43) — `crates/openfang-runtime/src/sandbox.rs`, host functions in `host_functions.rs`.
- Docker sandbox — `docker_sandbox.rs`.
- Subprocess sandbox — `subprocess_sandbox.rs`.
- Tool policy and approval gating — `crates/openfang-kernel/src/approval.rs`, `crates/openfang-runtime/src/tool_policy.rs`.
- Capability flags (bitflags) — `crates/openfang-types/src/capability.rs`.

## Monitoring & Observability

**Error tracking:**
- No external APM (no Sentry/Datadog/OpenTelemetry deps detected). Errors propagate via `thiserror` (typed) and `anyhow` (runtime).

**Logs:**
- `tracing` 0.1 + `tracing-subscriber` 0.3 with `env-filter` + `json` features. Level via `RUST_LOG` (e.g. `RUST_LOG=openfang=debug`).
- Audit log: `crates/openfang-runtime/src/audit.rs`.
- Metering / budgets: `crates/openfang-kernel/src/metering.rs`; exposed via `/api/budget`, `/api/budget/agents`, `/api/budget/agents/{id}` (per `CLAUDE.md`).
- Heartbeat / health: `crates/openfang-kernel/src/heartbeat.rs`, `/api/health` and `/api/status` (`crates/openfang-api/src/server.rs:172-177`).
- Provider health probing: `crates/openfang-runtime/src/provider_health.rs`.

## CI/CD & Deployment

**Hosting:**
- Distribution-only — there is no managed cloud. Users self-host.
- GitHub Container Registry: `ghcr.io/rightnow-ai/openfang:<tag>` (multi-arch `linux/amd64`+`linux/arm64`, set public via API in `release.yml`).
- GitHub Releases: signed/notarized Tauri bundles + CLI tarballs + auto-update `latest.json` manifest.
- Web installer at `https://openfang.sh/install` (+ PowerShell `install.ps1`), smoke-tested in CI (`.github/workflows/ci.yml` `install-smoke` job).

**CI Pipeline:**
- GitHub Actions, declared in `.github/workflows/`.
- `ci.yml`: jobs `check` (3 OSes), `test` (3 OSes), `clippy` (-D warnings), `fmt` (rustfmt --check), `audit` (cargo-audit), `secrets` (TruffleHog `--only-verified`), `install-smoke` (curl from openfang.sh).
- `release.yml`: triggered on tags `v*`. Jobs: `desktop` (5 platforms, Tauri signing + Apple notarization secrets), `cli` (7 targets incl. `armv7-unknown-linux-gnueabihf` via `cross`, macOS ad-hoc codesign), `docker` (multi-arch GHCR + visibility set to public).
- Dependabot config in `.github/dependabot.yml`.
- Issue and PR templates under `.github/ISSUE_TEMPLATE/`, `.github/pull_request_template.md`.

## Environment Configuration

**Required env vars (no hard requirement — pick the providers/channels you use):**
- At minimum one LLM key for the configured `default_model.provider` (e.g. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `GROQ_API_KEY`, etc.).
- `OPENFANG_API_KEY` — strongly recommended when binding outside localhost (Bearer auth).
- `OPENFANG_LISTEN` — overrides bind address (e.g. `0.0.0.0:4200` for Docker/LAN).
- `OPENFANG_HOME` — overrides `~/.openfang/`.
- `RUST_LOG` — overrides log level (default `info`).
- `OPENFANG_SUBPROCESS_TIMEOUT_SECS` — emergency knob for subprocess-driver timeouts (Claude Code, etc.).
- Channel-specific keys when those adapters are enabled (Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Email — see STACK.md "Configuration" section).
- Vertex AI: `GOOGLE_APPLICATION_CREDENTIALS` + `GOOGLE_CLOUD_PROJECT` (+ region).
- Bedrock: `AWS_REGION` (+ Bedrock API key in config).

**Secrets location:**
- Per-user `~/.openfang/config.toml` (mode-restricted in default install).
- `.env` file read at startup (`.env.example` is the template; `.env` is git-ignored — see `.gitignore`).
- Encrypted vault: `crates/openfang-extensions/src/vault.rs` (`aes-gcm` + `argon2`), used for credential storage and OAuth token persistence.
- Docker: secrets passed through environment in `docker-compose.yml`.
- CI: GitHub Actions secrets (`MAC_CERT_BASE64`, `MAC_CERT_PASSWORD`, `TAURI_SIGNING_PRIVATE_KEY`, `MAC_NOTARIZE_*`).

## Webhooks & Callbacks

**Incoming:**
- HTTP/WS API mounted by `crates/openfang-api/src/server.rs` on `OPENFANG_LISTEN` / `api_listen` (default `127.0.0.1:4200`). Routes include `/api/health`, `/api/status`, `/api/version`, `/api/agents`, `/api/agents/{id}/message`, `/api/agents/{id}/ws`, `/api/profiles`, `/api/budget` + sub-routes, `/api/network/status`, `/api/peers`, `/api/a2a/*`, `/api/chat`, plus channel-specific webhooks (Slack events, Discord interactions, WhatsApp Cloud API on port 8443, Telegram bot webhook, generic `/webhook/*`, MCP `streamable-http` endpoints).
- A2A discovery: `/.well-known/agent.json` (`crates/openfang-runtime/src/a2a.rs:24`).
- Web chat / SPA dashboard served at `/` with PWA assets (`/manifest.json`, `/sw.js`, `/favicon.ico`, `/logo.png`) — `crates/openfang-api/src/webchat.rs`.
- OpenAI-compatible proxy: `crates/openfang-api/src/openai_compat.rs` (mounts `/v1/...` style endpoints letting OpenFang act as an OpenAI-API drop-in).
- WebSocket streaming with dedup + chunking: `crates/openfang-api/src/stream_dedup.rs`, `stream_chunker.rs`, `ws.rs`.
- Channel ingress: HTTPS webhooks per adapter under `crates/openfang-channels/src/` (Slack, Discord, Telegram webhook mode, WhatsApp Cloud API, generic `webhook.rs`).

**Outgoing:**
- LLM provider HTTP/HTTPS calls from drivers under `crates/openfang-runtime/src/drivers/`.
- MCP server invocation (child process or HTTP) via `rmcp`.
- OFP peer-to-peer messages between OpenFang nodes (`crates/openfang-wire`).
- A2A outbound: `A2aClient` POSTs to remote agent cards (`crates/openfang-runtime/src/a2a.rs`).
- Channel egress: per-adapter publish APIs (Telegram `sendMessage`, Slack `chat.postMessage`, Discord `interactions`, IMAP/SMTP, MQTT publish, etc.).
- Cron-triggered delivery: `crates/openfang-kernel/src/cron.rs` + `cron_delivery.rs` fire scheduled messages out to channels.
- Auto-reply / triggers / workflows: `crates/openfang-kernel/src/auto_reply.rs`, `triggers.rs`, `workflow.rs`.

---

*Integration audit: 2026-06-06*
