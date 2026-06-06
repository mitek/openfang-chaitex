---
phase: 01-self-learning-core
plan: 04
type: execute
wave: 4
depends_on: [01-03]
files_modified:
  - crates/openfang-runtime/src/tool_runner.rs
  - crates/openfang-memory/src/session.rs
autonomous: true
must_haves:
  truths:
    - "Agent calls `session_search(\"<term>\")` and receives a JSON array of ranked hits with `session_id`, `agent_id`, `role`, `timestamp`, `snippet`, `score` (FTS-04)"
    - "Search returns < 500ms on pc162 against an agent's typical history (FTS-04 latency criterion)"
    - "`agent_id` filter restricts results to that agent's sessions only (FTS-04)"
    - "Tool is registered in both the dispatch match arm AND the schema list (X-01)"
  artifacts:
    - "`pub fn search_sessions_fts(&self, query: &str, limit: usize, agent_id: Option<AgentId>) -> OpenFangResult<Vec<SessionSearchHit>>` on SessionStore"
    - "`async fn tool_session_search(input: serde_json::Value, kernel: &dyn KernelHandle) -> Result<String, String>` in tool_runner.rs"
  key_links:
    - "Anchor comment `// === PHASE 1 PLAN 01-04 session_search ===` ... `// === END PHASE 1 PLAN 01-04 ===` wraps every addition to tool_runner.rs so parallel-wave plans 01-09/01-13/01-14 do not merge-conflict"
    - "Read-only — no capability flag required; defaults available to every agent"
---

<objective>
Expose FTS5 session search to agents as the new built-in tool `session_search`. This is the first user-visible deliverable of the FTS5 track and feeds plan 01-11's `ReasoningEngine::Low+` fact retrieval.
</objective>

<context>
@.planning/phases/01-self-learning-core/01-CONTEXT.md
@docs/chaitex/phase1-self-learning-architecture.md
@docs/chaitex/phase1-addendum-codebase-grounding.md
@crates/openfang-runtime/src/tool_runner.rs
@crates/openfang-memory/src/session.rs
@crates/openfang-memory/src/substrate.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: search_sessions_fts helper on SessionStore</name>
  <files>crates/openfang-memory/src/session.rs</files>
  <action>
Add `pub struct SessionSearchHit { pub session_id: String, pub agent_id: String, pub role: String, pub timestamp: String, pub snippet: String, pub score: f64 }` with `#[derive(Debug, Clone, Serialize, Deserialize)]`. Add `pub fn search_sessions_fts(&self, query: &str, limit: usize, agent_id: Option<&AgentId>) -> OpenFangResult<Vec<SessionSearchHit>>` to `impl SessionStore`. SQL:
```
SELECT m.session_id, m.agent_id, m.role, m.timestamp,
       snippet(session_messages_fts, 0, '<b>', '</b>', '...', 32) AS snippet,
       bm25(session_messages_fts) AS score
FROM session_messages_fts
JOIN session_messages m ON m.rowid = session_messages_fts.rowid
WHERE session_messages_fts MATCH ?1
  [AND m.agent_id = ?2]
ORDER BY score
LIMIT ?N
```
bm25 returns lower=better; the agent_id branch is appended only when `Some`. Use rusqlite `query_map`. Cap `limit` server-side to 50 (sanity). Lock duration must be minimal — do NOT hold the mutex across `.await` (per CONVENTIONS.md and the `!Send` guard rule from CONTEXT.md anchor `registry.rs:55`).
  </action>
  <verify>
`cargo build -p openfang-memory --lib` clean. Add a unit test in `mod tests`: save a session with "octopus" in a message, call `search_sessions_fts("octopus", 5, None)`, assert exactly 1 hit. `cargo test -p openfang-memory search_sessions_fts`.
  </verify>
  <done>
Helper exists, returns `SessionSearchHit` with snippet + bm25 score; unit test green.
  </done>
</task>

<task type="auto">
  <name>Task 2: tool_session_search dispatch + schema in tool_runner.rs</name>
  <files>crates/openfang-runtime/src/tool_runner.rs</files>
  <action>
Around line 203 (the `match tool_name { ... }` block), insert ABOVE the closing brace, surrounded by anchor comments verbatim:
```
        // === PHASE 1 PLAN 01-04 session_search ===
        "session_search" => tool_session_search(input, kernel).await,
        // === END PHASE 1 PLAN 01-04 ===
```
Implement `async fn tool_session_search(input: serde_json::Value, kernel: &dyn KernelHandle) -> Result<String, String>`. Parse `input`: required `query: String`; optional `limit: usize` (default 5, max 50); optional `agent_id: String` (parse via `AgentId::from_str` if present). Call `kernel.memory().sessions().search_sessions_fts(...)` (consult `KernelHandle` for the exact accessor — add a thin method if necessary). Return `serde_json::to_string(&hits).map_err(|e| e.to_string())?`. On empty hits return `"[]"` not an error.

Around line 645 in the schema list, insert (also anchored):
```
        // === PHASE 1 PLAN 01-04 session_search schema ===
        json!({
            "name": "session_search",
            "description": "Full-text search across all past conversation sessions. Use to recall what was discussed, decided, or discovered.",
            "input_schema": { "type": "object", "properties": {
                "query":    { "type": "string", "description": "FTS5 search query" },
                "limit":    { "type": "integer", "description": "Max results (default 5, max 50)" },
                "agent_id": { "type": "string", "description": "Filter to a specific agent UUID" }
            }, "required": ["query"] }
        }),
        // === END PHASE 1 PLAN 01-04 session_search schema ===
```
No capability gate — read-only tool. If a `tool_count` constant exists, increment by 1 (search the file for it).
  </action>
  <verify>
`cargo build -p openfang-runtime --lib` clean. `cargo clippy -p openfang-runtime --all-targets -- -D warnings` clean.
  </verify>
  <done>
Tool dispatchable; schema list contains session_search; anchors present.
  </done>
</task>

<task type="auto">
  <name>Task 3: Live integration test of session_search</name>
  <files>(no source edit — manual)</files>
  <action>
Run the 8-step live workflow from CLAUDE.md against this change ONLY (do not bundle with other waves):
1. `taskkill` any daemon. 2. `cargo build --release -p openfang-cli`. 3. `GROQ_API_KEY=... target/release/openfang.exe start &; sleep 6; curl -s http://127.0.0.1:4200/api/health`. 4. Get agent ID via `curl -s http://127.0.0.1:4200/api/agents`. 5. Send two messages mentioning "neutrino" and "octopus" via `/api/agents/{id}/message`. 6. Send a message: `Use session_search to find any past mention of "neutrino" and report the snippet.` 7. Verify the agent's tool call result contains a snippet with `<b>neutrino</b>`. 8. Tail the daemon log — confirm no errors. 9. Stop daemon.
Record observed latency in 01-04-SUMMARY.md (must be < 500ms per FTS-04).
  </action>
  <verify>
Manual checklist; record curl outputs in summary.
  </verify>
  <done>
Live agent successfully invoked session_search; latency observed; daemon clean.
  </done>
</task>

</tasks>

<verification>
- `cargo build --workspace --lib` clean.
- `cargo test --workspace` clean (1+ new helper test).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Live curl probe per Task 3 completes; agent uses the tool successfully.
</verification>

<success_criteria>
- [ ] `session_search` callable from an agent and returns JSON hits.
- [ ] Snippet column contains `<b>...</b>` highlight markers.
- [ ] agent_id filter narrows results.
- [ ] Live latency < 500ms on pc162.
- [ ] tool_runner.rs has both anchor blocks intact.
</success_criteria>

<output>
After completion, create `.planning/phases/01-self-learning-core/01-04-SUMMARY.md` summarizing:
- What was implemented (3-5 bullets)
- Files changed (final list)
- Tests added (count + brief)
- Live latency observed (must be < 500ms)
- Any decisions made during execution that weren't pinned in this plan
- Any follow-ups for later plans
</output>
