---
phase: 01-self-learning-core
plan: 13
type: execute
wave: 3
depends_on: [01-11, 01-12]
files_modified:
  - crates/openfang-runtime/src/tool_runner.rs
  - crates/openfang-kernel/src/kernel.rs
files_created:
  - crates/openfang-reasoning/src/kernel_llm.rs
autonomous: true
must_haves:
  truths:
    - "Agent calls `memory_reason(query, reasoning_level=\"medium\")` and receives JSON `{answer, supporting_facts, confidence, level, caveats, estimated_cost_usd}` (MR-04, MR-07, success-criterion 5)"
    - "Agent calls `memory_reason(query, reasoning_level=\"max\")` without `approved=true` returns an `ApprovalRequired` JSON error (skeleton open-decision 5, success-criterion 6)"
    - "When monthly budget is exceeded AND `budget_exceeded_action=\"warn\"`: subsequent calls are forced to Low and a structured warning is in the result; tracker records the transition (MR-08, success-criterion 7)"
    - "When monthly budget is exceeded AND `budget_exceeded_action=\"block\"`: subsequent calls return `BudgetExceeded` JSON error (MR-08, success-criterion 7)"
    - "Tool is registered in dispatch + schema list using `// === PHASE 1 PLAN 01-13 memory_reason ===` anchors (X-01)"
  artifacts:
    - "`async fn tool_memory_reason(input: Value, kernel: &dyn KernelHandle) -> Result<String, String>` in tool_runner.rs"
    - "`crates/openfang-reasoning/src/kernel_llm.rs` with `pub struct KernelLlmAdapter` impl `ReasoningLlm` calling `KernelHandle::complete`"
    - "ReasoningEngine + BudgetTracker initialized inside `OpenFangKernel::boot` and made reachable via KernelHandle"
  key_links:
    - "Anchor comments `// === PHASE 1 PLAN 01-13 memory_reason ===` and `// === END PHASE 1 PLAN 01-13 ===` wrap every addition to tool_runner.rs"
    - "KernelLlmAdapter wraps a `Weak<OpenFangKernel>` (mirrors the Weak handle pattern used elsewhere — CONVENTIONS.md) to avoid cycles"
---

<objective>
Expose `memory_reason` to agents, wired through the kernel-backed LLM adapter and the BudgetTracker. This is the top of the reasoning track.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-runtime/src/tool_runner.rs
@crates/openfang-kernel/src/kernel.rs
@crates/openfang-runtime/src/kernel_handle.rs
@crates/openfang-reasoning/src/lib.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: KernelLlmAdapter</name>
  <files>crates/openfang-reasoning/src/kernel_llm.rs, crates/openfang-reasoning/src/lib.rs</files>
  <action>
Create `kernel_llm.rs`:
```rust
//! KernelHandle-backed implementation of ReasoningLlm — synthesizes via the
//! agent's existing LLM provider (no new HTTP client).

use std::sync::Weak;
use async_trait::async_trait;
use crate::{FactReference, ReasoningError, ReasoningLevel, ReasoningLlm};

pub struct KernelLlmAdapter<KH: KernelHandleLike + ?Sized> {
    kernel: Weak<KH>,
}
```
The adapter exposes `pub fn new(kernel: Weak<KH>) -> Self`. `KernelHandleLike` is the existing `KernelHandle` trait (just call it directly via `openfang_runtime::kernel_handle::KernelHandle`). impl ReasoningLlm: build a prompt — depth-tuned per `level` — that lists the facts (`for f in facts { write!(prompt, "- [{}] {}\n", f.source_short(), f.content)?; }`) and asks for synthesis. Call `kernel.upgrade().ok_or_else(...).complete(...)` against the kernel's existing LLM driver via whatever method the KernelHandle exposes (likely `complete` or a thin `reason` helper). Wrap errors as `ReasoningError::Llm(e.to_string())`. Truncate prompt at `cfg.max_input_tokens * 4 chars` (4 chars≈1 token heuristic).

Re-export from lib.rs: `pub mod kernel_llm; pub use kernel_llm::KernelLlmAdapter;`.
  </action>
  <verify>
`cargo build -p openfang-reasoning --lib` clean.
  </verify>
  <done>
Adapter compiles, holds `Weak<KH>`, calls KernelHandle LLM.
  </done>
</task>

<task type="auto">
  <name>Task 2: Wire ReasoningEngine + BudgetTracker into OpenFangKernel</name>
  <files>crates/openfang-kernel/src/kernel.rs, crates/openfang-runtime/src/kernel_handle.rs</files>
  <action>
Add to `OpenFangKernel`:
- `pub reasoning_engine: Arc<openfang_reasoning::ReasoningEngine>`,
- `pub budget_tracker: Arc<openfang_reasoning::BudgetTracker>`.
Initialize at the end of `boot()` (or whichever fn finalizes the kernel) after `MemorySubstrate` is ready:
```
let budget_tracker = Arc::new(BudgetTracker::new(memory.clone(), cfg.reasoning.monthly_budget_usd));
let llm_adapter   = Arc::new(KernelLlmAdapter::new(Arc::downgrade(&kernel_arc)));
let reasoning_engine = Arc::new(
    ReasoningEngine::new(memory.clone()).with_llm(llm_adapter)
);
```
Add to `Cargo.toml` of openfang-kernel: `openfang-reasoning = { path = "../openfang-reasoning" }`.

Add to `KernelHandle` trait two thin accessors:
```
fn reasoning_engine(&self) -> Arc<openfang_reasoning::ReasoningEngine>;
fn budget_tracker(&self)   -> Arc<openfang_reasoning::BudgetTracker>;
fn reasoning_config(&self) -> openfang_types::config::ReasoningConfig;
```
Implement on OpenFangKernel by cloning the Arc / cloning the config. Call `log_effective_reasoning_config(&cfg.reasoning, &cfg_path)` from boot (plan 01-12 task 3 expectation).
  </action>
  <verify>
`cargo build -p openfang-kernel -p openfang-runtime --lib` clean.
`cargo test -p openfang-kernel boot` — existing boot tests still pass.
  </verify>
  <done>
Kernel exposes the reasoning subsystem to KernelHandle consumers.
  </done>
</task>

<task type="auto">
  <name>Task 3: tool_memory_reason dispatch + schema + budget logic + tests</name>
  <files>crates/openfang-runtime/src/tool_runner.rs</files>
  <action>
Around line 203, anchored:
```
        // === PHASE 1 PLAN 01-13 memory_reason ===
        "memory_reason" => tool_memory_reason(input, kernel).await,
        // === END PHASE 1 PLAN 01-13 ===
```
Body of `tool_memory_reason`:
1. Parse `input["query"]: String` (required), `input["reasoning_level"]: String` default `"medium"`, `input["max_facts"]: usize` optional, `input["approved"]: bool` optional default false.
2. Map level string to `ReasoningLevel`; unknown level returns `Ok(json!({"error":"InvalidLevel"}).to_string())`.
3. Read `kernel.reasoning_config()`. If `level > parse(cfg.max_level)` → return `LevelNotAllowed` JSON.
4. Check budget: `spent = kernel.budget_tracker().current_month_spent()?;` if `spent >= cfg.monthly_budget_usd`:
   - `budget_exceeded_action == "block"` → return `BudgetExceeded` JSON.
   - `budget_exceeded_action == "warn"` → force `level = ReasoningLevel::Low` and add a caveat `"Monthly reasoning budget exceeded — downgraded to Low."`. Tracker records the downgrade in the next `record(...)` call (mark with a query_preview prefix `"[downgraded]"`).
5. If `level == Max && cfg.require_approval_for_max && !approved` → return `ApprovalRequired` JSON: `json!({"error":"ApprovalRequired","level":"max","estimated_cost_usd":<estimate>,"query_preview":<first 80 chars>})`.
6. Call `kernel.reasoning_engine().reason(ReasoningQuery {..}).await`. Map result to JSON. Surface `estimated_cost_usd` at the top level (MR-07).
7. Record into BudgetTracker (`record(BudgetRecord { timestamp: now, level, input_tokens: 0 if Minimal else heuristic, output_tokens: heuristic, estimated_cost_usd, query_preview: truncated })`).

Around line 645, schema entry anchored `// === PHASE 1 PLAN 01-13 memory_reason schema ===`:
```
json!({
  "name": "memory_reason",
  "description": "Ask questions about the user based on accumulated memory. Returns synthesized answers, not just raw facts. Use reasoning_level to control depth: minimal (fast/cheap), low, medium, high, max (deep/expensive).",
  "input_schema": { "type": "object", "properties": {
    "query": { "type": "string" },
    "reasoning_level": { "type": "string", "enum": ["minimal","low","medium","high","max"] },
    "max_facts": { "type": "integer" },
    "approved": { "type": "boolean", "description": "Required when reasoning_level=\"max\" and config require_approval_for_max=true" }
  }, "required": ["query"] }
}),
```

Unit tests with fake KernelHandle:
- `memory_reason_medium_returns_answer_facts_confidence` — fake engine returns a fixed ReasoningResult; assert JSON has all 6 fields incl. `estimated_cost_usd`.
- `memory_reason_max_without_approved_returns_approval_required` — assert JSON error == `ApprovalRequired`.
- `memory_reason_max_with_approved_proceeds` — assert engine called.
- `memory_reason_budget_exceeded_warn_downgrades_to_low` — fake tracker reports spent > budget, action=warn; assert engine called with `level=Low` and result caveats contain the downgrade message.
- `memory_reason_budget_exceeded_block_returns_error` — same but action=block; assert JSON error == `BudgetExceeded`.
- `memory_reason_level_above_max_level_rejected` — cfg.max_level=medium, input level=high → `LevelNotAllowed`.
- `memory_reason_records_to_budget_tracker` — fake tracker counts calls; after a successful reason, count == 1.
  </action>
  <verify>
`cargo test -p openfang-runtime memory_reason` runs ≥ 7 tests.
`cargo clippy --workspace --all-targets -- -D warnings` clean.
`cargo build --workspace --lib` clean.
  </verify>
  <done>
Tool wired, budget enforced, approval gate enforced, anchors present.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Live (combined with 01-14, in 01-16 checkpoint): `curl POST /api/agents/{id}/message` instructing the agent to call memory_reason at medium and max levels; verify budget row inserts via `sqlite3` query against `reasoning_budget`.
</verification>

<success_criteria>
- [ ] `memory_reason` dispatchable from the agent.
- [ ] Result includes `answer`, `supporting_facts`, `confidence`, `level`, `caveats`, `estimated_cost_usd`.
- [ ] Max requires `approved=true` when `require_approval_for_max=true`.
- [ ] Warn mode downgrades; block mode returns `BudgetExceeded`.
- [ ] Every reason call records in `reasoning_budget`.
- [ ] Both anchor blocks present in tool_runner.rs.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-13-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- KernelHandle accessor surface added
- Any follow-ups for later plans (notably 01-14's profile writes call into the same engine)
</output>
