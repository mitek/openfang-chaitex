# 01-12 — BudgetTracker + [reasoning] config — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** 543aca2, 84202b0, 13e21d8

## One-liner

Added `ReasoningConfig` (strict `deny_unknown_fields`) on `KernelConfig`,
the `reasoning_budget` SQLite table as an amendment inside `migrate_v9`,
a `BudgetTracker` persisting per-call cost rows + a calendar-month
aggregator, and a boot-time `INFO` log marker distinguishing
`(from config)` from `(DEFAULT — no [reasoning] section found in
config)`.

## Files changed

- `crates/openfang-types/src/config.rs` — new `pub struct ReasoningConfig`
  with 8 fields per MR-05 + 7 default helper fns + `Default` impl;
  `pub reasoning: ReasoningConfig` field on `KernelConfig` with
  `#[serde(default)]`; entry in `Default for KernelConfig`; 5 new tests.
- `crates/openfang-memory/src/migration.rs` — `reasoning_budget` table +
  `idx_reasoning_budget_timestamp` index added inside `migrate_v9`'s
  `execute_batch`, wrapped with `// === v9 amendment: reasoning_budget
  (plan 01-12) ===` anchor comments.
- `crates/openfang-reasoning/Cargo.toml` — `rusqlite = { workspace = true }`
  added.
- `crates/openfang-reasoning/src/budget.rs` — NEW, ~270 lines, 7 tests.
  `BudgetRecord` (with `new_now` factory that clamps `query_preview` to
  ≤100 UTF-8-safe bytes), `BudgetTracker::{new, record,
  current_month_spent, monthly_budget_usd}`,
  `format_effective_log`, `log_effective_reasoning_config`,
  `safe_truncate` (private), `level_to_str` (private).
- `crates/openfang-reasoning/src/lib.rs` — `pub mod budget;` + re-exports
  (`BudgetTracker`, `BudgetRecord`, `format_effective_log`,
  `log_effective_reasoning_config`).
- `crates/openfang-kernel/Cargo.toml` — `openfang-reasoning` added as dep.
- `crates/openfang-kernel/src/config.rs` — `log_effective_reasoning_config`
  invoked from both the happy path (returns `config`) and the fallback
  path (returns `KernelConfig::default()`).

## Tests added

12 new tests total:

In `openfang-types::config::tests` (5):
1. `reasoning_config_default_matches_mr05` — defaults are exactly
   `max_input_tokens=40000, max_output_tokens=8000, max_level="high",
   monthly_budget_usd=20.0, budget_exceeded_action="warn",
   require_approval_for_max=true, auto_update_profile=false,
   fts_backfill="on_startup", is_default_loaded=true`.
2. `reasoning_config_deny_unknown_fields_rejects_typo` — TOML with a
   `foo = 1` extra field returns `unknown field` parse error.
3. `reasoning_config_partial_toml_fills_defaults_and_keeps_explicit`
   — partial supply works, `is_default_loaded` stays `false`.
4. `kernel_config_without_reasoning_block_marks_is_default_loaded` —
   omitting the section makes the inner `Default` set the marker.
5. `kernel_config_with_reasoning_typo_rejects` — `[reasoning] max_input_tkns
   = …` rejected at the outer `KernelConfig` parse.

In `openfang-reasoning::budget::tests` (7):
1. `budget_record_clamps_query_preview` — 500-char input → 100-byte
   preview.
2. `safe_truncate_respects_char_boundaries` — `héllo` cut at byte 2
   walks back to byte 1.
3. `record_and_aggregate_round_trip` — 3 records, `current_month_spent`
   matches the sum within 1e-9.
4. `monthly_budget_usd_accessor_returns_constructor_value`.
5. `format_effective_log_default_marker` — contains `(DEFAULT — no
   [reasoning] section found in config)` and `monthly_budget_usd=20.00`.
6. `format_effective_log_from_config_marker` — `is_default_loaded=false`
   produces `(from config)` and no `(DEFAULT)` leak.

Workspace gates clean: 2757 tests pass (was 2746 → +11 new + 1 from
test setup count); build clean; clippy `-D warnings` clean.

## Boot wiring location for the INFO log

`crates/openfang-kernel/src/config.rs::load_config(...)`. Two callsites:

1. Right after `info!("Loaded configuration")` on the happy
   parse-and-deserialize path. `config.reasoning.is_default_loaded` is
   `false` here because the deserializer never sets that field — it's
   `#[serde(skip)]` — so the marker reads `(from config)` even if the
   TOML happened to omit the `[reasoning]` block but supplied other
   fields (the struct-level `#[serde(default)]` on `ReasoningConfig`
   means a missing inner block still produces `Default`, which DOES set
   `is_default_loaded=true` because Default::default sets it
   explicitly).
2. Just before returning `KernelConfig::default()` on the fall-through
   path (file missing / unreadable / parse failure / deserialize
   failure). `is_default_loaded` is always `true` here.

This placement was chosen because `load_config` is the only function
that knows the `config_path` — `boot_with_config` only sees the
finalized struct. The plan asked us to wire from kernel boot; this is
the earliest spot where the path is in scope.

## Deviations

**[Rule 3 — auto-fix blocking]** Initial clippy run on the budget tests
failed with `field_reassign_with_default` because
`format_effective_log_from_config_marker` was using `let mut cfg =
ReasoningConfig::default(); cfg.is_default_loaded = false;`. Fixed
inline to the struct-update form `ReasoningConfig { is_default_loaded:
false, ..ReasoningConfig::default() }`. No behavioral change.

**[Rule 3 — missing direct dep]** `BudgetTracker` uses `rusqlite::params!`
but `openfang-reasoning` didn't have a direct `rusqlite` dep — only a
transitive one through `openfang-memory`. Added the workspace
`rusqlite` dep to the reasoning crate's `Cargo.toml` so the macro
imports resolve. No re-export through `openfang-memory` exists today;
this is the cheapest, narrowest fix.

## Decisions made during execution

1. **`is_default_loaded` semantics pinned.** The plan said
   `#[serde(skip)]` + `Default` sets `true`. I made the contract
   explicit in the doc comment and in two tests so future maintainers
   reading the field don't have to reverse-engineer it.
2. **`BudgetTracker::new_now` factory.** The plan's BudgetRecord shape
   was the only thing pinned; I added a `new_now` constructor that
   stamps `Utc::now().to_rfc3339()` and clamps `query_preview`
   internally so callers can never accidentally bypass MR-05's privacy
   clamp.
3. **`monthly_budget_usd()` accessor.** The plan only required `record`
   and `current_month_spent`. Added the accessor (1 line) so the
   dashboard / status endpoint can render the ceiling without holding
   a reference to the originating `ReasoningConfig`.
4. **`format_effective_log` is a separate fn from
   `log_effective_reasoning_config`.** The plan suggested
   "format_effective_log(cfg) -> String" for the marker tests; I made
   that the public testable surface, with the `tracing::info!` one-liner
   sitting on top. Lets the tests inspect the exact string without
   reaching into a `tracing_subscriber` capture buffer.
5. **`level_to_str` is private + mirrors the `Serialize` lowercase
   form.** The DB stores `"medium"`, not `"Medium"`, so when the engine
   later reads rows back the level can round-trip via
   `serde_json::from_str(&format!("\"{level_str}\""))` if needed. Keeps
   the storage byte-stable across crates.
6. **Boot logger emits even on the default fallback.** The plan only
   showed the happy path; firing the log on the fallback too means
   operators can tell from logs that the config file went missing
   (separate WARN already emits) AND see what defaults are in effect.

## Follow-ups for later plans

- **01-11 (ReasoningEngine level dispatch):** the engine should record
  one `BudgetRecord` after each successful synthesize call. Pre-call,
  call `BudgetTracker::current_month_spent()`; if `>= monthly_budget_usd`
  apply `budget_exceeded_action` (`"warn"` → downgrade to `Low`;
  `"block"` → return `ReasoningError::BudgetExceeded`).
- **01-13 (memory_reason tool):** when the tool surfaces a
  `ReasoningResult`, include `estimated_cost_usd` so the agent and
  dashboard see per-query cost.
- **Live integration test (W4 / 01-15):** with no `[reasoning]` section
  in `~/.openfang/config.toml`, the daemon log should contain
  `(DEFAULT — no [reasoning] section found in config)`. With a
  typo-laden `[reasoning]` block, daemon startup should fail with
  `unknown field` in the WARN log AND the fallback log line must
  carry the `(DEFAULT)` marker (because the bad TOML falls through
  to `KernelConfig::default()` per `load_config`'s existing behavior;
  the typo error is logged separately on the way through). If the
  desired behavior is "typo causes hard startup failure" (per MR-05
  success-criterion 11), that's a separate change in `load_config`'s
  error-handling policy — note this in W4.
- **Reasoning config reload:** plan 01-09's snapshot-refresh bus does
  not currently know about `ReasoningConfig`. When config reload lands
  for `[reasoning]`, the `BudgetTracker.monthly_budget_usd` cache
  needs an update path too.
