# 01-01 — rusqlite fts5 flag and flattener — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** 15d6a1a

## One-liner

Shipped the deterministic `flatten_message_content` / `role_string` helpers in
a new `openfang-memory::session_fts` module, with a runtime FTS5 probe that
confirms the bundled SQLite already exposes `ENABLE_FTS5` — so the FTS-01
requirement is satisfied without a Cargo.toml change.

## Files changed

- `crates/openfang-memory/src/lib.rs` — added `pub mod session_fts;`
- `crates/openfang-memory/src/session_fts.rs` — NEW, 280 lines, 15 tests

`Cargo.toml` is **unchanged** (see Deviations).

## Tests added

15 unit tests in `session_fts::tests`:

1. `flatten_plain_text_returns_verbatim`
2. `flatten_text_block`
3. `flatten_tool_use_block`
4. `flatten_tool_result_block`
5. `flatten_image_block_uses_marker`
6. `flatten_thinking_block`
7. `flatten_redacted_thinking_block_uses_marker`
8. `flatten_unknown_block_uses_marker`
9. `flatten_multiple_blocks_joined_by_newline`
10. `flatten_empty_blocks_vec_returns_empty`
11. `flatten_tool_use_truncates_large_json`
12. `flatten_is_deterministic_across_runs`
13. `flatten_tool_use_respects_utf8_boundary_when_truncating`
14. `role_string_lowercase`
15. `fts5_is_compiled_in` — the **runtime FTS5 capability probe**

All 15 pass. Workspace gates (build / test / clippy `-D warnings`) clean.

## Deviations

**[Rule 3 — blocking compile]** The plan's Task 1 said "extend rusqlite
feature list with `fts5`". Attempted; build failed:

```
package `openfang-memory` depends on `rusqlite` with feature `fts5` but
`rusqlite` does not have that feature.
```

`rusqlite 0.31` (the version pinned workspace-wide) does **not** expose an
`fts5` top-level feature. That feature only exists in `rusqlite >= 0.32`,
where it gates Rust-side custom-tokenizer wrappers — **not** the FTS5
virtual-table itself.

The FTS5 virtual-table is enabled at the C/amalgamation layer by
`libsqlite3-sys`' bundled build (`build.rs` always passes
`-DSQLITE_ENABLE_FTS5`). So FTS-01's actual measurable requirement —
"`PRAGMA compile_options` reports `ENABLE_FTS5` at runtime" — is **already
satisfied** by the current `rusqlite = { features = ["bundled", ...] }`
setting.

**Resolution:**
- Reverted `Cargo.toml` to its pre-plan state (no `fts5` feature).
- Added `fts5_is_compiled_in` runtime test that calls `pragma_compile_options`
  AND tries `CREATE VIRTUAL TABLE … USING fts5(content)` — both succeed.

If a later plan needs the rusqlite custom-tokenizer Rust API, that requires a
`rusqlite` major-version bump (0.31 → ≥0.32), which is a separate cross-cutting
change with its own blast-radius review. None of plans 01-02/03/04 need the
Rust tokenizer API — they all run raw SQL.

## Decisions made during execution

1. **`Role::Tool` does not exist** in `openfang-types`. The plan's `role_string`
   spec listed `Role::User|Assistant|System|Tool`, but the actual enum has only
   three variants. Tool results are carried as a `ContentBlock::ToolResult`
   inside a user-role message. `role_string` covers the three real variants and
   has a comment noting where to add a fourth arm if `Role::Tool` is ever
   introduced.
2. **No external `str_utils` dep added.** `openfang-runtime`'s `str_utils` is
   not currently a `openfang-memory` dep. Inlined a six-line `safe_truncate`
   helper in `session_fts.rs` so the crate stays dependency-disjoint.
3. **JSON determinism.** `serde_json::Value::Object` is backed by a `BTreeMap`
   when the `preserve_order` feature is OFF — which it is in this workspace.
   So `serde_json::to_string(input)` is byte-stable across runs for the same
   `Value`. The `flatten_is_deterministic_across_runs` test pins this.

## Follow-ups for later plans

- **01-02 (schema v9 migration + backfill):** The `flatten_message_content`
  helper is `pub use openfang_memory::session_fts::flatten_message_content`.
  Backfill should iterate sessions, decode the BLOB with `rmp_serde::from_slice`,
  flatten each `Message`, and insert one row per message into the new flat
  `session_messages` table.
- **01-03 (SessionStore dual-write):** `save_session` will need to call
  `flatten_message_content` for each new message and rewrite the flat table
  inside the same transaction. The function is cheap and re-entrant.
- **01-04 (`session_search` tool):** Snippet/BM25 results from
  `session_messages_fts` can be joined back to `session_messages` for context
  and the matching `Message` (by `(session_id, message_index)`).
- **Future rusqlite bump:** if any plan wants the Rust tokenizer API, document
  the upgrade path. Until then, raw SQL FTS5 is fully accessible via the
  current bundled amalgamation.
