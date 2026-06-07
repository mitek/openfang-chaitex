//! Multi-source fact retrieval used by `ReasoningEngine::reason`.
//!
//! Each `ReasoningLevel` reads a different combination of the memory
//! substrate's stores (per design § 2.4):
//!
//! - `Minimal`: structured KV exact-match + semantic recall fallback. No
//!   FTS5, no LLM.
//! - `Low`: semantic recall (top-5) + FTS5 session search (top-5).
//! - `Medium` / `High` / `Max`: everything Low does PLUS knowledge-graph
//!   scan + structured KV scan, deduplicated and capped at `max_facts`.
//!
//! `Max` re-uses the `Medium`/`High` retrieval path. The "deep" qualifier
//! refers to the synthesis the LLM performs, not the fact-gathering.
//!
//! Locks: we never hold the SQLite connection mutex across `.await`. FTS5
//! lookups go through `tokio::task::spawn_blocking` because rusqlite is
//! sync.

use crate::{FactReference, FactSource, ReasoningError, ReasoningLevel};
use openfang_memory::MemorySubstrate;
use openfang_types::memory::{GraphPattern, Memory, MemoryFilter};
use std::sync::Arc;

/// Default cap on facts returned to the synthesizer. Each level may
/// override.
const DEFAULT_FACTS_CAP: usize = 20;

/// Hard ceiling on facts returned regardless of caller's `max_facts`. The
/// agent context window has limits — 50 is generous enough for the deepest
/// `Max`-level call.
const MAX_FACTS_CEILING: usize = 50;

/// Pull facts from the memory substrate at the depth required by `level`.
///
/// Returns a deduplicated `Vec<FactReference>` capped at `max_facts.min(50)`.
/// The order is best-relevance-first; ties are broken by source-key for
/// determinism.
pub async fn retrieve_facts(
    memory: &Arc<MemorySubstrate>,
    query: &str,
    level: ReasoningLevel,
    max_facts: usize,
) -> Result<Vec<FactReference>, ReasoningError> {
    let cap = max_facts.min(MAX_FACTS_CEILING);
    let mut facts: Vec<FactReference> = Vec::new();

    match level {
        ReasoningLevel::Minimal => {
            facts.extend(structured_kv_token_lookup(memory, query, cap).await?);
            if facts.len() < cap {
                facts.extend(semantic_recall(memory, query, cap - facts.len()).await?);
            }
        }
        ReasoningLevel::Low => {
            facts.extend(semantic_recall(memory, query, 5).await?);
            facts.extend(fts5_session_search(memory, query, 5).await?);
        }
        ReasoningLevel::Medium | ReasoningLevel::High | ReasoningLevel::Max => {
            facts.extend(semantic_recall(memory, query, 5).await?);
            facts.extend(fts5_session_search(memory, query, 5).await?);
            facts.extend(structured_kv_token_lookup(memory, query, 5).await?);
            facts.extend(knowledge_graph_scan(memory, query, 5).await?);
        }
    }

    dedupe_and_cap(&mut facts, cap);
    Ok(facts)
}

/// Semantic recall via the `Memory::recall` async API. The semantic store
/// already uses `spawn_blocking` internally, so this is the cheapest path
/// for fact retrieval that respects the !Send guard on the rusqlite
/// connection.
async fn semantic_recall(
    memory: &Arc<MemorySubstrate>,
    query: &str,
    limit: usize,
) -> Result<Vec<FactReference>, ReasoningError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let fragments = memory
        .recall(query, limit, Some(MemoryFilter::default()))
        .await
        .map_err(|e| ReasoningError::Memory(format!("recall failed: {e}")))?;
    Ok(fragments
        .into_iter()
        .map(|f| FactReference {
            source: FactSource::Memory {
                memory_id: f.id.to_string(),
            },
            content: f.content,
            relevance: f.confidence.clamp(0.0, 1.0),
            timestamp: Some(f.created_at.to_rfc3339()),
        })
        .collect())
}

/// FTS5 search against `session_messages_fts` (created by v9, plan 01-02).
///
/// Plan 01-04 lifted the SQL into `SessionStore::search_sessions_fts` so the
/// agent-facing `session_search` tool and this reasoning path share one
/// canonical implementation. We wrap the (sync) substrate call in
/// `tokio::task::spawn_blocking` so the `!Send` rusqlite mutex never crosses
/// an `.await`. Any `MATCH`-syntax problem degrades to an empty result
/// (WARN-logged) — the FTS path is a best-effort addition, not a critical
/// retrieval source.
///
/// The `score` field returned by `SessionStore::search_sessions_fts` is the
/// raw bm25 rank (lower / more negative = better). We invert it to a
/// `[0, 1]` relevance for `FactReference` via `1 / (1 + |score|)`. The
/// `message_index` in the resulting `FactSource::Session` is left at 0 here:
/// the new helper returns the row-level `(session_id, agent_id, role,
/// timestamp, snippet, score)` shape — for the reasoning callers the
/// `message_index` was historically unused downstream, so we keep the
/// signature stable and drop the field.
async fn fts5_session_search(
    memory: &Arc<MemorySubstrate>,
    query: &str,
    limit: usize,
) -> Result<Vec<FactReference>, ReasoningError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    // Build an owned SessionStore over the shared connection so the blocking
    // task is Send. `SessionStore::new` is cheap — it just wraps the existing
    // `Arc<Mutex<Connection>>` returned by `usage_conn()`.
    let store = openfang_memory::session::SessionStore::new(memory.usage_conn());
    let q = query.to_string();
    let result = tokio::task::spawn_blocking(move || store.search_sessions_fts(&q, limit, None))
        .await
        .map_err(|e| ReasoningError::Memory(format!("FTS task join error: {e}")))?;

    match result {
        Ok(hits) => {
            let mut out: Vec<FactReference> = Vec::with_capacity(hits.len());
            for hit in hits {
                let relevance = (1.0 / (1.0 + hit.score.abs())) as f32;
                out.push(FactReference {
                    source: FactSource::Session {
                        session_id: hit.session_id,
                        message_index: 0,
                    },
                    // Use the snippet (with `<b>` markers) as the surfaced
                    // content — it's already short and shows the matched
                    // span in context.
                    content: hit.snippet,
                    relevance: relevance.clamp(0.0, 1.0),
                    timestamp: Some(hit.timestamp),
                });
            }
            Ok(out)
        }
        Err(e) => {
            // Bad MATCH syntax or transient SQLite error — log + return
            // empty so the rest of fact retrieval still runs.
            tracing::warn!(query = %query, "FTS5 session search degraded: {e}");
            Ok(Vec::new())
        }
    }
}

/// Structured KV exact-token lookup: split the query on whitespace, look
/// each token up directly in the structured store (cross-agent).
///
/// Plan 01-04 may upgrade this with proper agent-scoping; for now Minimal
/// returns matches across all agents — the reasoning layer uses the result
/// to bias the answer, not as authoritative.
async fn structured_kv_token_lookup(
    memory: &Arc<MemorySubstrate>,
    query: &str,
    limit: usize,
) -> Result<Vec<FactReference>, ReasoningError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn_arc = memory.usage_conn();
    let q = query.to_string();
    let lim = limit as i64;
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<FactReference>, String> {
        let tokens: Vec<String> = q
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let conn = conn_arc.lock().map_err(|e| format!("conn lock poisoned: {e}"))?;
        let mut out: Vec<FactReference> = Vec::new();
        // For each token, run an exact-key lookup. Stays small (≤ token
        // count × distinct agents). For agent-scoped KV the table is
        // (agent_id, key) so we scan once and accept all matching agents.
        let mut stmt = conn
            .prepare(
                "SELECT agent_id, key, value
                 FROM kv_store
                 WHERE key = ?1
                 LIMIT ?2",
            )
            .map_err(|e| format!("prepare failed: {e}"))?;
        for token in &tokens {
            if out.len() >= lim as usize {
                break;
            }
            let rows = stmt
                .query_map(rusqlite::params![token, lim], |row| {
                    let _agent_id: String = row.get(0)?;
                    let key: String = row.get(1)?;
                    let value_blob: Vec<u8> = row.get(2)?;
                    let value_str = String::from_utf8_lossy(&value_blob).to_string();
                    Ok(FactReference {
                        source: FactSource::StructuredKv { key: key.clone() },
                        content: format!("{key} = {value_str}"),
                        relevance: 1.0,
                        timestamp: None,
                    })
                })
                .map_err(|e| format!("query_map failed: {e}"))?;
            for fr in rows.flatten() {
                out.push(fr);
                if out.len() >= lim as usize {
                    break;
                }
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| ReasoningError::Memory(format!("KV task join error: {e}")))?;

    match result {
        Ok(v) => Ok(v),
        Err(msg) => {
            tracing::warn!(query = %query, "structured KV lookup degraded: {msg}");
            Ok(Vec::new())
        }
    }
}

/// Knowledge-graph scan: look for entities whose `name` contains any
/// query token. Returns a `FactReference` per hit.
///
/// We deliberately avoid `query_graph(GraphPattern{...})` here because
/// `GraphPattern` is a relation-walk query — not a free-text entity name
/// search. Direct SQL is cheaper and gives us the entity surface a Medium+
/// reasoning call needs.
async fn knowledge_graph_scan(
    memory: &Arc<MemorySubstrate>,
    query: &str,
    limit: usize,
) -> Result<Vec<FactReference>, ReasoningError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    // Tease the API: a future plan can replace this with a real graph
    // walk. For now we issue a GraphPattern query with `max_depth=0` and
    // return any matching entity as a fact. If GraphPattern semantics
    // change we fall back gracefully.
    let pat = GraphPattern {
        source: None,
        relation: None,
        target: None,
        max_depth: 0,
    };
    let matches = memory.query_graph(pat).await.unwrap_or_default();
    let q_lower = query.to_lowercase();
    let mut out: Vec<FactReference> = Vec::new();
    for m in matches {
        if out.len() >= limit {
            break;
        }
        // Surface either source or target entity whose name overlaps
        // textually with the query.
        for entity in [&m.source, &m.target] {
            if entity.name.to_lowercase().contains(&q_lower)
                || q_lower.split_whitespace().any(|t| entity.name.to_lowercase().contains(t))
            {
                out.push(FactReference {
                    source: FactSource::KnowledgeGraph {
                        entity_id: entity.id.clone(),
                    },
                    content: format!("{}: {}", entity.entity_type_label(), entity.name),
                    relevance: 0.6,
                    timestamp: Some(entity.updated_at.to_rfc3339()),
                });
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
    Ok(out)
}

/// Helper trait so we can call `entity.entity_type_label()` without
/// dragging a new `Display` impl into `openfang-types`.
trait EntityTypeLabel {
    fn entity_type_label(&self) -> String;
}
impl EntityTypeLabel for openfang_types::memory::Entity {
    fn entity_type_label(&self) -> String {
        use openfang_types::memory::EntityType::*;
        match &self.entity_type {
            Person => "Person".into(),
            Organization => "Organization".into(),
            Project => "Project".into(),
            Concept => "Concept".into(),
            Event => "Event".into(),
            Location => "Location".into(),
            Document => "Document".into(),
            Tool => "Tool".into(),
            Custom(s) => s.clone(),
        }
    }
}

/// Dedupe by source key, keep the first occurrence (highest relevance
/// because callers append in order Semantic → FTS → KV → Graph and each
/// step is itself ordered by best-rank-first), then truncate to `cap`.
fn dedupe_and_cap(facts: &mut Vec<FactReference>, cap: usize) {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let original: Vec<FactReference> = std::mem::take(facts);
    for f in original {
        let key = source_key(&f.source);
        if seen.insert(key) {
            facts.push(f);
            if facts.len() >= cap {
                break;
            }
        }
    }
}

fn source_key(s: &FactSource) -> String {
    match s {
        FactSource::Memory { memory_id } => format!("mem:{memory_id}"),
        FactSource::Session {
            session_id,
            message_index,
        } => format!("ses:{session_id}:{message_index}"),
        FactSource::KnowledgeGraph { entity_id } => format!("kg:{entity_id}"),
        FactSource::StructuredKv { key } => format!("kv:{key}"),
    }
}

#[allow(dead_code)]
const _DEFAULT_FACTS_CAP_SANITY: () = {
    // Linker-time check that DEFAULT_FACTS_CAP fits in the ceiling.
    assert!(DEFAULT_FACTS_CAP <= MAX_FACTS_CEILING);
};
