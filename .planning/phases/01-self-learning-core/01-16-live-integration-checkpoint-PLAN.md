---
phase: 01-self-learning-core
plan: 16
type: checkpoint:human-verify
wave: 5
depends_on: [01-04, 01-09, 01-14, 01-15]
files_modified: []
files_created:
  - .planning/phases/01-self-learning-core/01-16-UAT.md
autonomous: false
user_setup:
  - "Set `GROQ_API_KEY` (or equivalent provider key) in the shell that runs the daemon"
  - "Be at the repo root with a clean working tree (commit pending Phase-1 work first)"
  - "Have a known-good v8 database OR an empty `~/.openfang/memory.db` to test the fresh-install path. If testing the migration path, take a backup of the v8 DB first."
must_haves:
  truths:
    - "Daemon boots clean against a v8 user database; `pragma_user_version` reports 9 after first boot (success-criterion 8)"
    - "All four new tools (`session_search`, `skill_manage`, `memory_reason`, `memory_conclude`) execute end-to-end against a real LLM call (success-criterion 9, X-05)"
    - "Side effects observable: `session_messages` populated, `reasoning_budget` row inserted per `memory_reason` call, `__user_profile__` key written by `memory_conclude` (FTS-03, MR-05, MR-06)"
    - "Dashboard HTML (`curl /`) surfaces the new tools (X-01)"
    - "All 12 success criteria from REQUIREMENTS.md verified — UAT.md captures pass/fail per criterion"
  artifacts:
    - ".planning/phases/01-self-learning-core/01-16-UAT.md with 12 ticked criteria and observed outputs"
---

<objective>
Human-verify Phase 1 end-to-end against a running daemon. This checkpoint blocks further phase work until all 12 REQUIREMENTS success criteria are ticked. No code edits in this plan — pure validation.
</objective>

<context>
@CLAUDE.md
@.planning/REQUIREMENTS.md
@.planning/phases/01-self-learning-core/01-CONTEXT.md
</context>

<tasks>

<task type="manual">
  <name>Task 1: Boot a fresh release daemon</name>
  <files>(none — runtime only)</files>
  <action>
Per CLAUDE.md "How to Run Live Integration Tests" steps 1-3:
1. `tasklist | grep -i openfang; taskkill //PID <pid> //F; sleep 3`
2. `cargo build --release -p openfang-cli` (must be green).
3. `GROQ_API_KEY=... target/release/openfang.exe start &; sleep 6; curl -s http://127.0.0.1:4200/api/health` (expect 200 + healthy JSON).
Record daemon log path. Tail-and-grep for the marker line from plan 01-12: `loaded reasoning config ... (DEFAULT — no [reasoning] section)` OR `(from config)`. Capture in UAT.md.
  </action>
  <verify>
`/api/health` returns 200.
Daemon log shows the reasoning-config marker.
  </verify>
  <done>
Daemon up; criterion 11 (DEFAULT marker) initial observation captured.
  </done>
</task>

<task type="manual">
  <name>Task 2: Verify each success criterion via curl</name>
  <files>.planning/phases/01-self-learning-core/01-16-UAT.md</files>
  <action>
Create UAT.md with one section per success criterion (1..12 from REQUIREMENTS.md ## "Mapping to Phase 1 success criteria"). For each, record the exact curl command + response excerpt. Sketch per criterion:

1. **skill_manage(create)** — POST `/api/agents/{id}/message` with prompt "Create a new skill named test-skill via skill_manage. Then call skill_manage(action=list) and report whether it appears." Expect both calls in the response; assert the new skill is in `list`.
2. **skill_manage(patch)** on mutable — first `create`, then patch within the same turn; verify file diff on disk via `cat ~/.openfang/skills/test-skill/skill.toml`; assert patched content visible to the agent in the next call.
3. **skill_manage(patch) on protected** — ask the agent to patch `memory-core`; agent response must include the structured `ProtectedSkill` error; verify `sha256sum ~/.openfang/bundled/memory-core/skill.toml` matches the pre-state.
4. **session_search** — seed 2 messages mentioning unique terms via `/api/agents/{id}/message`, then ask `Use session_search to find "<term>"`; time the round-trip; assert <500ms (record exact ms).
5. **memory_reason medium** — ask `Use memory_reason at level=medium to summarize what I want from this agent`; assert response JSON has `answer`, `supporting_facts`, `confidence`.
6. **memory_reason max** — ask `Use memory_reason at level=max`; assert ApprovalRequired error in agent's tool result. Then `memory_reason at level=max with approved=true` proceeds.
7. **Budget exceeded** — temporarily set `monthly_budget_usd = 0.0001` in `~/.openfang/config.toml`, restart daemon, repeat a `memory_reason(medium)` call. Verify:
   - warn mode: response includes downgrade caveat; `reasoning_budget` row marked `[downgraded]`.
   - block mode: response includes `BudgetExceeded` error.
   Reset config after.
8. **Workspace gates** — run on the same checkout: `cargo build --workspace --lib && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`. Capture exit codes. Run the v8→v9 transition test explicitly: `cargo test -p openfang-memory migrate_v8_to_v9 -- --nocapture`.
9. **Live integration** — this entire checkpoint constitutes the pass; mark green when criteria 1-7 all green.
10. **Bundled defaults** — `curl /api/agents/{id}/message` asking the agent to call `skill_manage(action=list)`. Inspect the returned list: every SYSTEM_SKILL has `protected=true`; every non-SYSTEM bundled skill has `protected=false, mutable=false`. Cross-check: `git diff bundled/` is empty.
11. **deny_unknown_fields** — write `[reasoning]\nmax_input_tkns = 30000\n` (typo) into `~/.openfang/config.toml`, restart daemon, expect startup error mentioning `unknown field`. Record exact log line. Then restore config; verify log shows `(DEFAULT — no [reasoning] section)` or `(from config)` markers.
12. **CHANGELOG** — `grep -A 25 'Unreleased' CHANGELOG.md` — verify the schema-v9 / backward-compat bullets.

Side-effect probes after the message flow:
- `sqlite3 ~/.openfang/memory.db "SELECT COUNT(*) FROM session_messages"` — > 0.
- `sqlite3 ~/.openfang/memory.db "SELECT COUNT(*) FROM session_messages_fts"` — > 0.
- `sqlite3 ~/.openfang/memory.db "SELECT * FROM reasoning_budget ORDER BY id DESC LIMIT 5"` — recent rows.
- `sqlite3 ~/.openfang/memory.db "SELECT key FROM kv WHERE key LIKE '__user_profile__%'"` — after `memory_conclude` runs, ≥ 1 key.

Dashboard probe (X-01 surfacing):
- `curl -s http://127.0.0.1:4200/ | grep -c -E 'session_search|skill_manage|memory_reason|memory_conclude'` — > 0 (only check that any of the four tool names surface; full dashboard wiring is out of scope).
  </action>
  <verify>
UAT.md lists 12 criteria each annotated PASS or FAIL with the curl/sqlite evidence.
  </verify>
  <done>
All 12 criteria PASS; user signs off in UAT.md.
  </done>
</task>

<task type="manual">
  <name>Task 3: Cleanup + sign-off</name>
  <files>.planning/phases/01-self-learning-core/01-16-UAT.md</files>
  <action>
1. `taskkill //PID <pid> //F` to stop daemon.
2. Append sign-off line to UAT.md: `signed-off-by: <user>, date: 2026-06-06` (use actual date).
3. If any criterion FAILED: do NOT sign off; open a follow-up plan to remediate and re-run this checkpoint.
4. Restore any temporarily-edited config files (e.g. budget reset from criterion 7).
  </action>
  <verify>
No daemon process remaining. UAT.md contains sign-off line.
  </verify>
  <done>
Daemon stopped. Phase-1 sign-off line in UAT.md.
  </done>
</task>

</tasks>

<verification>
- All 12 success criteria from REQUIREMENTS.md "## Mapping" table marked PASS in UAT.md.
- `cargo build --workspace --lib && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` all green on the released binary's checkout.
- v8→v9 transition test runs and passes against an actual populated DB (or the in-memory test from plan 01-02).
- Daemon stopped clean; no orphan processes.
</verification>

<success_criteria>
- [ ] Criterion 1: skill_manage(create) end-to-end PASS.
- [ ] Criterion 2: skill_manage(patch) on mutable PASS.
- [ ] Criterion 3: skill_manage(patch) on protected PASS (structured error + no disk diff).
- [ ] Criterion 4: session_search latency < 500ms PASS.
- [ ] Criterion 5: memory_reason(medium) returns answer/facts/confidence PASS.
- [ ] Criterion 6: memory_reason(max) gated by ApprovalRequired PASS.
- [ ] Criterion 7: budget exceeded → warn-downgrade OR block PASS.
- [ ] Criterion 8: workspace gates + v8→v9 transition test PASS.
- [ ] Criterion 9: live integration PASS.
- [ ] Criterion 10: bundled defaults correct, no `bundled/` git diff PASS.
- [ ] Criterion 11: typo'd `[reasoning]` fails startup; DEFAULT marker observed PASS.
- [ ] Criterion 12: CHANGELOG entry present PASS.
</success_criteria>

<output>
After completion, ensure `.planning/phases/01-self-learning-core/01-16-UAT.md` contains:
- One section per criterion with PASS/FAIL + evidence
- Sign-off line at the bottom
- Any deviations from this plan (e.g. criterion 4 latency observed)
No 01-16-SUMMARY.md needed for a checkpoint — UAT.md is the artifact.
</output>
