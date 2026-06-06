---
phase: 01-self-learning-core
plan: 12
type: execute
wave: 2
depends_on: [01-10]
files_modified:
  - crates/openfang-types/src/config.rs
  - crates/openfang-memory/src/migration.rs
files_created:
  - crates/openfang-reasoning/src/budget.rs
autonomous: true
must_haves:
  truths:
    - "A typo in the `[reasoning]` config block fails daemon startup with a `deny_unknown_fields` parse error (MR-05, success-criterion 11)"
    - "Default config (no `[reasoning]` section) logs at INFO `(DEFAULT — no [reasoning] section)` for each effective field; explicit config logs `(from config)` (MR-05, addendum § C.2, success-criterion 11)"
    - "`BudgetTracker::record(level, in_tokens, out_tokens, cost)` persists to `reasoning_budget` SQLite table; `current_month_spent()` aggregates the calendar month (MR-05, MR-07)"
    - "Defaults match REQ MR-05: `max_input_tokens=40000, max_output_tokens=8000, max_level=\"high\", monthly_budget_usd=20.0, budget_exceeded_action=\"warn\", require_approval_for_max=true, auto_update_profile=false, fts_backfill=\"on_startup\"`"
  artifacts:
    - "`pub struct ReasoningConfig` with `#[serde(deny_unknown_fields)]` in crates/openfang-types/src/config.rs"
    - "`pub reasoning: ReasoningConfig` field on `KernelConfig`"
    - "`crates/openfang-reasoning/src/budget.rs` with `pub struct BudgetTracker` + `pub struct BudgetRecord`"
    - "`reasoning_budget` SQLite table created by an amendment inside v9 migration (plan 01-02 territory — see Task 3)"
---

<objective>
Ship the `[reasoning]` config block (strict deny_unknown_fields), the startup INFO log that distinguishes defaults from explicit config, and the persistent BudgetTracker. The tracker writes to a new SQLite table created as part of schema v9.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-types/src/config.rs
@crates/openfang-memory/src/migration.rs
@crates/openfang-reasoning/src/lib.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: `ReasoningConfig` struct + default values + KernelConfig wiring</name>
  <files>crates/openfang-types/src/config.rs</files>
  <action>
Add `pub struct ReasoningConfig`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningConfig {
    #[serde(default = "default_max_input_tokens")]   pub max_input_tokens: u32,
    #[serde(default = "default_max_output_tokens")]  pub max_output_tokens: u32,
    #[serde(default = "default_max_level")]          pub max_level: String,
    #[serde(default = "default_monthly_budget_usd")] pub monthly_budget_usd: f64,
    #[serde(default = "default_budget_action")]      pub budget_exceeded_action: String,
    #[serde(default = "default_require_approval")]   pub require_approval_for_max: bool,
    #[serde(default)]                                pub auto_update_profile: bool,
    #[serde(default = "default_fts_backfill")]       pub fts_backfill: String,
    /// True only when this config came from defaults because the user's TOML
    /// had no `[reasoning]` section. Skipped by serde; set by KernelConfig::load.
    #[serde(skip)]                                   pub is_default_loaded: bool,
}
```
Free fns return the literal defaults from `must_haves.truths`. Add `Default` impl matching the same values + `is_default_loaded: true`. Add the field on `KernelConfig` (line 1145):
```rust
#[serde(default)]
pub reasoning: ReasoningConfig,
```
The TOML loader logic that produces a `KernelConfig` from a file: when the source TOML did NOT contain `[reasoning]`, the field deserializes via the `#[serde(default)]` Default impl which sets `is_default_loaded=true`. When it DID contain `[reasoning]`, deny_unknown_fields applies + `is_default_loaded` stays false. Confirm by re-reading the loader path (likely in openfang-kernel or openfang-cli).
  </action>
  <verify>
`cargo build -p openfang-types --lib` clean.
Test in `mod tests`: `toml::from_str::<ReasoningConfig>("max_input_tokens = 30000\nfoo = 1\nmax_level = \"medium\"")` returns an error containing `"unknown field"`. Test that an empty TOML yields a `Default::default()`-equivalent with `is_default_loaded=true`.
  </verify>
  <done>
ReasoningConfig present with deny_unknown_fields; KernelConfig has the field; Default impl matches the required values.
  </done>
</task>

<task type="auto">
  <name>Task 2: BudgetTracker + `reasoning_budget` table amendment to v9</name>
  <files>crates/openfang-reasoning/src/budget.rs, crates/openfang-memory/src/migration.rs, crates/openfang-reasoning/src/lib.rs</files>
  <action>
Extend `migrate_v9` from plan 01-02 with one more `CREATE TABLE`:
```sql
CREATE TABLE IF NOT EXISTS reasoning_budget (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    level TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    estimated_cost_usd REAL NOT NULL,
    query_preview TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_reasoning_budget_timestamp
    ON reasoning_budget(timestamp);
```
This is an ADDITION to the v9 block 01-02 wrote — keep it inside the same `execute_batch`. Coordinate via a comment `// === v9 amendment: reasoning_budget (plan 01-12) ===`.

Create `crates/openfang-reasoning/src/budget.rs`:
```rust
pub struct BudgetTracker {
    memory: Arc<MemorySubstrate>,
    monthly_budget_usd: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetRecord {
    pub timestamp: String,
    pub level: ReasoningLevel,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost_usd: f64,
    pub query_preview: String,
}
impl BudgetTracker {
    pub fn new(memory: Arc<MemorySubstrate>, monthly_budget_usd: f64) -> Self { ... }
    pub fn record(&self, rec: BudgetRecord) -> Result<(), ReasoningError>;
    pub fn current_month_spent(&self) -> Result<f64, ReasoningError>;
}
```
`record` INSERTS into `reasoning_budget`. `current_month_spent` runs `SELECT COALESCE(SUM(estimated_cost_usd),0) FROM reasoning_budget WHERE timestamp >= ?1` where `?1` is the first-of-month ISO-8601 string. Use the connection accessor on `MemorySubstrate`. Wrap rusqlite errors as `ReasoningError::Memory`. `query_preview` MUST be truncated to ≤100 chars per MR-05; use a UTF-8-safe truncate.

Re-export from `lib.rs`: `pub mod budget; pub use budget::{BudgetTracker, BudgetRecord};`
  </action>
  <verify>
`cargo build -p openfang-reasoning -p openfang-memory --lib` clean.
Unit test: build a tracker on `MemorySubstrate::open_in_memory(0.0)`, record 3 BudgetRecords, assert `current_month_spent() == sum_of_costs` within `1e-9`.
  </verify>
  <done>
Table created in v9; BudgetTracker reads/writes; UTF-8-safe preview truncation.
  </done>
</task>

<task type="auto">
  <name>Task 3: Startup INFO log distinguishing default vs config-supplied</name>
  <files>crates/openfang-reasoning/src/budget.rs (or a small `boot.rs` in this crate)</files>
  <action>
Add `pub fn log_effective_reasoning_config(cfg: &openfang_types::config::ReasoningConfig, config_path: &Path)` to budget.rs. Inside, emit `tracing::info!`:
```
loaded reasoning config from {path}: max_input_tokens={n} {(from config)|(DEFAULT — no [reasoning] section found in config)} max_output_tokens=... ... monthly_budget_usd={x:.2} ...
```
The marker `(from config)` vs `(DEFAULT — no [reasoning] section found in config)` is selected by `cfg.is_default_loaded`. One emit per startup. Wire the call from kernel boot — search `crates/openfang-kernel/src/kernel.rs` for the boot path where KernelConfig is finalized and add the call there.

Tests:
- `default_config_marker` — build `ReasoningConfig::default()`, capture tracing output via `tracing_subscriber::fmt::Subscriber::builder().with_writer(...)` or simpler: just inspect the formatted string from a helper `format_effective_log(cfg) -> String`. Assert the string contains `(DEFAULT — no [reasoning] section found in config)` and `monthly_budget_usd=20.00`.
- `explicit_config_marker` — build with `is_default_loaded=false`, assert string contains `(from config)`.
- `typo_in_reasoning_block_rejects` — `toml::from_str::<KernelConfig>("[reasoning]\nmax_input_tkns = 30000\n")` returns Err.
  </action>
  <verify>
`cargo test -p openfang-reasoning budget log` runs the marker tests.
`cargo test -p openfang-types reasoning_config` runs the deny_unknown_fields test.
`cargo clippy --workspace --all-targets -- -D warnings` clean.
  </verify>
  <done>
Logger distinguishes default vs explicit; typo rejection green; tests pass.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (≥ 5 new tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Live (post-13): start daemon with no `[reasoning]` section; tail log; confirm `(DEFAULT — no [reasoning] section)` markers visible.
</verification>

<success_criteria>
- [ ] `ReasoningConfig` exists with `deny_unknown_fields` and the 8 fields.
- [ ] `KernelConfig.reasoning` populated; default values match MR-05.
- [ ] `reasoning_budget` table created in schema v9.
- [ ] `BudgetTracker.record` + `current_month_spent` work against real SQLite.
- [ ] Startup log marker distinguishes DEFAULT vs from-config.
- [ ] Typo in `[reasoning]` fails parse.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-12-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Boot wiring location for the INFO log
- Any follow-ups for later plans
</output>
