# Plan 01-03 — SessionStore dual-write — SUMMARY

**One-liner.** `SessionStore::save_session` now transactionally writes both the msgpack BLOB AND the flat `session_messages` rows so the v9 FTS5 index reflects the latest conversation; delete paths cascade.

## What was implemented

- `save_session` body rewritten to acquire the mutex once, open a `rusqlite::Transaction`, run the existing `sessions` BLOB upsert, then `DELETE FROM session_messages WHERE session_id = ?` and re-insert one row per message using `session_fts::flatten_message_content` + `session_fts::role_string`. Empty-flattened messages are skipped (nothing to index). Single `tx.commit()` at the end.
- `delete_session` now also runs `DELETE FROM session_messages WHERE session_id = ?` on the same connection guard; FTS5 trigger `session_messages_ad` drains the index automatically.
- `delete_agent_sessions` mirror cascade by `agent_id`.
- 4 new tests cover: populate flat table + FTS hit, resave replaces flat rows without duplicates and removes stale FTS terms, session delete cascade, agent delete cascade.

## Files changed

- `crates/openfang-memory/src/session.rs`

## Tests added (4)

- `save_session_populates_flat_table_and_fts`
- `save_session_replaces_flat_rows_on_resave`
- `delete_session_cascades_to_flat_and_fts`
- `delete_agent_sessions_cascades`

Test helpers `count_flat_rows`, `count_flat_rows_for_agent`, `count_fts_hits` added to the test module (private to `mod tests`).

## Decisions made during execution

- The `setup()` test helper already uses `Connection::open_in_memory()` + `run_migrations`, equivalent to `MemorySubstrate::open_in_memory()` semantically; new tests reuse the existing helper rather than introducing a second flavor.
- Empty-flattened messages (e.g. assistant turns with `MessageContent::Blocks(vec![])`) are skipped from the flat-table insert. They would index as empty content and serve no search purpose; the BLOB still preserves them for the read path.
- The single mutex acquisition + `conn.transaction()` shape avoids the toctou/deadlock risk called out in commit `5447bf7` — only one lock per call.

## Deviations vs plan

None — all 3 tasks landed verbatim per `<action>`.

## Follow-ups for W4

- Plan 01-04 will lift FTS5 search SQL into `SessionStore::search_sessions_fts`; the helpers in this test module read directly from `session_messages`/`session_messages_fts` and will continue to work without change.
- Live integration check (`sqlite3 ~/.openfang/memory.db "SELECT COUNT(*) FROM session_messages_fts"` after a few agent messages) is deferred to plan 01-04 / 01-16 once the search tool is wired.

## Commits

- `327448d` — feat(01-03): SessionStore.save_session dual-writes session_messages
