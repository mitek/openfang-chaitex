# 01-02 — schema v9 migration + backfill — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** 2d20a5e, 8668e74

## One-liner

Bumped `SCHEMA_VERSION` to 9; added `migrate_v9` (flat `session_messages`
table + external-content FTS5 `session_messages_fts` + ai/ad/au triggers,
DDL exactly per addendum § A.2.1) and a best-effort
`backfill_session_messages` that decodes existing msgpack BLOBs via
`rmp_serde` and flattens via `session_fts::flatten_message_content`.

## Files changed

- `crates/openfang-memory/src/migration.rs` — `SCHEMA_VERSION = 9`,
  `migrate_v9`, `backfill_session_messages`, run_migrations wired with
  `if current_version < 9`, two new v8→v9 transition tests + two test
  helpers (`open_at_v8`, `insert_session_row`).

## Tests added

2 new tests in `migration::tests` (total in module now 4):

1. `migrate_v8_to_v9_preserves_sessions_and_backfills_fts` — seeds two
   sessions with real `Vec<Message>` msgpack BLOBs, runs `migrate_v9`,
   asserts both backfilled (2 rows each), FTS `MATCH 'rust'` returns ≥1
   hit, and the canonical `sessions.messages` BLOBs are byte-equal pre
   vs post.
2. `migrate_v8_to_v9_skips_broken_blob` — seeds one valid + one garbage
   BLOB, asserts `migrate_v9` returns `Ok`, valid session backfilled,
   broken session absent from `session_messages`, and the broken BLOB
   is untouched in `sessions.messages`.

Workspace gates clean: 2746 tests pass; clippy `-D warnings` clean;
build clean.

## Deviations

None. Plan executed as written. `MemorySubstrate::open_in_memory(...)`
already exists in `substrate.rs:107` so no Rule-3 fix was needed.

## Decisions made during execution

1. **Backfill uses a prepared `INSERT OR IGNORE` statement** that is
   reused across all rows. Slightly more efficient than `conn.execute`
   per row; safer than wrapping in an explicit transaction, since the
   migration is itself frequently called from inside a transaction
   wrapper at the substrate boot level (see addendum § A.2.4 note).
2. **Per-row insert failures are also WARN-and-continue.** The plan
   explicitly covered per-session decode failures; I extended the same
   policy to insert failures so a malformed row (e.g. unexpected NULL)
   never aborts the upgrade. Same WARN format.
3. **`set_schema_version(&conn, 8)`** call inside `open_at_v8` test
   helper — necessary because `run_migrations` reads `user_version` to
   decide which migrations to skip. Pinning it at 8 lets us call
   `migrate_v9` in isolation and reproduce the upgrade-from-v8 codepath
   the actual daemon takes.
4. **`migrations` row inserted by `migrate_v9`** — matches the pattern
   set by every other migrate_vN. The user_version pragma is the
   authoritative source; the `migrations` table is the audit trail.

## Follow-ups for later plans

- **01-03 (SessionStore dual-write):** the triggers in v9 keep
  `session_messages_fts` in lockstep with `session_messages`, so the
  dual-write code only needs to write `session_messages` itself — it
  must NOT INSERT into `session_messages_fts` directly. Use
  delete-then-insert per session per addendum § A.2.3.
- **01-12 (BudgetTracker):** plan 01-12 adds the `reasoning_budget`
  table as an amendment INSIDE `migrate_v9`'s `execute_batch` (kept
  inside the v9 boundary so a v8 → v9 upgrade is single-transaction).
  Anchor comment: `// === v9 amendment: reasoning_budget (plan 01-12) ===`.
- **01-04 (session_search tool):** FTS5 query uses `session_messages_fts
  MATCH ?1`; join back to `session_messages` by `rowid` for the original
  text + session/agent metadata. The triggers ensure rowid stays in
  sync.
