# Сравнительный анализ: OpenFang vs Hermes Agent System

**Дата**: 2026-06-06  
**Контекст**: оценка возможности дополнить OpenFang до функционального уровня Hermes, с фокусом на self-learning и self-evolving.

---

## 1. Методология

Сравнение по 12 измерениям. Для каждого: что есть в OpenFang (v1.0-dev, Rust, 14 crates), что есть в Hermes (v1.0, Python/Node.js), и gap — что нужно добавить.

---

## 2. Сравнительная таблица

### 2.1 Агентский цикл

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Архитектура | `run_agent_loop()` в openfang-runtime, 5493 строки Rust | `AIAgent.run_conversation()` в run_agent.py, ~12K строк Python | — |
| Макс. итераций | 50 | 90 | **Малый** — увеличить до 90 |
| Retry логика | Exponential backoff, 3 retries | Exponential backoff через provider | — |
| Loop guard | SHA256-based tool loop detection | Нет (полагается на модель) | У OpenFang лучше |
| Session repair | Валидация и авто-исправление истории | Нет | У OpenFang лучше |
| Phantom action detection | Детекция галлюцинаций о действиях | Нет | У OpenFang лучше |
| Stability guidelines | Инжектятся в system prompt | Нет (red lines в промпте) | — |
| Streaming | Да (SSE + WebSocket) | Да | — |
| Continuation limit | 5 | Нет явного | — |

**Вердикт**: OpenFang имеет более продвинутую стабильность агентского цикла (loop guard, session repair, phantom detection). По richness — эквивалентны.

---

### 2.2 Память (Memory)

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| KV-хранилище | Да (SQLite, JSON) | Да (memory tool, SQLite) | — |
| Семантический поиск | Да (векторные эмбеддинги + cosine similarity) | Нет (только FTS5) | В OpenFang богаче |
| Knowledge graph | Да (entity-relation) | Нет | В OpenFang эксклюзив |
| Сессии | Да (conversation history + token tracking) | Да (SessionDB, FTS5) | — |
| Полнотекстовый поиск | Нет | **Да (FTS5)** | **Критический gap** — добавить FTS5 |
| Канонические сессии | Да (cross-channel) | Нет (каналы изолированы) | В OpenFang эксклюзив |
| Task board | Да (общая очередь задач) | Нет (только delegate_task) | В OpenFang эксклюзив |
| Memory decay | Да (consolidation engine) | Нет | В OpenFang эксклюзив |
| LLM-саммаризация | Да (store_llm_summary, block-aware) | Нет | В OpenFang эксклюзив |
| Honcho-style reasoning | **Нет** | **Да (Honcho — active inference о пользователе, от minimal до max reasoning)** | **Критический gap** — самый важный для self-learning |

**Вердикт**: OpenFang имеет более богатую memory-архитектуру (6 слоёв против 3 в Hermes). Но отсутствует аналог Honcho — системы, которая не просто хранит факты, а **активно рассуждает о пользователе**, строит его профиль, выявляет паттерны.

---

### 2.3 Self-Learning / Self-Evolving

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Skill self-patching | **Нет** (только install из marketplace) | **Да (skill_manage — create/patch/delete на лету)** | **Критический gap** |
| Memory self-update | Memory decay (пассивный) | Memory tool + Honcho (активный) | **Средний gap** |
| Self-correction | Loop guard (инструментальный), нет рефлексии | Нет системной рефлексии (но модель может) | — |
| Процедурная память | Skills (60 bundled, статические) | Skills (200+, self-patchable) | **Критический gap** |
| Опыт между сессиями | Канонические сессии + memory decay | Session DB + memory + Honcho | Эквивалентно |
| Обратная связь от пользователя | Нет явного механизма | Нет явного (через Honcho неявно) | — |

**Вердикт**: Это главный разрыв. Hermes позволяет агенту **самому улучшать свои навыки** (skill_manage/patch) на основе опыта. OpenFang такой возможности не имеет — навыки статические, загружаются из marketplace.

---

### 2.4 Координация / Оркестрация

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Subagent spawning | agent_spawn (создание нового агента) | delegate_task (изолированный контекст) | Разные модели |
| Inter-agent messaging | agent_send (сообщение другому агенту) | Нет (только hub-and-spoke) | Разные модели |
| Workflow engine | Да (multi-step, conditional, parallel, loop) | Нет (только ручная декомпозиция) | В OpenFang эксклюзив |
| Trigger engine | Да (event-driven активация) | Нет | В OpenFang эксклюзив |
| Batch delegation | Нет | Да (до 3 параллельных delegate_task) | **Малый gap** |
| Task bus | Task board (SQLite, agent-agnostic) | Нет (контекст через delegate_task) | Разные модели |
| Agent-to-Agent protocol | A2A (Google standard) + OFP (свой p2p) | Нет | В OpenFang эксклюзив |

**Вердикт**: OpenFang имеет более мощную оркестрацию — workflow engine, trigger engine, A2A. Hermes — более простую hub-and-spoke с delegate_task. Это не gap, а разные архитектурные философии. Workflow engine в OpenFang — это как раз то, что мы проектировали для Hermes Company в `docs/coordination.md`.

---

### 2.5 Автономность (Autonomous Operation)

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Background executor | Да (Continuous, Periodic, Proactive) | Да (cronjob) | — |
| Heartbeat monitor | Да (обнаружение зависших агентов) | Нет (полагается на cron/процесс) | В OpenFang эксклюзив |
| Self-prompting | Да (Continuous mode) | Нет | В OpenFang эксклюзив |
| Hands | Да (9 curated autonomous packages) | Нет (skills играют похожую роль) | В OpenFang эксклюзив |
| Cron chaining | Нет | Да (context_from — цепочки задач) | **Малый gap** |
| Approval system | Да (ApprovalManager, auto_approve_autonomous) | Нет (red lines декларативно) | В OpenFang эксклюзив |

**Вердикт**: OpenFang превосходит Hermes по автономности. Hands — это именно то, что Дмитрий хочет: автономные пакеты для self-running агентов. Hermes имеет cron-чейнинг, которого нет в OpenFang.

---

### 2.6 Инструменты (Tools)

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Встроенные инструменты | 23 | 50+ | **Средний gap** — расширить до 50+ |
| MCP client | Да (JSON-RPC 2.0, stdio/SSE) | Да (native MCP) | — |
| MCP server | Да | Да | — |
| Tool auto-discovery | Нет (жёсткая регистрация) | Да (tools/registry.py) | **Средний gap** |
| Tool capability gating | Да (CapabilityManager) | Нет (доверие к модели) | У OpenFang лучше |
| Tool timeout | 120 сек (настраиваемый) | Зависит от инструмента | — |
| WASM sandbox | Да (Wasmtime, dual fuel+epoch metering) | Нет | В OpenFang эксклюзив |
| Web search | 4 провайдера (Tavily/Brave/Perplexity/DDG) | web_search tool | — |
| Web fetch | Да (SSRF protection) | browser tool | — |

**Вердикт**: Hermes имеет больше инструментов (50+ vs 23). MCP позволяет расширять оба. WASM sandbox в OpenFang — эксклюзивная фича.

---

### 2.7 Skills / Навыки

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Количество | 60 bundled | 200+ | **Значительный gap** |
| Skill marketplace | FangHub + ClawHub | Нет | В OpenFang эксклюзив |
| Skill self-patching | **Нет** | **Да** | **Критический gap** |
| SKILL.md совместимость | Да (парсер ClawHub-формата) | Да (нативный) | — |
| PromptOnly skills | Да | Да | — |
| WASM skills | Да | Нет | В OpenFang эксклюзив |
| Python skills | Да (subprocess sandbox) | Да (нативные) | — |
| Node.js skills | Да (OpenClaw compat) | Нет | — |

**Вердикт**: Hermes имеет больше навыков (200+ vs 60). Но главный gap не в количестве, а в **self-patching** — Hermes-агент может улучшать свои навыки на лету, OpenFang — нет.

---

### 2.8 Модели и провайдеры

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Провайдеры | 20 (3 драйвера: Anthropic, Gemini, OpenAI-compat) | 10+ (через плагины) | — |
| Модели в каталоге | 51 | 30+ | — |
| Model router | Да (TaskComplexity scoring) | Да (fallback model) | — |
| Per-agent model | Да | Да | — |
| Cost tracking | Да (MeteringEngine, 20+ model families) | Нет | В OpenFang эксклюзив |
| Token quota | Да (AgentScheduler, per-hour) | Нет (лимиты провайдера) | В OpenFang эксклюзив |

**Вердикт**: OpenFang имеет более продвинутое управление моделями (cost tracking, quotas, model router).

---

### 2.9 Каналы / Мессенджеры

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Количество | 40 адаптеров | 20+ | В OpenFang существенно больше |
| Channel overrides | Да (model, system_prompt, policy, rate_limit) | Частично (platform-specific) | — |
| Rate limiting | Да (per-user DashMap) | Нет | В OpenFang эксклюзив |
| Форматтеры | Да (Markdown → TelegramHTML/SlackMrkdwn/PlainText) | Частично | — |
| Chat commands | Да (/models, /providers, /new, /compact, etc.) | Да (slash commands) | — |

**Вердикт**: OpenFang лидирует по каналам (40 vs 20+).

---

### 2.10 Безопасность

| Аспект | OpenFang | Hermes | Gap |
|--------|----------|--------|-----|
| Capability-based security | Да (CapabilityManager, наследование) | Нет (red lines декларативно) | Разные модели |
| Path traversal protection | Да (safe_resolve_path) | Нет | У OpenFang лучше |
| SSRF protection | Да (is_ssrf_target, is_private_ip) | Нет | У OpenFang лучше |
| Subprocess sandbox | Да (env_clear + selective vars) | Нет (полный доступ) | У OpenFang лучше |
| Merkle audit trail | Да (tamper-evident) | Нет | В OpenFang эксклюзив |
| Taint tracking | Да (TaintLabel, TaintSet) | Нет | В OpenFang эксклюзив |
| Manifest signing | Да (Ed25519) | Нет | В OpenFang эксклюзив |
| Prompt injection scan | Да | Нет | В OpenFang эксклюзив |
| Secret zeroization | Да (Zeroizing<String>) | Нет | В OpenFang эксклюзив |
| GCRA rate limiter | Да | Нет | В OpenFang эксклюзив |

**Вердикт**: OpenFang **радикально превосходит** Hermes по безопасности. 16 систем защиты против декларативных red lines.

---

### 2.11 Self-Evolving / Обучение на опыте

Это ключевой запрос Дмитрия. Детальный разбор:

| Механизм | OpenFang | Hermes | Что нужно добавить в OpenFang |
|----------|----------|--------|------------------------------|
| **Skill improvement** | Только install/upsert из marketplace | skill_manage: create, patch, delete на лету | `skill_update` tool + API, позволяющий агенту патчить skill.md |
| **Memory active inference** | Пассивное хранение фактов + decay | Honcho: reasoning levels, построение профиля пользователя | Добавить `MemoryReasoner` слой над SQLite, способный к инференсу |
| **Session search** | Только семантический поиск (embeddings) | FTS5 полнотекстовый | Добавить FTS5-индекс в SQLite (SQLite уже есть, нужно только расширение) |
| **Self-correction loops** | Loop guard (инструментальный) | Нет системного | Добавить рефлексивный шаг после выполнения задачи |
| **Feedback incorporation** | Нет | Через Honcho неявно | Добавить явный feedback-цикл |
| **Cross-session learning** | Канонические сессии + memory decay | Memory + Honcho + session_search | Эквивалентно |

---

### 2.12 Производительность на слабом железе

| Аспект | OpenFang (Rust, pc162) | Hermes (Python, home2) |
|--------|------------------------|------------------------|
| RAM idle | 116 MB (измерено) | 432 MB |
| RAM пиково | ~200 MB | 600-900 MB |
| Бинарник | Статический, ~20 MB | Python + Node.js, ~500 MB |
| Запуск на pc162 | ✅ Да (1.2 GB свободно) | ❌ Нет |
| CPU load | Низкий (Rust, нативный) | Высокий (Python, интерпретация) |

**Вердикт**: OpenFang — единственный вариант для pc162.

---

## 3. Итоговый Gap Analysis

### Критические gaps (должны быть закрыты)

| # | Gap | Приоритет | Оценка усилий | Эффект |
|---|-----|-----------|---------------|--------|
| 1 | **Skill self-patching** | P0 | 2-3 недели | Агент сможет улучшать свои навыки на лету |
| 2 | **Honcho-style memory reasoning** | P0 | 4-6 недель | Агент будет активно строить модель пользователя |
| 3 | **Session FTS5 search** | P1 | 1 неделя | Полнотекстовый поиск по истории |

### Средние gaps

| # | Gap | Приоритет | Оценка усилий |
|---|-----|-----------|---------------|
| 4 | Tool count (23 → 50+) | P1 | 4-8 недель |
| 5 | Tool auto-discovery | P2 | 2-3 недели |
| 6 | Cron chaining | P2 | 2-3 недели |
| 7 | Batch agent delegation | P2 | 2-3 недели |

### Что в OpenFang УЖЕ лучше (не нужно добавлять)

- ✅ Loop guard + session repair + phantom detection (стабильность лучше)
- ✅ Memory substrate (6 слоёв, богаче)
- ✅ Workflow engine + trigger engine (оркестрация мощнее)
- ✅ Hands (автономные пакеты)
- ✅ Безопасность (16 систем)
- ✅ Каналы (40 vs 20+)
- ✅ Cost tracking + quotas
- ✅ WASM sandbox
- ✅ A2A protocol + OFP p2p
- ✅ Производительность на слабом железе

---

## 4. Дорожная карта: от OpenFang к Self-Evolving агенту

### Фаза 1: Self-learning core (6-8 недель)

1. **Skill self-patching** (P0)
   - Новый инструмент `skill_update` в openfang-skills
   - Агент может читать свой skill.md, патчить, сохранять
   - SHA256-верификация при сохранении
   - Prompt injection scan при каждом обновлении

2. **Session FTS5 search** (P1)
   - Включить FTS5-расширение в SQLite (уже bundled)
   - Создать FTS5-индекс над таблицей sessions
   - Новый инструмент `session_search` для агентов

3. **Honcho-style Memory Reasoning** (P0)
   - Новый крейт `openfang-reasoning`
   - Уровни: minimal/low/medium/high/max
   - Активный вывод о пользователе на основе истории
   - Обновление профиля пользователя в SQLite
   - API: `memory_reason(query, level)` → synthesized answer

### Фаза 2: Расширение инструментов (4-8 недель)

4. **Tool count до 50+**
   - Перенести ключевые инструменты из Hermes, которых нет в OpenFang
   - Приоритет: delegate_task (batch), cronjob (chaining), session_search, skill_manage

5. **Tool auto-discovery**
   - Плагинная система: сканирование директории tools/
   - Регистрация при обнаружении
   - Совместимость с существующими 23 инструментами

### Фаза 3: Координация (4-6 недель)

6. **Batch agent delegation**
   - Параллельный spawn агентов (до 3)
   - Сбор результатов от всех

7. **Cron chaining**
   - `context_from` в cron-задачах
   - Передача выхода одной задачи на вход другой

### Итого: 14-22 недели до полного паритета + self-evolving

---

## 5. Стратегическая рекомендация

### Рекомендация: Дополнить OpenFang, а не портировать Hermes

OpenFang **уже** имеет более сильную архитектуру в ключевых областях:
- Безопасность (16 систем vs декларативные red lines)
- Память (6 слоёв vs 3)
- Оркестрация (workflow engine + triggers vs ручная декомпозиция)
- Производительность на слабом железе (116 MB vs 432 MB)
- Автономность (Hands + BackgroundExecutor + Heartbeat)

Что нужно добавить — три критических компонента для self-learning:
1. **Skill self-patching** (2-3 недели)
2. **Honcho-style memory reasoning** (4-6 недель)
3. **Session FTS5 search** (1 неделя)

После этого OpenFang сравняется с Hermes по self-learning и превзойдёт его по безопасности и автономности. И будет работать на pc162.

### Альтернатива: оставить оба

OpenFang на pc162 (Rust, легковесный, автономный) и Hermes на home2 (Python, богатый инструментарий, оркестратор). Они не конкуренты — они комплементарны. OpenFang может быть специалистом-исполнителем, Hermes — координатором.

Но если цель — автономный self-evolving агент на pc162, то дорожная карта выше — правильный путь.
