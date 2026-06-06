---
phase: 01-self-learning-core
plan: 03
type: execute
wave: 3
depends_on: [01-02]
files_modified:
  - crates/openfang-memory/src/session.rs
autonomous: true
must_haves:
  truths:
    - "After `save_session`, querying `session_messages_fts` for any term in the saved messages returns at least one matching row (FTS-03)"
    - "After `delete_session`, no rows for that session_id remain in `session_messages` and the FTS5 index reflects the deletion (FTS-03)"
    - "After `delete_agent_sessions`, every row for that agent_id is removed from `session_messages` (FTS-03)"
    - "`get_session` still reads from the BLOB column — read latency is unchanged (FTS-03)"
  artifacts:
    - "`save_session` body containing a `conn.transaction()` that writes the BLOB AND rewrites `session_messages` rows for the session"
    - "`delete_session` and `delete_agent_sessions` cascade to `session_messages`"
  key_links:
    - "Uses `session_fts::flatten_message_content` and `role_string` from plan 01-01 — do NOT duplicate flattening logic"
    - "FTS5 index updates happen automatically via the triggers from plan 01-02 — do NOT write to `session_messages_fts` directly"
---

<objective>
Make every session write produce an up-to-date FTS5 index by adding a transactional dual-write to `SessionStore::save_session` and cascading deletes from `delete_session` / `delete_agent_sessions` into `session_messages`. The msgpack BLOB read path stays unchanged so existing performance characteristics are preserved.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-memory/src/session.rs
@crates/openfang-memory/src/session_fts.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Transactional dual-write in save_session</name>
  <files>crates/openfang-memory/src/session.rs</files>
  <action>
Rewrite the body of `pub fn save_session(&self, session: &Session) -> OpenFangResult<()>` at line 78. Acquire the mutex once, then `conn.transaction()` and use the `Transaction` for both writes — DO NOT nest lock acquisition or call `self.conn.lock()` twice in the same fn (toctou + deadlock risk noted in recent commit 5447bf7). Inside the tx:
1. Existing BLOB INSERT-OR-UPDATE on `sessions` (preserve the current SQL verbatim — just route it through `tx.execute` instead of `conn.execute`).
2. `tx.execute("DELETE FROM session_messages WHERE session_id = ?1", [session_id_str])`.
3. Prepare `INSERT INTO session_messages (session_id, agent_id, message_index, role, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)` once, loop over `session.messages.iter().enumerate()`. For each message compute `crate::session_fts::flatten_message_content(msg)` — skip if empty — and `crate::session_fts::role_string(&msg.role)`. Use `now` (the same `chrono::Utc::now().to_rfc3339()` used for the BLOB row) as the timestamp on every flat row.
4. `drop(stmt); tx.commit()`.
Pattern matches addendum § A.2.3 verbatim.
  </action>
  <verify>
`cargo build -p openfang-memory --lib` clean.
`cargo test -p openfang-memory session` — existing tests must still pass.
  </verify>
  <done>
save_session writes both stores atomically via one transaction; uses the plan 01-01 flattener.
  </done>
</task>

<task type="auto">
  <name>Task 2: Cascade deletes</name>
  <files>crates/openfang-memory/src/session.rs</files>
  <action>
In `delete_session` (line 104) and `delete_agent_sessions` (line 118), after the existing `DELETE FROM sessions` SQL, add a `DELETE FROM session_messages WHERE session_id = ?1` (or `agent_id = ?1` for the agent variant). Use the same connection guard / transaction the existing code uses — do not introduce a new lock acquisition. The FTS5 `session_messages_ad` trigger from plan 01-02 fires automatically to keep the index in sync, so NO direct write to `session_messages_fts` from here.
  </action>
  <verify>
`cargo build -p openfang-memory --lib` clean.
`cargo test -p openfang-memory delete` runs the existing delete tests.
  </verify>
  <done>
Both delete paths cascade to session_messages; FTS index drains via trigger.
  </done>
</task>

<task type="auto">
  <name>Task 3: Dual-write + cascade unit tests</name>
  <files>crates/openfang-memory/src/session.rs</files>
  <action>
Add to the existing `#[cfg(test)] mod tests` in session.rs (or create one if absent):
1. `fn save_session_populates_flat_table_and_fts` — open `MemorySubstrate::open_in_memory()` (per TESTING.md), create one `Session` with messages mentioning "ferrocene", call `save_session`, assert via `SELECT COUNT(*) FROM session_messages WHERE session_id = ?` ≥ 1 and `SELECT COUNT(*) FROM session_messages_fts WHERE session_messages_fts MATCH 'ferrocene'` ≥ 1.
2. `fn save_session_replaces_flat_rows_on_resave` — save the same session twice with different message content; assert no duplicate rows; FTS finds the new term but not the old one.
3. `fn delete_session_cascades_to_flat_and_fts` — save, then `delete_session`; assert zero rows in `session_messages` and zero FTS hits.
4. `fn delete_agent_sessions_cascades` — save two sessions for the same agent, call `delete_agent_sessions`, assert both gone from the flat table.
Use `tempfile::tempdir()`/`open_in_memory` patterns per TESTING.md.
  </action>
  <verify>
`cargo test -p openfang-memory save_session_ delete_session_ delete_agent_` — all four pass.
`cargo clippy -p openfang-memory --all-targets -- -D warnings` clean.
  </verify>
  <done>
4+ new tests proving the dual-write + cascade invariants.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (4+ new tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Live (best done after plan 01-04): start daemon, send agent a message, query `sqlite3 ~/.openfang/memory.db "SELECT COUNT(*) FROM session_messages_fts"` — counter increases.
</verification>

<success_criteria>
- [ ] save_session uses one transaction for BLOB + flat writes.
- [ ] session_messages reflects the in-memory `session.messages` exactly after each save.
- [ ] delete_session and delete_agent_sessions cascade to session_messages.
- [ ] No regressions in existing session tests.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-03-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Any decisions made during execution that weren't pinned in this plan
- Any follow-ups for later plans
</output>
