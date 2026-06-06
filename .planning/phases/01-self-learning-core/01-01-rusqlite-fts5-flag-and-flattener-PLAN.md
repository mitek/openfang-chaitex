---
phase: 01-self-learning-core
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - crates/openfang-memory/src/lib.rs
files_created:
  - crates/openfang-memory/src/session_fts.rs
autonomous: true
must_haves:
  truths:
    - "Workspace builds with FTS5 enabled: `PRAGMA compile_options` reports `ENABLE_FTS5` at runtime (FTS-01)"
    - "Every `Message` variant produces non-empty indexable text — a session containing only tool_use or image blocks still appears in future search (FTS-02)"
    - "Flattener output is stable: re-running it on the same `Message` yields byte-identical strings (FTS-02)"
  artifacts:
    - "crates/openfang-memory/src/session_fts.rs"
    - "`pub fn flatten_message_content(msg: &Message) -> String`"
    - "`pub fn role_string(role: &Role) -> &'static str`"
  key_links:
    - "session_fts module re-exported from openfang-memory::lib so plan 01-02 backfill and plan 01-03 dual-write can call it"
---

<objective>
Enable SQLite FTS5 in the workspace and ship the deterministic `Message → searchable text` flattener that every downstream plan (v9 migration backfill, SessionStore dual-write, session_search results) will rely on. This is the smallest plan in the phase and unblocks the entire FTS5 track.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@Cargo.toml
@crates/openfang-types/src/message.rs
@crates/openfang-memory/src/lib.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add `fts5` to rusqlite features</name>
  <files>Cargo.toml</files>
  <action>
Edit `Cargo.toml` line 58 (per CONTEXT.md anchor) — extend the rusqlite feature list:
`rusqlite = { version = "0.31", features = ["bundled", "serde_json", "fts5"] }`.
Do not change the version; do not move the line. The bundled SQLite source rebuilds with FTS5 — no system packages needed. See addendum § A.1.
  </action>
  <verify>
`cargo build --workspace --lib` succeeds. Spot-check FTS5 is linked:
`cargo test -p openfang-memory --lib -- --nocapture` (existing migration tests must still pass).
  </verify>
  <done>
rusqlite features include `"fts5"`; workspace builds clean; no test regressions.
  </done>
</task>

<task type="auto">
  <name>Task 2: Create `session_fts` module with stable flattener</name>
  <files>crates/openfang-memory/src/session_fts.rs, crates/openfang-memory/src/lib.rs</files>
  <action>
Create `crates/openfang-memory/src/session_fts.rs`. Implement:
- `pub fn flatten_message_content(msg: &Message) -> String` — handles both `MessageContent::Text(s)` (verbatim) and `MessageContent::Blocks(Vec<ContentBlock>)`. For Blocks, join with `'\n'` and dispatch every variant of `ContentBlock` enumerated in `crates/openfang-types/src/message.rs:86-167`: `Text {text, ..}` → text; `ToolUse {name, input, ..}` → `[tool_use:NAME] {json}` where json is `serde_json::to_string(input)` truncated to 2048 bytes via `str_utils::safe_truncate_str` (UTF-8 safe); `ToolResult {content, ..}` → `[tool_result] {content}`; `Image {..}` → `"[image]"`; `Thinking {thinking, ..}` → `[thinking] {thinking}`; `RedactedThinking {..}` → `"[redacted_thinking]"`; `Unknown` → `"[unknown_block]"`. NEVER return `Some("")` for a non-empty message (addendum § A.2.2 invariant).
- `pub fn role_string(role: &Role) -> &'static str` — map `Role::User|Assistant|System|Tool` to lowercase string literals.
Register the module in `crates/openfang-memory/src/lib.rs` with `pub mod session_fts;` so plans 01-02 and 01-03 can import it. Use the str_utils crate from openfang-runtime ONLY if it is already a memory dep; otherwise inline a safe UTF-8 truncate (find a char boundary at-or-before 2048).
  </action>
  <verify>
`cargo build -p openfang-memory --lib` clean.
`cargo test -p openfang-memory session_fts -- --nocapture` runs the new tests.
  </verify>
  <done>
File exists with both pub fns. Module re-exported from lib.rs. Unit tests cover every ContentBlock arm + plain Text + empty Blocks vec.
  </done>
</task>

<task type="auto">
  <name>Task 3: Unit tests per variant</name>
  <files>crates/openfang-memory/src/session_fts.rs</files>
  <action>
Add `#[cfg(test)] mod tests` at the bottom of `session_fts.rs`. One `#[test] fn flatten_<variant>` per ContentBlock arm. Each constructs a minimal `Message` (use `Message::user`, `Message::assistant_with_blocks` from message.rs:256-289) and asserts the flattened output begins with the expected marker. Add a `flatten_tool_use_truncates_2kb` test that feeds a 5KB JSON input and asserts `output.len() <= 2048 + len("[tool_use:NAME] ")`. Add `role_string_lowercase` test for all four Role variants. Tests must be deterministic (no random JSON ordering).
  </action>
  <verify>
`cargo test -p openfang-memory session_fts` shows all new tests passing.
`cargo clippy -p openfang-memory --all-targets -- -D warnings` clean.
  </verify>
  <done>
≥ 8 tests in `mod tests`; all pass; clippy clean.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (~8 new tests added).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Runtime probe (manual): start daemon, attach to memory DB, run `SELECT * FROM pragma_compile_options WHERE compile_options LIKE 'ENABLE_FTS5';` — must return one row.
</verification>

<success_criteria>
- [ ] FTS5 module compiled in: `PRAGMA compile_options` returns `ENABLE_FTS5`.
- [ ] `flatten_message_content` exists and handles every `ContentBlock` variant from `message.rs:86-167`.
- [ ] `role_string` covers all `Role` variants.
- [ ] No `Message` with at least one block flattens to empty string.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-01-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Any decisions made during execution that weren't pinned in this plan
- Any follow-ups for later plans
</output>
