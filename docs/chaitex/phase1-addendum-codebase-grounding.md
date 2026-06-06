# Фаза 1: Аддендум — корректировки по результатам codebase mapping

**Дата**: 2026-06-06
**Статус**: Корректировка к [phase1-self-learning-architecture.md](phase1-self-learning-architecture.md)
**Источник**: Карта кодовой базы в `.planning/codebase/` (CONCERNS.md, ARCHITECTURE.md, STRUCTURE.md)

---

## Зачем этот документ

Оригинальный архитектурный дизайн Фазы 1 был написан на основе высокоуровневого понимания OpenFang. После прямого чтения исходного кода обнаружились **расхождения между предполагаемой и фактической архитектурой**, которые делают часть оригинального плана не-исполнимой как написано.

Этот аддендум **не заменяет** оригинальный документ. Он указывает, какие разделы оригинала нужно интерпретировать иначе, и предоставляет корректные реализации для затронутых частей.

Все исправления — **additive**: ни одна корректировка не отменяет архитектурные принципы оригинала (security-first, backward compat, protected skills, budget control). Они только привязывают дизайн к реальному коду.

---

## A. Блокирующие правки (без них Фаза 1 не соберётся)

### A.1 Включить FTS5 в `rusqlite` features

**Проблема**: оригинал § 2.5 проектирует `CREATE VIRTUAL TABLE ... USING fts5(...)`. Это вернёт `no such module: fts5` в рантайме, потому что фича `fts5` не включена.

**Текущее состояние** ([`Cargo.toml:58`](../../Cargo.toml#L58)):
```toml
rusqlite = { version = "0.31", features = ["bundled", "serde_json"] }
```

**Правка**:
```toml
rusqlite = { version = "0.31", features = ["bundled", "serde_json", "fts5"] }
```

Это включит FTS5 модуль в bundled SQLite. Никаких изменений в системных пакетах не нужно (SQLite собирается из исходников).

**Проверка после правки**: `cargo build --workspace --lib` должен пройти. Live-тест FTS5 модуля:
```sql
SELECT sqlite_version();              -- 3.x.x
SELECT * FROM pragma_compile_options WHERE compile_options LIKE 'ENABLE_FTS5';
-- должна вернуться строка ENABLE_FTS5
```

**Оценка**: 5 минут.

---

### A.2 Сессии хранятся как msgpack BLOB — FTS5 не может индексировать BLOB напрямую

**Проблема**: оригинал § 2.5 описывает триггер:
```sql
CREATE TRIGGER ... AFTER INSERT ON messages
BEGIN
    INSERT INTO session_messages_fts(session_id, agent_id, role, content)
    VALUES (new.session_id, new.agent_id, new.role, new.content);
END;
```

Этого триггера **не на чем повесить**. Таблица `messages` не существует. Фактическая схема:
```sql
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    messages BLOB NOT NULL,            -- ← rmp_serde::to_vec_named(&Vec<Message>)
    context_window_tokens INTEGER,
    label TEXT,
    created_at TEXT,
    updated_at TEXT
);
```
Источник: [`migration.rs:89`](../../crates/openfang-memory/src/migration.rs#L89), запись в [`session.rs:83`](../../crates/openfang-memory/src/session.rs#L83), чтение в [`session.rs:62`](../../crates/openfang-memory/src/session.rs#L62).

`messages` — это **один BLOB** на сессию, перезаписываемый целиком на каждый `save_session()`. FTS5 не умеет проникать внутрь BLOB.

#### A.2.1 Решение: миграция v9 — flat companion table + FTS5

Добавляем плоскую таблицу сообщений, которая поддерживается синхронно с BLOB-сессиями, и FTS5 поверх неё.

```rust
// crates/openfang-memory/src/migration.rs

const SCHEMA_VERSION: u32 = 9;  // bump from 8

// ...

if current_version < 9 {
    migrate_v9(conn)?;
}

/// Version 9: row-per-message flat table + FTS5 index for session search.
///
/// Sessions remain stored as msgpack BLOB in `sessions.messages` for
/// fast load. This v9 adds a parallel flat table indexed by FTS5 that
/// SessionStore::save_session keeps in sync. Read path unchanged —
/// FTS5 is only consulted by SessionSearchTool / ReasoningEngine.
fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        -- Flat per-message storage for indexing. NOT the primary read path.
        -- SessionStore.get_session() still loads from sessions.messages BLOB.
        CREATE TABLE IF NOT EXISTS session_messages (
            session_id    TEXT NOT NULL,
            agent_id      TEXT NOT NULL,
            message_index INTEGER NOT NULL,
            role          TEXT NOT NULL,          -- user|assistant|system|tool
            content       TEXT NOT NULL,          -- flattened text (see A.2.2)
            timestamp     TEXT NOT NULL,
            PRIMARY KEY (session_id, message_index)
        );
        CREATE INDEX IF NOT EXISTS idx_session_messages_agent
            ON session_messages(agent_id);
        CREATE INDEX IF NOT EXISTS idx_session_messages_session
            ON session_messages(session_id);

        -- External-content FTS5 index. Storage lives in session_messages;
        -- FTS5 only stores its inverted index. Smaller on disk than
        -- contentless or content-owning modes.
        CREATE VIRTUAL TABLE IF NOT EXISTS session_messages_fts USING fts5(
            content,
            session_id   UNINDEXED,
            agent_id     UNINDEXED,
            role         UNINDEXED,
            timestamp    UNINDEXED,
            content='session_messages',
            content_rowid='rowid',
            tokenize='porter unicode61 remove_diacritics 1'
        );

        -- Triggers that keep the FTS index in lockstep with session_messages.
        CREATE TRIGGER IF NOT EXISTS session_messages_ai
            AFTER INSERT ON session_messages
        BEGIN
            INSERT INTO session_messages_fts(rowid, content, session_id, agent_id, role, timestamp)
            VALUES (new.rowid, new.content, new.session_id, new.agent_id, new.role, new.timestamp);
        END;

        CREATE TRIGGER IF NOT EXISTS session_messages_ad
            AFTER DELETE ON session_messages
        BEGIN
            INSERT INTO session_messages_fts(session_messages_fts, rowid, content)
            VALUES ('delete', old.rowid, old.content);
        END;

        CREATE TRIGGER IF NOT EXISTS session_messages_au
            AFTER UPDATE ON session_messages
        BEGIN
            INSERT INTO session_messages_fts(session_messages_fts, rowid, content)
            VALUES ('delete', old.rowid, old.content);
            INSERT INTO session_messages_fts(rowid, content, session_id, agent_id, role, timestamp)
            VALUES (new.rowid, new.content, new.session_id, new.agent_id, new.role, new.timestamp);
        END;
        ",
    )?;

    // Backfill: walk existing sessions BLOBs and populate the flat table.
    // Best-effort — on individual decode failure we log and skip the session
    // rather than aborting the migration. Backfill failures don't break
    // existing read paths because the BLOB store is unchanged.
    backfill_session_messages(conn)?;
    Ok(())
}

fn backfill_session_messages(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, messages, updated_at FROM sessions"
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;

    for row in rows {
        let (session_id, agent_id, blob, ts) = match row {
            Ok(r) => r,
            Err(e) => { tracing::warn!("v9 backfill: row error: {}", e); continue; }
        };
        let messages: Vec<openfang_types::message::Message> =
            match rmp_serde::from_slice(&blob) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(
                        session_id = %session_id,
                        "v9 backfill: msgpack decode failed, skipping: {}", e
                    );
                    continue;
                }
            };
        for (idx, msg) in messages.iter().enumerate() {
            let content = flatten_message_content(msg);
            if content.is_empty() { continue; }
            conn.execute(
                "INSERT OR IGNORE INTO session_messages
                 (session_id, agent_id, message_index, role, content, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    session_id,
                    agent_id,
                    idx as i64,
                    role_string(&msg.role),
                    content,
                    ts,
                ],
            )?;
        }
    }
    Ok(())
}
```

#### A.2.2 Flattening `Message` → searchable text

`Message::content` is a `MessageContent` enum (text, tool_use, tool_result, images, etc.). FTS5 needs plain text. Define a stable flattener:

```rust
// crates/openfang-memory/src/session_fts.rs (new)

use openfang_types::message::{ContentBlock, Message, MessageContent};

/// Flatten a Message into a single searchable string.
///
/// Rules:
/// - Text blocks: included verbatim, joined by '\n'.
/// - tool_use: include tool name + serialized input (truncated to 2KB).
/// - tool_result: include the textual portion of the result.
/// - Images, audio: replaced by "[image]" / "[audio]" markers so the
///   message still indexes (a session with only an image otherwise
///   wouldn't appear in search).
///
/// Stable across versions — changing this requires a re-index migration.
pub fn flatten_message_content(msg: &Message) -> String {
    match &msg.content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Blocks(blocks) => {
            let mut out = String::new();
            for b in blocks {
                if !out.is_empty() { out.push('\n'); }
                match b {
                    ContentBlock::Text { text } => out.push_str(text),
                    ContentBlock::ToolUse { name, input, .. } => {
                        out.push_str(&format!("[tool_use:{}] ", name));
                        let mut s = serde_json::to_string(input).unwrap_or_default();
                        s.truncate(2048);
                        out.push_str(&s);
                    }
                    ContentBlock::ToolResult { content, .. } => {
                        out.push_str("[tool_result] ");
                        // ToolResult content varies; serialize the text portion
                        out.push_str(&serialize_tool_result_text(content));
                    }
                    ContentBlock::Image { .. } => out.push_str("[image]"),
                    // … add any other variants present in the actual enum
                }
            }
            out
        }
    }
}
```

Exact arms depend on the real `ContentBlock` enum in `openfang-types`. Phase planning must read it to fix the match. The principle stands: produce *some* indexable string for every message type, never produce `Some("")` that drops the message from search.

#### A.2.3 Dual-write in `SessionStore::save_session`

`save_session` rewrites the entire BLOB. Make it also rewrite the flat table for that session in a single transaction:

```rust
// crates/openfang-memory/src/session.rs

pub fn save_session(&self, session: &Session) -> OpenFangResult<()> {
    let mut conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
    let tx = conn.transaction().map_err(|e| OpenFangError::Memory(e.to_string()))?;

    // 1. Existing BLOB write — unchanged read path.
    let messages_blob = rmp_serde::to_vec_named(&session.messages)
        .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO sessions (id, agent_id, messages, context_window_tokens, label, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET messages = ?3, context_window_tokens = ?4, label = ?5, updated_at = ?6",
        rusqlite::params![
            session.id.0.to_string(), session.agent_id.0.to_string(),
            messages_blob, session.context_window_tokens as i64,
            session.label.as_deref(), now,
        ],
    ).map_err(|e| OpenFangError::Memory(e.to_string()))?;

    // 2. Flat table dual-write: delete-then-insert for this session.
    //    Simpler than diff-based update; per-session, bounded size.
    tx.execute(
        "DELETE FROM session_messages WHERE session_id = ?1",
        [session.id.0.to_string()],
    ).map_err(|e| OpenFangError::Memory(e.to_string()))?;

    let mut ins = tx.prepare(
        "INSERT INTO session_messages
         (session_id, agent_id, message_index, role, content, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    ).map_err(|e| OpenFangError::Memory(e.to_string()))?;
    for (idx, msg) in session.messages.iter().enumerate() {
        let content = crate::session_fts::flatten_message_content(msg);
        if content.is_empty() { continue; }
        ins.execute(rusqlite::params![
            session.id.0.to_string(),
            session.agent_id.0.to_string(),
            idx as i64,
            role_string(&msg.role),
            content,
            now,
        ]).map_err(|e| OpenFangError::Memory(e.to_string()))?;
    }
    drop(ins);
    tx.commit().map_err(|e| OpenFangError::Memory(e.to_string()))?;
    Ok(())
}
```

**Trade-off**: every save rewrites all flat rows for the session. Sessions in OpenFang are bounded (context window pressure forces compaction), so a full rewrite per save is acceptable. Alternative: track message-count high-water-mark and only insert tail messages. Optimize later only if profiling shows it.

**`delete_session` and `delete_agent`** must also delete from `session_messages` (FK-style cascade). FTS5 triggers handle removal from the index.

#### A.2.4 Session backfill — costs

For an agent with 500 sessions × 100 messages each: 50k INSERTs at ~10–50 µs each = ~0.5–2.5 seconds on a fast disk, longer on SD card (pc162). Acceptable as a one-time migration. Wrap in a transaction so it's atomic (already a SQLite migration convention).

If profiling shows pc162 backfill is too slow: defer backfill behind a flag (`reasoning.fts_backfill = "on_startup" | "lazy" | "off"`). Lazy mode populates the flat table on `save_session`, accepting that old sessions are invisible to FTS until next save.

---

### A.3 Migration v→v+1 transition tests on populated databases

**Problem flagged by CONCERNS.md**: existing migration tests only verify fresh DB creation. A v8→v9 migration on a real user database has no test coverage today.

**Fix**: add a test pattern to [`migration.rs`](../../crates/openfang-memory/src/migration.rs) tests:

```rust
#[test]
fn migrate_v8_to_v9_preserves_sessions_and_backfills_fts() {
    let conn = Connection::open_in_memory().unwrap();
    // Force v8.
    set_schema_version(&conn, 8).unwrap();
    migrate_v1(&conn).unwrap();
    migrate_v2(&conn).unwrap();
    // … through v8

    // Insert two test sessions as BLOBs.
    let msgs = vec![
        Message::user_text("hello world"),
        Message::assistant_text("hi! how can I help with rust today?"),
    ];
    let blob = rmp_serde::to_vec_named(&msgs).unwrap();
    conn.execute("INSERT INTO sessions VALUES (?, ?, ?, 0, NULL, ?, ?)",
        rusqlite::params!["s1", "a1", blob, "2026-06-06", "2026-06-06"]).unwrap();

    // Run v9.
    migrate_v9(&conn).unwrap();

    // Assert: BLOB preserved, flat table populated, FTS5 returns hit.
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_messages WHERE session_id = 's1'",
        [], |r| r.get(0)).unwrap();
    assert_eq!(count, 2);

    let hits: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_messages_fts WHERE session_messages_fts MATCH 'rust'",
        [], |r| r.get(0)).unwrap();
    assert!(hits >= 1, "FTS5 should find 'rust' in backfilled session");
}
```

Add similar test for the "broken BLOB skipped, migration succeeds" path (defensive against schema drift in `Message`).

---

## B. Точечные правки оригинала

### B.1 § 1.7 Skill registry mutation visibility — `snapshot()` caveat

**Проблема**: оригинал § 1.6 предполагает, что после `patch_skill` агент сразу видит обновлённый навык. Фактически — нет.

Источник: [`registry.rs:55`](../../crates/openfang-skills/src/registry.rs#L55).
```rust
/// Used to avoid holding `RwLockReadGuard` across `.await` points
/// (the guard is `!Send`).
pub fn snapshot(&self) -> SkillRegistry {
    SkillRegistry { skills: self.skills.clone(), … }
}
```

The runtime takes a `SkillRegistry` snapshot (deep clone of the `skills` HashMap) and passes `&SkillRegistry` to the agent loop / tool runner ([`agent_loop.rs:301`](../../crates/openfang-runtime/src/agent_loop.rs#L301), [`tool_runner.rs:116`](../../crates/openfang-runtime/src/tool_runner.rs#L116)). Mutations on the live registry made by `skill_manage` (which is a tool, so it runs *inside* the snapshot's borrow scope) are not visible to the snapshot the agent is currently using.

**Fix options** (pick during Phase 1 planning):

1. **Re-snapshot per tool invocation.** Cheap (HashMap clone), simple, has the cost of cloning skills per tool call. Probably wrong default — most tool calls don't mutate skills.

2. **Re-snapshot post-`skill_manage`.** `skill_manage` returns a sentinel signal; the agent loop detects it and rebuilds the snapshot before the next tool dispatch. Localized, no cost in the common case. **Recommended.**

3. **Refactor SkillRegistry access to use `Arc<RwLock<…>>` directly with non-async holders.** Bigger change, removes the snapshot dance. Out of scope for Phase 1.

4. **Document and accept.** `skill_manage` is followed by an explicit `skill_reload` step. Simplest, worst UX.

**Decision in this addendum**: option 2. `skill_manage` always returns a `{ "skill_refresh_required": true, … }` field; the agent loop checks for it after every tool result and re-snapshots before the next iteration. Cost: one HashMap clone after each mutation. No cost in the read-only common path.

This also addresses § 1.5's claim "Агент улучшает навык" — without the re-snapshot, the next `skill_manage(patch=…, "v3 instructions")` call would patch a *stale* snapshot view, and the agent's understanding of which version is current would diverge from disk.

### B.2 § 1.3 Security pipeline — `Merkle audit record`

`Merkle audit record` corresponds to [`audit_entries`](../../crates/openfang-memory/src/migration.rs#L310) inserted via the existing audit infrastructure. Phase 1 should reuse that, not invent a new audit format. The audit append on every skill mutation is a one-line call into the existing `AuditAppender`; document the exact entry shape (`event_type`, `payload` schema) in the implementation plan, not now.

### B.3 § 2.6 / § 3 Tool registration is **not** trait-based today

**Problem**: original specifies `impl Tool for SkillManageTool { … }`. There is no `Tool` trait in `openfang-runtime`.

**Actual pattern** (verified [`tool_runner.rs:203`](../../crates/openfang-runtime/src/tool_runner.rs#L203)):

```rust
let result = match tool_name {
    "file_read"      => tool_file_read(input, …).await,
    "web_search"     => tool_web_search(input, …).await,
    "memory_store"   => tool_memory_store(input, kernel),
    "memory_recall"  => tool_memory_recall(input, kernel),
    // ... 30+ arms
};
```

Plus a schema list (around [`tool_runner.rs:645`](../../crates/openfang-runtime/src/tool_runner.rs#L645)) where every built-in tool's JSON schema is hand-registered.

**Adjustment**: each new Phase 1 tool (`skill_manage`, `memory_reason`, `session_search`) lands as:

1. A free function `tool_<name>(input, kernel, …) -> Result<String, String>` in `tool_runner.rs`.
2. One arm in the dispatch `match`.
3. One schema entry in the schema list, returning the JSON Schema from the original design § 1.4 / § 2.6 / § 3.

A `Tool` trait refactor is reasonable mid-term but out of scope for Phase 1. Do not block on it.

### B.4 § 1.7.4 Migration via shell scripts — replace with code-level defaults

**Problem flagged by CONCERNS.md**: original proposes `scripts/protect-system-skills.sh` to append `protected = true` to bundled `skill.toml` files at build time. This couples build to script order and silently mutates source files.

**Replacement**: encode the protected/mutable defaults in `SkillRegistry::load_bundled`:

```rust
// crates/openfang-skills/src/registry.rs

const SYSTEM_SKILLS: &[&str] = &[
    "skill-manage", "tool-dispatch", "memory-core", "memory-reason",
    "session-manager", "session-search", "event-bus", "kernel-api",
    "security-scanner", "prompt-injection",
];

impl SkillRegistry {
    pub fn load_bundled(&mut self, …) -> Result<…> {
        for entry in bundled_iter() {
            let mut config = parse_skill_toml(&entry)?;

            // Apply defaults if the skill.toml didn't set the field explicitly.
            // Treats absence as "use system default", not "mutable".
            if config.skill.mutable.is_none() {
                config.skill.mutable = Some(false);  // bundled = immutable by default
            }
            if config.skill.protected.is_none() {
                config.skill.protected = Some(SYSTEM_SKILLS.contains(&entry.name()));
            }
            // ...
        }
    }
}
```

`mutable` and `protected` become `Option<bool>` in the manifest (use `#[serde(default)]`), with code-level defaults applied at load. No build scripts. Backward compatible: existing skill.toml files with no `mutable`/`protected` fields silently get the right defaults.

This also resolves the design contradiction with `create_skill`: it now takes an optional `mutable: Option<bool>` arg, defaulting to `true` for user-created skills (overridable to pin a creation as immutable).

---

## C. Не-блокирующие, но важные правки

### C.1 § 2.8 UserProfile auto-write contradicts § 5 "no automatic memory writes"

Original § 2.8 says profile is updated after every medium+ reasoning call. Original § 5 architectural principle says "reasoning engine does not auto-write to memory — only via explicit `memory_conclude`."

These cannot both be true. **Resolution**: profile updates are *opt-in*. `memory_reason` returns the synthesized result; the caller (usually the agent or `memory_conclude`) decides whether to persist. Auto-update is a separate config flag `[reasoning] auto_update_profile = false` (default false). This preserves both invariants:

- The agent retains control over what is remembered (no surprise writes).
- Power users can flip the flag for fully-automatic profile maintenance.

Document the flag in § 2.4.1's config block. The example in § 2.7 of "profile updates after the query" becomes "the agent then calls `memory_conclude(...)` to persist a new fact" — explicit, not automatic.

### C.2 § 2.4.1 Budget tracking — config silently falls back to defaults

CONCERNS.md flags `TODO(GAP-012-Tier-2)` in config loader: a typo in `[reasoning]` silently produces the default. With the proposed `monthly_budget_usd = 20.0` default, a typo'd `[reasoning]` section gives the user an *implicit* $20 budget without warning.

**Fix**: budget tracker logs `WARN` at startup with the loaded budget config, including the source (file path + parse result). The user sees in the daemon log:

```
WARN openfang-reasoning: loaded reasoning config from /home/u/.openfang/config.toml: 
     monthly_budget_usd=20.00 (DEFAULT — no [reasoning] section found in config)
```

vs.

```
INFO openfang-reasoning: loaded reasoning config from /home/u/.openfang/config.toml: 
     monthly_budget_usd=50.00 (from config)
```

The user can immediately spot "DEFAULT" vs "from config" and catch the typo.

Bonus fix in same PR: switch config deserialization to `deny_unknown_fields` for the `[reasoning]` block specifically — typos in known sections become errors rather than silent defaults.

### C.3 API key zeroization drift

CONCERNS.md notes [`config.rs`](../../crates/openfang-types/src/config.rs) stores `api_key: String`, not `Zeroizing<String>`, despite SECURITY.md claiming zeroization. Not a Phase 1 deliverable but worth fixing in the same PR window — one-line type change, no behavioural difference.

### C.4 Single Arc<Mutex<Connection>> contention

`SessionStore` and others share one `Arc<Mutex<Connection>>`. FTS5 search latency adds to the critical section. Two paths:

1. **Now**: leave as-is. Phase 1 acceptance criterion: session search returns in <500ms on pc162 for typical histories. If it breaks the budget, escalate.

2. **Later**: switch to a connection pool (`r2d2_sqlite`) so search and writes can proceed in parallel. Out of scope for Phase 1.

---

## D. Sequence change (replaces оригинал § "Последовательность")

Reordered to put schema groundwork before reasoning. FTS5 is now Week 1 because it unblocks A.2 and is the smallest delta to ship.

```
Week 1:    A.1 + A.2 (FTS5 feature flag + v9 migration + dual-write + backfill + tests)
           B.4 (load_bundled defaults for mutable/protected)
Week 2-3:  Skill Self-Patching (registry methods + skill_manage tool + security pipeline)
           B.1 (snapshot refresh signal in agent loop)
Week 4:    session_search tool (depends on Week 1)
Week 5-7:  Reasoning Engine (minimal/low/medium/high; defer max behind approval gate)
           C.2 (budget config logging + deny_unknown_fields)
Week 8:    memory_reason tool + UserProfile (opt-in via C.1) + integration tests + 
           docs/migration notes
```

Schema migration drops to Week 1 to derisk; reasoning engine work (the largest piece) is contiguous in Weeks 5-7 once skill self-patching is shippable.

---

## E. Acceptance check before opening the phase

Before `/gsd:add-phase` and any implementation:

- [ ] `Cargo.toml` rusqlite features set includes `fts5`. `cargo build --workspace --lib` passes.
- [ ] Phase 1 plan ([`gsd:plan-phase`]) reads § A.2.1 v9 migration **before** writing the SkillRegistry methods. FTS5 work blocks nothing downstream once Week 1 lands.
- [ ] Test plan for v8→v9 migration on a populated DB (§ A.3) is in the phase verification doc, not just the unit test file.
- [ ] Snapshot-refresh choice (§ B.1) is explicitly recorded — implementer doesn't have to re-decide.
- [ ] Profile auto-write contradiction (§ C.1) resolved in the phase context doc (the agent picks one rule and sticks to it).
- [ ] CHANGELOG entry drafted: "openfang-memory: schema v9 — FTS5 session search, msgpack-BLOB sessions augmented with flat `session_messages` table for full-text indexing. Backward compatible; existing sessions backfilled on first start after upgrade."

---

## F. What this addendum does NOT change

To be clear, the following from the original document stand without modification:

- Three-component split (Skill Self-Patching / Memory Reasoning / Session FTS5 Search).
- 5-level reasoning model with budget control (`max_level`, `monthly_budget_usd`, `require_approval_for_max`).
- Protected/mutable two-tier skill defense.
- "Additive, backward compatible" principle.
- `openfang-reasoning` as a new crate; skills + memory extensions stay in their existing crates.
- Cost estimates and budget defaults (§ 2.4.1).
- Tool semantics: `skill_manage`, `memory_reason`, `session_search` exposed to the agent.
- Reuse of agent's existing LLM driver via `KernelHandle` for synthesis.

The original document remains the canonical design statement. This addendum corrects the **mechanics**, not the **intent**.
