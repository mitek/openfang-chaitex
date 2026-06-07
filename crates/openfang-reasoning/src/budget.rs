//! Persistent budget tracking for reasoning calls.
//!
//! Every reasoning call records one row in the `reasoning_budget` SQLite
//! table (created as part of the v9 migration's amendment in plan 01-02
//! — see `migrate_v9` in `openfang-memory/src/migration.rs`). The tracker
//! aggregates the calendar-month spend so the engine can clamp / downgrade
//! per `[reasoning].budget_exceeded_action`.
//!
//! Privacy: `query_preview` is truncated to ≤100 chars (UTF-8-safe). This
//! prevents secrets pasted into queries from being persisted at full
//! length while still letting an operator audit what was spent on what.

use crate::{ReasoningError, ReasoningLevel};
use chrono::{Datelike, Utc};
use openfang_memory::MemorySubstrate;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Maximum bytes in a persisted `query_preview` (MR-05 privacy clamp).
const QUERY_PREVIEW_MAX_BYTES: usize = 100;

/// One row in the `reasoning_budget` table.
///
/// `timestamp` is RFC3339 (UTC). `level` round-trips via serde — the
/// table stores the lowercase string form so it matches the wire format
/// from `ReasoningLevel`'s `Serialize` impl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetRecord {
    /// RFC3339 timestamp the call started.
    pub timestamp: String,
    /// Level actually run (after any downgrade).
    pub level: ReasoningLevel,
    /// Input tokens consumed.
    pub input_tokens: u64,
    /// Output tokens produced.
    pub output_tokens: u64,
    /// Pre-call cost estimate (USD).
    pub estimated_cost_usd: f64,
    /// First ≤100 chars of the query for audit (MR-05 privacy clamp).
    pub query_preview: String,
}

impl BudgetRecord {
    /// Build a `BudgetRecord` for a call right now, applying the
    /// UTF-8-safe ≤100-byte clamp to `query_preview` so callers don't
    /// have to remember it.
    pub fn new_now(
        level: ReasoningLevel,
        input_tokens: u64,
        output_tokens: u64,
        estimated_cost_usd: f64,
        query: &str,
    ) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            level,
            input_tokens,
            output_tokens,
            estimated_cost_usd,
            query_preview: safe_truncate(query, QUERY_PREVIEW_MAX_BYTES),
        }
    }
}

/// Persistent budget tracker — one per process.
pub struct BudgetTracker {
    memory: Arc<MemorySubstrate>,
    /// Monthly USD cap; mirrored from `ReasoningConfig.monthly_budget_usd`
    /// at boot so the tracker can answer "is `current_month_spent` over the
    /// cap?" without re-reading config on every call.
    monthly_budget_usd: f64,
}

impl BudgetTracker {
    /// Build a tracker bound to the shared memory substrate.
    ///
    /// `monthly_budget_usd` is the ceiling from `[reasoning]`; the tracker
    /// itself doesn't enforce it (the engine does, per
    /// `budget_exceeded_action`). It's stored here so the public
    /// `monthly_budget_usd()` accessor can render it in dashboard / log
    /// output without re-reading config.
    pub fn new(memory: Arc<MemorySubstrate>, monthly_budget_usd: f64) -> Self {
        Self {
            memory,
            monthly_budget_usd,
        }
    }

    /// Monthly budget ceiling in USD.
    pub fn monthly_budget_usd(&self) -> f64 {
        self.monthly_budget_usd
    }

    /// Persist one reasoning call. INSERTs into `reasoning_budget`.
    pub fn record(&self, rec: BudgetRecord) -> Result<(), ReasoningError> {
        let conn = self.memory.usage_conn();
        let guard = conn
            .lock()
            .map_err(|e| ReasoningError::Memory(format!("conn lock poisoned: {e}")))?;
        guard
            .execute(
                "INSERT INTO reasoning_budget
                 (timestamp, level, input_tokens, output_tokens, estimated_cost_usd, query_preview)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    rec.timestamp,
                    level_to_str(rec.level),
                    rec.input_tokens as i64,
                    rec.output_tokens as i64,
                    rec.estimated_cost_usd,
                    rec.query_preview,
                ],
            )
            .map_err(|e| ReasoningError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Sum of `estimated_cost_usd` for the current calendar month (UTC).
    ///
    /// The query uses string-range comparison on the ISO-8601 timestamp
    /// so it works without a dedicated date column. The cutoff string is
    /// `YYYY-MM-01T00:00:00+00:00`.
    pub fn current_month_spent(&self) -> Result<f64, ReasoningError> {
        let now = Utc::now();
        let cutoff = format!(
            "{:04}-{:02}-01T00:00:00+00:00",
            now.year(),
            now.month()
        );
        let conn = self.memory.usage_conn();
        let guard = conn
            .lock()
            .map_err(|e| ReasoningError::Memory(format!("conn lock poisoned: {e}")))?;
        let spent: f64 = guard
            .query_row(
                "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
                 FROM reasoning_budget
                 WHERE timestamp >= ?1",
                rusqlite::params![cutoff],
                |r| r.get(0),
            )
            .map_err(|e| ReasoningError::Memory(e.to_string()))?;
        Ok(spent)
    }
}

/// Map `ReasoningLevel` to the lowercase string form stored in the table.
/// Mirrors the wire format produced by the `Serialize` impl so the JSON
/// representation (`"medium"`, etc.) matches what's on disk.
fn level_to_str(level: ReasoningLevel) -> &'static str {
    match level {
        ReasoningLevel::Minimal => "minimal",
        ReasoningLevel::Low => "low",
        ReasoningLevel::Medium => "medium",
        ReasoningLevel::High => "high",
        ReasoningLevel::Max => "max",
    }
}

/// UTF-8-safe truncate to ≤`max_bytes`. If `s.len() <= max_bytes` returns
/// `s` unchanged; otherwise walks back to the nearest char boundary and
/// slices there. Never panics on multibyte codepoints.
fn safe_truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut cut = max_bytes;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_string()
}

/// Render the effective `[reasoning]` config as a single human-readable
/// line.
///
/// The `(from config)` vs `(DEFAULT — no [reasoning] section found in
/// config)` marker is selected by `cfg.is_default_loaded` — when `true`
/// the config came from `Default::default()` because the TOML had no
/// `[reasoning]` section. This matches the addendum § C.2 contract.
///
/// `config_path` is included in the line so operators reading the log
/// know which file produced the values.
pub fn format_effective_log(
    cfg: &openfang_types::config::ReasoningConfig,
    config_path: &Path,
) -> String {
    let marker = if cfg.is_default_loaded {
        "(DEFAULT — no [reasoning] section found in config)"
    } else {
        "(from config)"
    };
    format!(
        "loaded reasoning config from {path}: \
         max_input_tokens={mi} {marker} \
         max_output_tokens={mo} {marker} \
         max_level={ml} {marker} \
         monthly_budget_usd={mb:.2} {marker} \
         budget_exceeded_action={ba} {marker} \
         require_approval_for_max={ra} {marker} \
         auto_update_profile={ap} {marker} \
         fts_backfill={fb} {marker}",
        path = config_path.display(),
        mi = cfg.max_input_tokens,
        mo = cfg.max_output_tokens,
        ml = cfg.max_level,
        mb = cfg.monthly_budget_usd,
        ba = cfg.budget_exceeded_action,
        ra = cfg.require_approval_for_max,
        ap = cfg.auto_update_profile,
        fb = cfg.fts_backfill,
        marker = marker,
    )
}

/// Emit the effective `[reasoning]` config as one `tracing::info!` line.
/// Call once at kernel boot. See `format_effective_log` for the format.
pub fn log_effective_reasoning_config(
    cfg: &openfang_types::config::ReasoningConfig,
    config_path: &Path,
) {
    tracing::info!("{}", format_effective_log(cfg, config_path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::config::ReasoningConfig;
    use std::path::PathBuf;

    fn fresh_memory() -> Arc<MemorySubstrate> {
        Arc::new(
            MemorySubstrate::open_in_memory(0.0).expect("in-memory MemorySubstrate must open"),
        )
    }

    #[test]
    fn budget_record_clamps_query_preview() {
        let long = "a".repeat(500);
        let rec = BudgetRecord::new_now(ReasoningLevel::Low, 0, 0, 0.0, &long);
        assert_eq!(rec.query_preview.len(), 100);
    }

    #[test]
    fn safe_truncate_respects_char_boundaries() {
        // "héllo" — the 'é' is 2 bytes (0xC3, 0xA9). Cutting at byte 2
        // would land in the middle. Walk back to byte 1.
        let s = "héllo";
        let cut = safe_truncate(s, 2);
        assert_eq!(cut, "h");
        // Cutting exactly at end is fine.
        assert_eq!(safe_truncate("abc", 3), "abc");
        // Cutting past end returns unchanged.
        assert_eq!(safe_truncate("abc", 99), "abc");
    }

    #[test]
    fn record_and_aggregate_round_trip() {
        // 3 records → sum equals their cost sum within 1e-9.
        let mem = fresh_memory();
        let tracker = BudgetTracker::new(mem, 50.0);
        let recs = [
            BudgetRecord::new_now(ReasoningLevel::Low, 10, 5, 0.001, "q1"),
            BudgetRecord::new_now(ReasoningLevel::Medium, 100, 50, 0.025, "q2"),
            BudgetRecord::new_now(ReasoningLevel::High, 1000, 200, 0.5, "q3"),
        ];
        let expected: f64 = recs.iter().map(|r| r.estimated_cost_usd).sum();
        for r in recs {
            tracker.record(r).expect("record");
        }
        let spent = tracker.current_month_spent().expect("aggregate");
        assert!(
            (spent - expected).abs() < 1e-9,
            "spent={spent} expected={expected}"
        );
    }

    #[test]
    fn monthly_budget_usd_accessor_returns_constructor_value() {
        let tracker = BudgetTracker::new(fresh_memory(), 12.34);
        assert!((tracker.monthly_budget_usd() - 12.34).abs() < 1e-9);
    }

    #[test]
    fn format_effective_log_default_marker() {
        // Default::default() sets is_default_loaded=true.
        let cfg = ReasoningConfig::default();
        let s = format_effective_log(&cfg, &PathBuf::from("/tmp/x.toml"));
        assert!(
            s.contains("(DEFAULT — no [reasoning] section found in config)"),
            "got: {s}"
        );
        assert!(s.contains("monthly_budget_usd=20.00"), "got: {s}");
        assert!(s.contains("max_input_tokens=40000"), "got: {s}");
        assert!(s.contains("max_level=high"), "got: {s}");
    }

    #[test]
    fn format_effective_log_from_config_marker() {
        // Mark as explicitly loaded.
        let cfg = ReasoningConfig {
            is_default_loaded: false,
            ..ReasoningConfig::default()
        };
        let s = format_effective_log(&cfg, &PathBuf::from("/etc/openfang.toml"));
        assert!(s.contains("(from config)"), "got: {s}");
        // The DEFAULT marker must NOT appear.
        assert!(
            !s.contains("(DEFAULT"),
            "DEFAULT marker leaked into from-config output: {s}"
        );
    }
}
