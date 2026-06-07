# Plan 01-13 — memory_reason tool + KernelLlmAdapter + BudgetTracker wiring — SUMMARY

**One-liner.** The agent-facing `memory_reason` tool is wired end-to-end: a `KernelLlmAdapter` in `openfang-reasoning` calls back into the kernel's existing `LlmDriver` (via a tiny `KernelLlm` seam to avoid the crate-DAG cycle), the `ReasoningEngine` gains pre/post-call `BudgetTracker` hooks, and the tool layer enforces the level ceiling + Max approval gate + budget action mode before invoking the engine.

## What was implemented

- New `KernelLlm` trait + `KernelLlmAdapter<K: KernelLlm>` in `openfang-reasoning::kernel_llm`. The adapter wraps `Weak<K>` (mirrors the `self_handle: Weak<OpenFangKernel>` pattern, per CONVENTIONS.md), builds a level-tuned prompt from query + facts, truncates to `max_input_tokens * 4` chars at UTF-8 boundaries, and forwards to `K::complete_for_reasoning(prompt, max_output_tokens, level)`. Returns the driver's real `TokenUsage` via the new `ReasoningLlm::synthesize_with_usage` default method.
- `ReasoningLlm::synthesize_with_usage` (new default method) returns `(String, TokenUsage)`. `engine.rs::reason_low`/`reason_deep` use it, falling back to the chars/4 coarse estimate only when `usage.total() == 0` (mock LLMs in tests).
- `ReasoningEngine::with_budget(tracker, budget_exceeded_action)` builder method + new `Option<Arc<BudgetTracker>>` field. `reason()` now delegates to `engine::reason_with_budget`, which: pre-call compares `current_month_spent` vs `monthly_budget_usd`, blocks (returning `ReasoningError::BudgetExceeded`) or downgrades to `Low` (adding a structured caveat); post-call records one `BudgetRecord` row (tracked failures are `tracing::warn` — never fail user-facing reasoning).
- `OpenFangKernel` gains two `OnceLock` fields (`budget_tracker`, `reasoning_engine`) populated by a new `init_reasoning_subsystem()` helper invoked from `set_self_handle()` (so the `Weak<OpenFangKernel>` is available by then). Boot also logs the effective `[reasoning]` config via `log_effective_reasoning_config` (plan 01-12 task 3).
- `OpenFangKernel` implements `openfang_reasoning::KernelLlm` by building a `CompletionRequest` against `self.default_driver` with a level-tuned temperature (0.0 Minimal → 0.1 Max) and a memory-recall system anchor.
- `KernelHandle` trait gains three new accessors with safe `None` defaults (`reasoning_engine`, `budget_tracker`, `reasoning_config`). `OpenFangKernel` overrides all three.
- `openfang-runtime` declares a new dep on `openfang-reasoning` so the runtime crate can name the types in the trait signatures.
- `tool_memory_reason` async dispatcher in `openfang-runtime::tool_runner`, inside `// === PHASE 1 PLAN 01-13 memory_reason ===` anchor blocks. Dispatch arm at the memory-tools cluster (lines 306–307 region); schema entry inside the `// === PHASE 1 PLAN 01-13 memory_reason schema ===` block. Result shape includes the 6 MR-04 fields (`answer`, `supporting_facts`, `confidence`, `level`, `caveats`, `estimated_cost_usd`). Engine errors are mapped to structured JSON via `reasoning_error_to_json`.

## Files changed

- `crates/openfang-reasoning/src/lib.rs` — `KernelLlm`/`KernelLlmAdapter` re-exports, new `synthesize_with_usage` default method, `with_budget` builder, `has_budget`, two new fields on `ReasoningEngine`.
- `crates/openfang-reasoning/src/engine.rs` — `reason_with_budget` wrapper, real-TokenUsage branch in `reason_low`/`reason_deep`, 3 new budget integration tests.
- `crates/openfang-reasoning/src/kernel_llm.rs` (new) — adapter + prompt builder + safe_truncate + 4 unit tests.
- `crates/openfang-runtime/Cargo.toml` — `openfang-reasoning` path dep.
- `crates/openfang-runtime/src/kernel_handle.rs` — 3 new trait accessors with safe defaults.
- `crates/openfang-runtime/src/tool_runner.rs` — anchored dispatch arm + schema + `tool_memory_reason` + `parse_reasoning_level` + `reasoning_error_to_json` + 8 unit tests with a `ReasonFakeKernel`.
- `crates/openfang-kernel/src/kernel.rs` — two new `OnceLock` fields, `init_reasoning_subsystem()` helper called from `set_self_handle`, KernelHandle overrides for the 3 accessors, full `KernelLlm` impl forwarding to `default_driver.complete`.

## Tests added (15)

In `openfang-reasoning::kernel_llm::tests` (4):
- `adapter_returns_real_token_usage`
- `adapter_errors_when_kernel_dropped`
- `adapter_truncates_prompt_to_max_input_tokens_times_four`
- `safe_truncate_respects_char_boundaries`

In `openfang-reasoning::engine::tests` (3):
- `budget_block_returns_budget_exceeded_when_over_limit`
- `budget_warn_downgrades_to_low_and_adds_caveat`
- `budget_records_one_row_per_successful_call`

In `openfang-runtime::tool_runner::tests` (8):
- `memory_reason_medium_returns_answer_facts_confidence`
- `memory_reason_max_without_approved_returns_approval_required`
- `memory_reason_max_with_approved_proceeds`
- `memory_reason_budget_exceeded_warn_downgrades_to_low`
- `memory_reason_budget_exceeded_block_returns_error`
- `memory_reason_level_above_max_level_rejected`
- `memory_reason_records_to_budget_tracker`
- `memory_reason_unknown_level_returns_invalid_level`

## KernelHandle accessor surface added

```rust
fn reasoning_engine(&self) -> Option<Arc<ReasoningEngine>>;
fn budget_tracker(&self)   -> Option<Arc<BudgetTracker>>;
fn reasoning_config(&self) -> Option<openfang_types::config::ReasoningConfig>;
```

All three default to `None` so existing test fakes keep compiling. `OpenFangKernel` returns `Some(...)` once `set_self_handle` has run.

## Decisions made during execution

1. **Local `KernelLlm` trait in `openfang-reasoning`.** The plan suggested the adapter call `KernelHandle::complete`, but `KernelHandle` lives in `openfang-runtime` and `openfang-reasoning` cannot depend on runtime (cycle). Introduced a tiny `KernelLlm` trait in the reasoning crate with one method (`complete_for_reasoning`) — the kernel implements it directly. Same architectural pattern used in plan 01-05 for `AuditAppend`/`SkillEventBus`.
2. **`synthesize_with_usage` as a default method.** Rather than break the existing `ReasoningLlm::synthesize -> String` contract, added a second method that returns `(String, TokenUsage)` with a default impl that delegates and returns a zero TokenUsage. The adapter overrides it; tests that still use the old `synthesize`-only path continue to work. The engine reads `usage.total() > 0` to decide which numbers to feed the cost estimator.
3. **`set_self_handle` does both jobs.** The reasoning engine needs `Weak<OpenFangKernel>` (available only after Arc wrapping). Hooking `init_reasoning_subsystem` into `set_self_handle` means existing callers that already invoke `set_self_handle` (all binaries + tests) automatically get reasoning wired without needing to add a second call.
4. **Tool-layer pre-call budget check duplicates the engine's check.** This is intentional: the engine's check is the architectural source of truth (plan invariant 6); the tool layer's check exists so block-mode returns a `BudgetExceeded` JSON **without invoking the engine at all**, which is a tighter UX contract than letting the engine bubble the error up.
5. **Max approval gate is tool-layer only.** The engine has no concept of `approved` (that's a tool-input field). The tool layer applies the gate, returns `ApprovalRequired` JSON, and on a follow-up call with `approved=true` it skips the gate and proceeds. The cost estimate uses the conservative Max per-1k pricing (0.015 in + 0.075 out for 1k tokens each) so the user sees a realistic ceiling.
6. **`reasoning_error_to_json` is sync** — the engine's error variants all carry data already; we just rename to structured JSON keys (`LevelNotAllowed`, `ApprovalRequired`, `BudgetExceeded`, `MemoryAccess`, `LlmSynthesis`, `NotYetImplemented`).
7. **`init_reasoning_subsystem` uses `OnceLock`** (not unconditional initialization), so calling `set_self_handle` twice (which existing test code does in places) doesn't panic or rebuild the engine.

## Deviations vs plan

- **[Rule 4 mitigation, not invoked]** The plan assumed `KernelHandle::complete` existed; it didn't. Rather than blocking on a Rule 4 architectural decision, the cleanest fix was the `KernelLlm` trait pattern (matches plan 01-05's adapter pattern verbatim). No new public contract; the kernel just implements one more local trait.
- **[Rule 3 mechanical]** Adding `openfang-reasoning` as a direct dep of `openfang-runtime` was required so the trait signatures can name `ReasoningEngine`/`BudgetTracker`/`ReasoningConfig`. The kernel already depends on both.
- **[Plan text says `kernel: &dyn KernelHandle`]** — actual signature is `kernel: Option<&Arc<dyn KernelHandle>>` to match every other tool function in `tool_runner.rs` (lines 306–307 etc.) and to enable the no-kernel branch returning a structured `KernelUnavailable` JSON. Behaviorally identical for the happy path.

## Follow-ups for W4

- **01-14 (user profile + conclude):** the plan-13 contract is the foundation for profile-write/auto-update. Profile writes will likely call `reasoning_engine().reason(...)` at a fixed level (Low or Medium) and apply the result to the structured KV store. The `KernelHandle::reasoning_engine` accessor added here is the entry point — 01-14 **must not** re-add these accessors.
- **01-04 (session search lift):** `tool_memory_reason` doesn't yet exercise FTS5 session search — the fact retrieval module (plan 01-11) covers that path. When 01-04 lifts `fts5_session_search` into `SessionStore::search_sessions_fts`, the engine's `retrieve_facts` will switch over transparently.
- **01-16 (live integration checkpoint):** real LLM exercise: start daemon with `GROQ_API_KEY=...`, call `memory_reason` at `medium` and at `max` (with `approved=true`) levels. Verify `sqlite3 ~/.openfang/memory.db "SELECT * FROM reasoning_budget"` shows rows; verify the engine actually invokes the driver.
- **Dashboard:** the `[reasoning]` config block already exists (plan 01-12); a future dashboard tab can surface `budget_tracker.current_month_spent()` + `monthly_budget_usd()` for the cost gauge.

## Workspace gates (after this plan)

- `cargo build --workspace --lib` — clean.
- `cargo test --workspace` — 2808 passed (was 2793 before this plan; +15).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.

## Commits

- `51c0d5b` — feat(01-13): KernelLlmAdapter
- `1260b07` — feat(01-13): wire BudgetTracker pre/post-call onto ReasoningEngine
- `7b39b72` — feat(01-13): memory_reason tool
