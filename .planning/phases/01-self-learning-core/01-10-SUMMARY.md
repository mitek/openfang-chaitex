# 01-10 — reasoning crate scaffold — SUMMARY

**Status:** complete
**Date:** 2026-06-07
**Commits:** 0f76e82

## One-liner

New `openfang-reasoning` workspace crate landed with the full public type
surface (`ReasoningLevel`, `ReasoningQuery`, `ReasoningResult`,
`FactReference`, `FactSource`, `ReasoningLlm` async trait,
`ReasoningEngine::{new, with_llm, has_llm, reason}`) and a stubbed
`reason()` that returns `ReasoningError::NotYetImplemented` so plan 01-13
can wire the agent loop / tool layer against a stable API today.

## Files created

- `crates/openfang-reasoning/Cargo.toml` — workspace-member manifest;
  deps scoped to REQ MR-01 / MR-03.
- `crates/openfang-reasoning/src/error.rs` — `ReasoningError` enum.
- `crates/openfang-reasoning/src/lib.rs` — public types + `ReasoningEngine`
  scaffold + 7 unit tests.

## Files modified

- `Cargo.toml` — added `"crates/openfang-reasoning"` to `members`
  alphabetically between `openfang-memory` and `openfang-runtime`.
- `Cargo.lock` — auto-updated.

## Tests added

7 unit tests in `lib.rs::tests`:

1. `level_ordering_holds` — `Minimal < Low < Medium < High < Max`; also
   pins the downgrade idiom `requested.min(ReasoningLevel::Low)`.
2. `level_serializes_lowercase` — wire format for the dashboard / tool
   schema.
3. `query_round_trip_json` — `ReasoningQuery` round-trip stable.
4. `fact_source_tag_is_type_field` — pins JSON wire format
   (`{"source":{"type":"Session", ...}}`).
5. `engine_reason_returns_not_yet_implemented` — async; uses
   `MemorySubstrate::open_in_memory(0.0)`; asserts the error message
   references plan 01-11 (so a maintainer chasing the stub knows where to
   look).
6. `level_not_allowed_error_format` — pins `Display` for log scrapers.
7. `approval_required_error_format` — same.

All 7 pass. Workspace gates clean (build / test / clippy `-D warnings`).

## Path corrections to MemorySubstrate import

None needed. `openfang_memory::MemorySubstrate` (the top-level re-export
from `crates/openfang-memory/src/lib.rs:21`) is the canonical path, and
the substrate module itself is also accessible at
`openfang_memory::substrate::MemorySubstrate`. I used the shorter
top-level re-export.

## Deviations

**[Rule 3 — auto-fix blocking compile]** A bare `pub(crate) memory:
Arc<MemorySubstrate>` triggered `dead_code` (`-D warnings` is on for the
workspace clippy gate). Annotated the field with `#[allow(dead_code)]` and
a doc comment explaining the field is intentionally held until plan 01-11
wires the dispatch body that reads it. The `has_llm()` accessor reads
`llm` so that field is not affected.

**[Rule 2 — added a critical helper]** Added `has_llm(&self) -> bool` as
a public accessor. The plan only required `new` / `with_llm` / `reason`,
but tests need a way to assert the LLM-attachment state without reaching
into private fields. The accessor is cheap and stable, so it's safer to
ship it now than to mark the field `pub` later.

## Decisions made during execution

1. **`thiserror` added to deps.** Plan said "use `thiserror` (workspace
   wide)", so I added it to the crate's `[dependencies]` explicitly. The
   workspace already declares it at the top level.
2. **`FactSource` tagged `#[serde(tag = "type")]`** — pinned by a test so
   downstream consumers (tool schema in 01-13, dashboard JSON parsing)
   know the wire format. The variant names use `PascalCase` defaults — if
   the tool layer wants `snake_case` (`session`/`memory`), 01-13 can add
   `#[serde(rename_all = "snake_case")]` then; pinning now would presume
   a downstream contract that isn't ratified yet.
3. **Optional `llm`.** The engine accepts `Option<Arc<dyn ReasoningLlm>>`
   because `Minimal` and `Low` levels are lookup-only — they need no LLM.
   This makes the engine constructible in unit tests without a fake LLM.
   Plan 01-11 will return a clear error if `Medium+` is invoked without
   an LLM attached.

## Follow-ups for later plans

- **01-11 (level dispatch + fact retrieval):** the `reason()` body lands
  here. Hook fact retrieval to `self.memory.recall(...)` and to
  `session_search` (FTS5 — plan 01-04). Replace the `NotYetImplemented`
  arm with the dispatch table.
- **01-12 (budget tracker + config):** add `ReasoningConfig` to
  `KernelConfig` with `deny_unknown_fields` (REQ MR-05). The
  `[reasoning]` log marker (`(from config)` vs `(DEFAULT)`) is wired
  here.
- **01-13 (tool integration):** register `memory_reason` tool. The
  agent-loop side wires `ReasoningLlm` to forward through `KernelHandle`
  → `LlmDriver` (REQ MR-03 — no new HTTP client).
- **`FactSource` rename rules:** if the tool schema in 01-13 wants
  lowercase / snake_case variant names, add the rename attribute there
  and update `fact_source_tag_is_type_field`.
- **Concrete cost model:** `ReasoningResult.estimated_cost_usd` is plumbed
  but unused in the stub. Plan 01-11 / 01-12 must define how it's computed
  (per-level token estimate × model price).
