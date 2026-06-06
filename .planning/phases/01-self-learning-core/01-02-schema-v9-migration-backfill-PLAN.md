---
phase: 01-self-learning-core
plan: 02
type: execute
wave: 2
depends_on: [01-01]
files_modified:
  - crates/openfang-memory/src/migration.rs
autonomous: true
must_haves:
  truths:
    - "Schema version probes as 9 after migration; existing v8 databases auto-upgrade on first daemon boot (FTS-02)"
    - "v8→v9 migration on a populated DB preserves every existing `sessions.messages` BLOB byte-for-byte (FTS-03, FTS-05)"
    - "FTS5 `MATCH` against `session_messages_fts` returns hits for keywords known to be in backfilled session text (FTS-02, FTS-05)"
    - "A session with a corrupt msgpack BLOB is skipped with a WARN log; the migration completes for all other sessions (FTS-05)"
  artifacts:
    - "`const SCHEMA_VERSION: u32 = 9` in crates/openfang-memory/src/migration.rs"
    - "`fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error>`"
    - "`fn backfill_session_messages(conn: &Connection) -> Result<(), rusqlite::Error>`"
  key_links:
    - "session_messages_fts triggers fire on session_messages INSERT/UPDATE/DELETE so plan 01-03's dual-write auto-updates the index"
    - "Migration writes are best-effort per row — a single decode failure does NOT abort the migration"
---

<objective>
Bump schema to v9 and ship the flat `session_messages` companion table + external-content FTS5 index + sync triggers + best-effort backfill of every existing session BLOB. This makes future writes (plan 01-03) and reads (plan 01-04) searchable while leaving the canonical BLOB read path untouched.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-memory/src/migration.rs
@crates/openfang-memory/src/session_fts.rs
@crates/openfang-types/src/message.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Bump SCHEMA_VERSION and wire migrate_v9</name>
  <files>crates/openfang-memory/src/migration.rs</files>
  <action>
Edit `migration.rs:8` from `const SCHEMA_VERSION: u32 = 8;` to `= 9;`. Inside `run_migrations`, add `if current_version < 9 { migrate_v9(conn)?; }` after the existing v8 block (preserve order). Implement `fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error>` using the exact DDL block from addendum § A.2.1: `CREATE TABLE session_messages` (PK on (session_id, message_index), indexes on agent_id + session_id), `CREATE VIRTUAL TABLE session_messages_fts USING fts5(...)` (external-content with `content='session_messages'`, `content_rowid='rowid'`, `tokenize='porter unicode61 remove_diacritics 1'`), and the three triggers `session_messages_ai/ad/au`. Run as one `execute_batch` — DDL is atomic per the SQLite migration convention already used in this file. End by calling `backfill_session_messages(conn)?`.
  </action>
  <verify>
`cargo build -p openfang-memory --lib` clean. Existing migration tests (fresh-DB path) still pass: `cargo test -p openfang-memory migration`.
  </verify>
  <done>
SCHEMA_VERSION=9, migrate_v9 fn exists and is invoked from run_migrations, all existing migration tests green.
  </done>
</task>

<task type="auto">
  <name>Task 2: Implement best-effort backfill</name>
  <files>crates/openfang-memory/src/migration.rs</files>
  <action>
Implement `fn backfill_session_messages(conn: &Connection) -> Result<(), rusqlite::Error>` per addendum § A.2.1 second block: `SELECT id, agent_id, messages, updated_at FROM sessions`, then for each row `rmp_serde::from_slice::<Vec<Message>>(&blob)` and iterate. For each `Message`, compute `crate::session_fts::flatten_message_content(msg)`. Skip empty content; `INSERT OR IGNORE INTO session_messages` for every non-empty message. On any per-session decode failure, `tracing::warn!(session_id=%id, "v9 backfill: msgpack decode failed, skipping: {}", e)` and `continue` — do NOT propagate the error. On row-iteration errors, same WARN-and-continue. The whole backfill returns `Ok(())` even if every session fails (the schema is still upgraded). Wrap inserts in a transaction if it improves throughput on pc162 (see addendum § A.2.4) — `conn.transaction()`. NOTE: this connection comes from inside `run_migrations`; if `conn` is already inside a tx wrapper, use savepoints, otherwise plain insert is fine.
  </action>
  <verify>
`cargo build -p openfang-memory --lib` clean.
`cargo test -p openfang-memory migration` shows existing tests still pass.
  </verify>
  <done>
Backfill walks every session, populates session_messages via the flattener from 01-01, skips broken BLOBs with WARN.
  </done>
</task>

<task type="auto">
  <name>Task 3: v8→v9 transition test on populated DB + broken-BLOB skip test</name>
  <files>crates/openfang-memory/src/migration.rs</files>
  <action>
Add to the existing `#[cfg(test)] mod tests` block:
1. `fn migrate_v8_to_v9_preserves_sessions_and_backfills_fts` — open in-memory conn, replay migrations v1..v8 directly, INSERT two `sessions` rows whose `messages` BLOB is `rmp_serde::to_vec_named(&vec![Message::user("hello world"), Message::assistant("hi! how can I help with rust today?")])`, then call `migrate_v9(&conn)`. Assert: `SELECT COUNT(*) FROM session_messages WHERE session_id='s1'` returns 2; `SELECT COUNT(*) FROM session_messages_fts WHERE session_messages_fts MATCH 'rust'` returns ≥ 1; the original BLOB is byte-identical when read back. Pattern matches addendum § A.3 example.
2. `fn migrate_v8_to_v9_skips_broken_blob` — same setup but for session "s2" insert garbage bytes as the BLOB. After migrate_v9, assert s1 rows present, s2 rows absent, and migration returned `Ok(())`.
Use `tempfile::tempdir()` patterns from TESTING.md.
  </action>
  <verify>
`cargo test -p openfang-memory migrate_v8_to_v9` — both tests pass.
`cargo clippy -p openfang-memory --all-targets -- -D warnings` clean.
  </verify>
  <done>
Both transition tests green; addendum § A.3 verification gate satisfied.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (2+ new migration tests, all old tests still green).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `sqlite3 ~/.openfang/memory.db "SELECT user_version FROM pragma_user_version"` returns 9 after one daemon boot against a v8 DB.
</verification>

<success_criteria>
- [ ] SCHEMA_VERSION == 9 in source; `pragma_user_version == 9` after migration.
- [ ] `session_messages` and `session_messages_fts` tables exist post-migration (`.schema` shows them).
- [ ] Existing `sessions.messages` BLOBs unchanged (byte-equal pre/post).
- [ ] FTS5 MATCH returns hits for known terms backfilled from real session data.
- [ ] Broken-BLOB session skipped with WARN; migration still returns Ok.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-02-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Any decisions made during execution that weren't pinned in this plan
- Any follow-ups for later plans
</output>
