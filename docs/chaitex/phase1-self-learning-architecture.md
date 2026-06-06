# Фаза 1: Self-Learning для OpenFang — Архитектурный дизайн

**Дата**: 2026-06-06  
**Статус**: Проект архитектуры  
**Цель**: Спроектировать два ключевых компонента self-learning для OpenFang, вдохновлённых Hermes, но реализованных в Rust-архитектуре OpenFang.

---

## Компонент 1: Skill Self-Patching

### Что это

Возможность агента создавать, патчить и удалять свои навыки (skills) на лету, без перезагрузки системы. Аналог `skill_manage` в Hermes.

### Текущее состояние OpenFang

`SkillRegistry` (`crates/openfang-skills/src/registry.rs`):
- `load_bundled()` — загрузка 60 встроенных навыков при старте
- `load_all()` / `load_workspace_skills()` — загрузка из директории
- `load_skill()` — загрузка одного навыка из skill.toml
- `remove()` — удаление навыка (директория + реестр)
- `get()`, `list()`, `count()` — чтение

**Отсутствует**: возможность изменить skill.toml или prompt_context.md существующего навыка и перезагрузить его без рестарта.

### Архитектурный дизайн

#### 1.1 Новый крейт: `openfang-skill-patch`

Не нужно создавать отдельный крейт. Всё добавляется в существующий `openfang-skills`.

#### 1.2 Новые методы `SkillRegistry`

```rust
impl SkillRegistry {
    /// Create a new skill from raw content, writing to disk and registering.
    /// Returns the skill name on success.
    pub fn create_skill(
        &mut self,
        name: &str,
        toml_content: &str,
        prompt_context: &str,
        category: Option<&str>,
    ) -> Result<String, SkillError>;

    /// Patch an existing skill by replacing old_string with new_string
    /// in the skill.toml file. Automatically re-verifies and reloads.
    pub fn patch_skill(
        &mut self,
        name: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool,
    ) -> Result<(), SkillError>;

    /// Replace the entire skill.toml content for an existing skill.
    /// Re-verifies prompt injection, re-validates manifest, reloads.
    pub fn edit_skill(
        &mut self,
        name: &str,
        toml_content: &str,
    ) -> Result<(), SkillError>;

    /// Write or overwrite a supporting file in the skill directory
    /// (e.g., references/api-guide.md, scripts/helper.py).
    pub fn write_skill_file(
        &self,
        name: &str,
        file_path: &str,  // relative to skill dir
        content: &str,
    ) -> Result<(), SkillError>;

    /// Reload a single skill from disk (after external modification).
    pub fn reload_skill(&mut self, name: &str) -> Result<(), SkillError>;

    /// Enable/disable a skill without removing it.
    pub fn set_skill_enabled(&mut self, name: &str, enabled: bool) -> Result<(), SkillError>;
}
```

#### 1.3 Security Pipeline (при каждом изменении)

```
1. Prompt injection scan (SkillVerifier::scan_prompt_content)
   → CRITICAL threats → REJECT
   → WARNING threats → log + accept

2. SHA256 hash computation + Merkle audit record

3. TOML validation (parse manifest)

4. Config resolution (apply_skill_config)

5. Atomic file write (write to .tmp → rename)

6. Reload in registry (update HashMap entry)

7. Emit SkillUpdated event on EventBus
```

#### 1.4 Инструмент для агента: `skill_manage`

Новый встроенный инструмент в `openfang-runtime/src/tools/`:

```rust
// tools/skill_manage.rs

pub struct SkillManageTool;

#[async_trait]
impl Tool for SkillManageTool {
    fn name(&self) -> &str { "skill_manage" }
    fn description(&self) -> &str {
        "Manage skills (create, patch, edit, delete, write_file). \
         Skills are your procedural memory — reusable approaches \
         for recurring task types."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "patch", "edit", "delete", "write_file", "remove_file", "list"]
                },
                "name": { "type": "string", "description": "Skill name" },
                "content": { "type": "string", "description": "Full skill.toml content (for create/edit)" },
                "prompt_context": { "type": "string", "description": "Prompt context markdown (for create)" },
                "old_string": { "type": "string", "description": "Text to find and replace (for patch)" },
                "new_string": { "type": "string", "description": "Replacement text (for patch)" },
                "replace_all": { "type": "boolean", "description": "Replace all occurrences" },
                "file_path": { "type": "string", "description": "Relative path within skill dir" },
                "file_content": { "type": "string", "description": "Content for file write" },
                "category": { "type": "string", "description": "Category for new skill" }
            },
            "required": ["action", "name"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult;
}
```

#### 1.5 Пример использования агентом

Агент осознаёт, что ему нужен новый подход к решению задач с PDF:

```
Agent: skill_manage(action="create", name="pdf-reader-v2",
  content='[skill]\nname = "pdf-reader-v2"\n...',
  prompt_context="# PDF Reader v2\n\nAlways check for OCR layer first...")
```

Позже агент улучшает навык:

```
Agent: skill_manage(action="patch", name="pdf-reader-v2",
  old_string="check for OCR layer first",
  new_string="check for OCR layer first, fall back to Tesseract if missing")
```

#### 1.6 Интеграция с существующим кодом

| Файл | Изменения |
|------|-----------|
| `crates/openfang-skills/src/registry.rs` | +6 методов (create_skill, patch_skill, edit_skill, write_skill_file, reload_skill, set_skill_enabled) |
| `crates/openfang-skills/src/lib.rs` | Экспорт новых методов |
| `crates/openfang-runtime/src/tools/skill_manage.rs` | **Новый файл** — инструмент для агента |
| `crates/openfang-runtime/src/tools/mod.rs` | Регистрация skill_manage |
| `crates/openfang-kernel/src/kernel.rs` | Поддержка SkillUpdated event |

**Оценка**: ~800 строк Rust, 2-3 недели.

#### 1.7 Migration Path и Protected Skills

**Проблема**: После добавления `patch_skill` все 60 существующих навыков становятся мутабельными. Агент может случайно или злонамеренно испортить системные навыки, от которых зависит его собственное функционирование.

**Решение**: Двухуровневая защита через поля в `skill.toml`.

##### 1.7.1 Поля в skill.toml

```toml
[skill]
name = "session-search"
version = "1.0.0"
# Флаг мутабельности — можно ли патчить/edit/delete навык
mutable = false
# Если true — навык вообще нельзя удалить или изменить,
# даже с явного подтверждения пользователя
protected = true
```

##### 1.7.2 Дефолты для разных категорий

| Категория навыка | `mutable` | `protected` | Комментарий |
|---|---|---|---|
| Системные (bundled, ядро агента) | `false` | `true` | Навыки от которых зависит работа агента: tool dispatch, memory, session management |
| Встроенные (bundled, утилиты) | `false` | `false` | Навыки из поставки: pdf-reader, code-review, github-pr, etc. Могут быть изменены при явном согласии пользователя |
| Пользовательские (созданы агентом) | `true` | `false` | Создаются через `create_skill`, полностью мутабельны |

##### 1.7.3 Проверки в SkillRegistry

```rust
impl SkillRegistry {
    fn check_mutable(&self, name: &str, action: &str) -> Result<(), SkillError> {
        let skill = self.get(name)?;

        // Protected skills: полный запрет
        if skill.config.protected {
            return Err(SkillError::ProtectedSkill {
                name: name.to_string(),
                action: action.to_string(),
                hint: "Этот навык защищён от изменений. "
                      "Для разблокировки измените protected = false в skill.toml "
                      "и перезагрузите агента.".to_string(),
            });
        }

        // Immutable skills: запрет без подтверждения
        if !skill.config.mutable {
            return Err(SkillError::ImmutableSkill {
                name: name.to_string(),
                action: action.to_string(),
                hint: "Навык помечен как immutable. "
                      "Измените mutable = true в skill.toml для разрешения.".to_string(),
            });
        }

        Ok(())
    }
}
```

##### 1.7.4 Migration Path

Текущее состояние: 60 навыков в `bundled/`, ни в одном нет полей `mutable`/`protected`.

**Шаг 1 (автоматический при сборке)**: Скрипт `scripts/protect-system-skills.sh`:
```bash
#!/bin/bash
# Добавляет mutable = false, protected = true в skill.toml
# для навыков из списка SYSTEM_SKILLS

SYSTEM_SKILLS=(
    "tool-dispatch"
    "memory-core"
    "session-manager"
    "skill-manage"      # критично: защищает сам механизм патчинга
    "event-bus"
    "kernel-api"
)

for skill in "${SYSTEM_SKILLS[@]}"; do
    if grep -q "^protected" "bundled/$skill/skill.toml"; then
        continue  # уже размечен
    fi
    echo -e "\nmutable = false\nprotected = true" >> "bundled/$skill/skill.toml"
done
```

**Шаг 2 (автоматический при сборке)**: Для остальных bundled-навыков:
```bash
# Добавляет mutable = false (без protected) в остальные skill.toml
for skill_toml in bundled/*/skill.toml; do
    if grep -q "^mutable" "$skill_toml"; then
        continue
    fi
    echo -e "\nmutable = false" >> "$skill_toml"
done
```

**Шаг 3 (в коде)**: `SkillRegistry::load_bundled()` при загрузке навыка без полей `mutable`/`protected`:
- Если навык в списке `SYSTEM_SKILLS` → `mutable = false, protected = true`
- Иначе → `mutable = false, protected = false`

**Шаг 4 (для пользователя)**: Явная документация — какие навыки protected и почему, как разблокировать.

##### 1.7.5 Список protected-навыков (предварительный)

| Навык | Причина защиты |
|---|---|
| `skill-manage` | Защищает сам механизм патчинга от компрометации |
| `tool-dispatch` | Ядро диспетчеризации инструментов |
| `memory-core` / `memory-reason` | Базовые операции с памятью |
| `session-manager` / `session-search` | Управление сессиями |
| `event-bus` / `kernel-api` | Системная шина событий |
| `security-scanner` / `prompt-injection` | Собственная безопасность агента |

##### 1.7.6 Что будет при попытке патча protected-навыка

```json
{
  "error": "ProtectedSkill",
  "name": "skill-manage",
  "action": "patch",
  "hint": "Этот навык защищён от изменений. Для разблокировки измените protected = false в skill.toml и перезагрузите агента."
}
```

Агент **не может** изменить `protected = false` через `patch_skill`, потому что этот вызов будет заблокирован раньше, чем дойдёт до файловой операции.

---

## Компонент 2: Memory Reasoning (Honcho-style)

### Что это

Система активного вывода о пользователе на основе накопленной истории. Не просто хранение фактов (как сейчас в MemorySubstrate), а способность задавать вопросы вроде «Что пользователь предпочитает?», «Какие паттерны поведения?», «Что изменилось за последний месяц?» и получать синтезированные ответы.

### Текущее состояние OpenFang

`MemorySubstrate` (`crates/openfang-memory/src/substrate.rs`):
- 6 слоёв хранения: KV, semantic search (векторы), knowledge graph, sessions, task board, canonical sessions
- `ConsolidationEngine` — пассивный decay старых воспоминаний
- `store_llm_summary` — сохранение LLM-саммари

**Отсутствует**: активный reasoning — способность делать выводы из накопленных данных, а не просто возвращать их.

### Архитектурный дизайн

#### 2.1 Новый крейт: `openfang-reasoning`

```rust
// crates/openfang-reasoning/Cargo.toml
[package]
name = "openfang-reasoning"
version = "0.1.0"
edition = "2024"

[dependencies]
openfang-types = { path = "../openfang-types" }
openfang-memory = { path = "../openfang-memory" }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }
```

#### 2.2 Reasoning Engine

```rust
// crates/openfang-reasoning/src/engine.rs

/// Уровни глубины рассуждения.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ReasoningLevel {
    /// Быстрый фактологический поиск (имя, роль, простые предпочтения)
    Minimal,
    /// Прямые вопросы с ясными ответами
    Low,
    /// Многоаспектные вопросы, требующие синтеза
    Medium,
    /// Сложные поведенческие паттерны, противоречия
    High,
    /// Максимально глубокий анализ — аудит-уровень
    Max,
}

/// Аргументы для reasoning-запроса.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReasoningQuery {
    /// Естественно-языковой вопрос
    pub query: String,
    /// Уровень глубины
    pub level: ReasoningLevel,
    /// ID агента (чей контекст анализируем)
    pub agent_id: Option<AgentId>,
    /// Максимальное количество возвращаемых фактов
    pub max_facts: Option<usize>,
}

/// Результат reasoning.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReasoningResult {
    /// Синтезированный ответ на вопрос
    pub answer: String,
    /// Факты, использованные для вывода
    pub supporting_facts: Vec<FactReference>,
    /// Уверенность (0.0 — 1.0)
    pub confidence: f32,
    /// Использованный уровень
    pub level: ReasoningLevel,
    /// Предупреждения (например, «недостаточно данных»)
    pub caveats: Vec<String>,
}

/// Ссылка на факт, использованный в reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactReference {
    pub source: FactSource,
    pub content: String,
    pub relevance: f32,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactSource {
    Memory { memory_id: String },
    Session { session_id: String, message_index: usize },
    KnowledgeGraph { entity_id: String },
    StructuredKv { key: String },
}
```

#### 2.3 Reasoning Engine Core

```rust
/// Основной движок reasoning.
pub struct ReasoningEngine {
    memory: Arc<MemorySubstrate>,
    /// Модель для синтеза ответов (опционально — может использовать
    /// тот же LLM, что и агент, через KernelHandle)
    llm: Option<Arc<dyn ReasoningLlm>>,
}

/// Трейт для LLM-синтеза ответов reasoning.
#[async_trait]
pub trait ReasoningLlm: Send + Sync {
    async fn synthesize(
        &self,
        query: &str,
        facts: &[FactReference],
        level: ReasoningLevel,
    ) -> Result<String, ReasoningError>;
}

impl ReasoningEngine {
    pub fn new(memory: Arc<MemorySubstrate>) -> Self {
        Self { memory, llm: None }
    }

    pub fn with_llm(mut self, llm: Arc<dyn ReasoningLlm>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// Выполнить reasoning-запрос.
    pub async fn reason(&self, query: ReasoningQuery) -> Result<ReasoningResult, ReasoningError> {
        match query.level {
            ReasoningLevel::Minimal => self.reason_minimal(&query).await,
            ReasoningLevel::Low => self.reason_low(&query).await,
            ReasoningLevel::Medium | ReasoningLevel::High | ReasoningLevel::Max => {
                self.reason_deep(&query).await
            }
        }
    }
}
```

#### 2.4 Уровни reasoning

##### Minimal (без LLM, только поиск)

```
1. Поиск по structured KV (точное совпадение ключа)
2. Поиск по knowledge graph (entity lookup)
3. Если найдено → вернуть как факты без синтеза
4. Если не найдено → semantic search (cosine similarity)
```

##### Low (лёгкий синтез)

```
1. Semantic search (векторный + FTS5) → top 5 фактов
2. Session search → релевантные сообщения
3. Лёгкий синтез через LLM (простой промпт: «На основе этих фактов ответь на вопрос»)
```

##### Medium/High/Max (глубокий анализ)

```
1. Multi-source retrieval:
   - Semantic search → top 10
   - Session search → последние N сессий по теме
   - Knowledge graph → связанные сущности
   - Structured KV → релевантные ключи
2. Cross-reference фактов (противоречия, подтверждения)
3. Temporal analysis (изменения во времени)
4. Pattern detection (повторяющиеся темы, evolving preferences)
5. Глубокий синтез через LLM (chain-of-thought промпт)
6. Confidence scoring
```

##### 2.4.1 Бюджет reasoning и стоимость

**Проблема**: Max reasoning при 100K+ токенов контекста + chain-of-thought может стоить $0.5–1.0 за один запрос. Пользователь должен это понимать и иметь контроль.

##### Оценка потребления токенов по уровням

| Уровень | LLM-вызовов | Входные токены | Выходные токены | Примерная стоимость* |
|---|---|---|---|---|
| **Minimal** | 0 | 0 | 0 | $0.00 |
| **Low** | 1 | ~1K–2K | ~200–500 | $0.003–0.01 |
| **Medium** | 1–2 | ~3K–8K | ~500–1.5K | $0.01–0.05 |
| **High** | 2–3 | ~8K–25K | ~1.5K–5K | $0.05–0.25 |
| **Max** | 3–5 | ~25K–80K+ | ~5K–20K+ | $0.25–1.00+ |

*\*По тарифам GPT-4o / Claude Sonnet 4 на июнь 2026: ~$2.50–3.75/1M input, ~$10–15/1M output. Фактическая стоимость зависит от провайдера и модели.*

##### Расчёт для Max reasoning (худший случай)

```
Вход:  80,000 токенов
  - FTS5 search results:       ~15K токенов (сессии, факты, граф)
  - Semantic search:            ~10K токенов
  - User profile:               ~5K токенов
  - Chain-of-thought промпт:    ~2K токенов
  - Накопленный контекст:       ~48K токенов (история диалога)

Выход: 15,000 токенов
  - Chain-of-thought рассуждения: ~8K токенов
  - Синтезированный ответ:        ~5K токенов
  - Confidence scoring + JSON:    ~2K токенов

Стоимость:
  Input:  80K × $3.75/1M  = $0.30
  Output: 15K × $15.00/1M  = $0.23
  Итого: ~$0.53 за один Max-reasoning запрос

При 5 итерациях chain-of-thought (Max уровень):
  $0.53 × 5 = $2.65 за один reasoning-запрос
```

##### Факторы, увеличивающие стоимость

1. **Размер контекста**. Если агент работает с 200K+ историей, входные токены утраиваются.
2. **Число источников**. High/Max опрашивают 4 источника (semantic + FTS5 + graph + KV).
3. **Chain-of-thought глубина**. Max делает 3-5 итераций рассуждения.
4. **Модель провайдера**. GPT-4o vs Claude Opus vs локальная модель — разница в 10–50×.

##### Механизмы контроля для пользователя

**1. Конфигурация в `config.toml`:**

```toml
[reasoning]
# Лимит токенов на один reasoning-запрос
max_input_tokens = 40000
max_output_tokens = 8000

# Максимальный разрешённый уровень (minimal/low/medium/high/max)
max_level = "high"

# Месячный бюджет на reasoning (в USD)
monthly_budget_usd = 50.0

# Действие при превышении бюджета: "warn" | "block"
budget_exceeded_action = "warn"

# Разрешить Max уровень только с явного подтверждения
require_approval_for_max = true
```

**2. Предупреждения агенту в системном промпте:**

```
[Budget Awareness]
- Minimal/Low reasoning: используй свободно
- Medium reasoning: не чаще 10 раз за сессию
- High reasoning: не чаще 3 раз за сессию
- Max reasoning: только по явному запросу пользователя
- При превышении месячного бюджета reasoning отключается до конца периода
```

**3. Runtime-проверки в `ReasoningEngine`:**

```rust
impl ReasoningEngine {
    async fn reason(&self, query: ReasoningQuery) -> Result<ReasoningResult, ReasoningError> {
        // Проверка бюджета
        let spent = self.budget_tracker.current_month_spent().await;
        if spent >= self.config.monthly_budget_usd {
            return match self.config.budget_exceeded_action {
                BudgetAction::Warn => {
                    tracing::warn!("Monthly reasoning budget exceeded (${:.2})", spent);
                    // Продолжаем, но с понижением до Low
                    self.reason_low(&query).await
                }
                BudgetAction::Block => {
                    Err(ReasoningError::BudgetExceeded {
                        spent,
                        limit: self.config.monthly_budget_usd,
                    })
                }
            };
        }

        // Проверка уровня
        if query.level > self.config.max_level {
            return Err(ReasoningError::LevelNotAllowed {
                requested: query.level,
                max_allowed: self.config.max_level,
            });
        }

        // Max требует подтверждения
        if query.level == ReasoningLevel::Max && self.config.require_approval_for_max {
            return Err(ReasoningError::ApprovalRequired {
                level: "max",
                estimated_cost: self.estimate_cost(&query),
            });
        }

        // ... выполнение
    }
}
```

**4. Трекинг и отчётность:**

```rust
pub struct BudgetTracker {
    monthly_budget: f64,
    current_month_spent: AtomicF64,
    query_history: Vec<BudgetRecord>,
}

pub struct BudgetRecord {
    pub timestamp: String,
    pub level: ReasoningLevel,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost: f64,
    pub query_preview: String,  // первые 100 символов запроса
}
```

##### Рекомендации по дефолтам

| Параметр | Дефолт | Обоснование |
|---|---|---|
| `max_level` | `"high"` | Max только осознанно, не по умолчанию |
| `monthly_budget_usd` | `20.0` | Разумный старт для индивидуального пользователя |
| `require_approval_for_max` | `true` | Защита от неожиданных счетов |
| `max_input_tokens` | `40000` | Достаточно для medium/high, ограничивает Max |
| `budget_exceeded_action` | `"warn"` | Не блокировать жёстко, но предупреждать |

#### 2.5 FTS5 для полнотекстового поиска

Расширение существующей миграции SQLite (schema v6):

```sql
-- Добавляем FTS5 для полнотекстового поиска по сессиям
CREATE VIRTUAL TABLE IF NOT EXISTS session_messages_fts USING fts5(
    session_id,
    agent_id,
    role,
    content,
    tokenize='porter unicode61'
);

-- Триггеры для авто-обновления FTS индекса
CREATE TRIGGER IF NOT EXISTS session_messages_fts_insert
    AFTER INSERT ON messages
BEGIN
    INSERT INTO session_messages_fts(session_id, agent_id, role, content)
    VALUES (new.session_id, new.agent_id, new.role, new.content);
END;
```

#### 2.6 Инструмент для агента: `memory_reason`

```rust
// tools/memory_reason.rs

pub struct MemoryReasonTool;

#[async_trait]
impl Tool for MemoryReasonTool {
    fn name(&self) -> &str { "memory_reason" }
    fn description(&self) -> &str {
        "Ask questions about the user based on accumulated memory. \
         Returns synthesized answers, not just raw facts. \
         Use reasoning_level to control depth: \
         minimal (fast/cheap), low, medium, high, max (deep/expensive)."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Natural language question" },
                "reasoning_level": {
                    "type": "string",
                    "enum": ["minimal", "low", "medium", "high", "max"],
                    "description": "Depth of analysis"
                },
                "max_facts": { "type": "integer", "description": "Max supporting facts to return" }
            },
            "required": ["query"]
        })
    }
}
```

#### 2.7 Пример использования агентом

```
Agent: memory_reason(
    query="Как пользователь предпочитает получать информацию: кратко или подробно?",
    reasoning_level="medium"
)

→ {
    answer: "Пользователь предпочитает краткие ответы с возможностью углубиться.
            В 8 из 10 последних сессий он явно просил 'короче' или прерывал
            длинные объяснения. Однако в технических вопросах (архитектура,
            инфраструктура) ожидает полного анализа.",
    confidence: 0.87,
    supporting_facts: [
        { source: "Session/abc123", content: "короче, дай суть", relevance: 0.95 },
        { source: "Session/def456", content: "распиши подробно архитектуру", relevance: 0.82 },
        ...
    ]
}
```

#### 2.8 Профиль пользователя (User Profile)

Отдельная структура для хранения выводов о пользователе:

```rust
/// Эволюционирующий профиль пользователя.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub agent_id: AgentId,
    pub facts: Vec<UserFact>,
    pub preferences: HashMap<String, Preference>,
    pub patterns: Vec<BehavioralPattern>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFact {
    pub fact: String,
    pub confidence: f32,
    pub source: FactSource,
    pub first_observed: String,
    pub last_confirmed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preference {
    pub key: String,        // "response_style", "language", "detail_level"
    pub value: String,      // "concise", "russian", "technical"
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralPattern {
    pub pattern: String,    // "Прерывает длинные ответы"
    pub occurrences: u32,
    pub first_seen: String,
    pub last_seen: String,
}
```

Профиль сохраняется в structured KV с ключом `__user_profile__` и обновляется после каждого reasoning-запроса уровня medium+.

#### 2.9 Интеграция с существующим кодом

| Файл | Изменения |
|------|-----------|
| `crates/openfang-reasoning/` | **Новый крейт** (engine.rs, profile.rs, fts5.rs, llm_synthesizer.rs) |
| `crates/openfang-reasoning/Cargo.toml` | **Новый файл** |
| `crates/openfang-memory/src/migration.rs` | +FTS5 виртуальная таблица (v6) |
| `crates/openfang-runtime/src/tools/memory_reason.rs` | **Новый файл** — инструмент |
| `crates/openfang-runtime/src/tools/memory_profile.rs` | **Новый файл** — чтение/запись профиля |
| `crates/openfang-kernel/src/kernel.rs` | Инициализация ReasoningEngine |
| `crates/openfang-kernel/Cargo.toml` | +openfang-reasoning dependency |

**Оценка**: ~1500 строк Rust, 4-6 недель.

---

## Компонент 3: Session FTS5 Search (инструмент)

### Что это

Полнотекстовый поиск по всей истории диалогов. Аналог `session_search` в Hermes.

### Реализация

Добавляется в рамках Компонента 2 (FTS5 уже нужен для reasoning).

```rust
// tools/session_search.rs

pub struct SessionSearchTool;

#[async_trait]
impl Tool for SessionSearchTool {
    fn name(&self) -> &str { "session_search" }
    fn description(&self) -> &str {
        "Full-text search across all past conversation sessions. \
         Use to recall what was discussed, decided, or discovered."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "FTS5 search query" },
                "limit": { "type": "integer", "description": "Max results (default 5)" },
                "agent_id": { "type": "string", "description": "Filter to specific agent" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        // FTS5 query → ranked results with context snippets
        // Returns: [{ session_id, timestamp, snippet, relevance }]
    }
}
```

**Оценка**: ~200 строк Rust, 1 неделя (в составе Компонента 2).

---

## Сводка Фазы 1

| Компонент | Крейт | Строк Rust | Время | Приоритет |
|-----------|-------|-----------|-------|-----------|
| **Skill Self-Patching** | openfang-skills (расширение) | ~1200 | 2-3 нед. | P0 |
| **Memory Reasoning** | openfang-reasoning (новый) | ~2000 | 4-6 нед. | P0 |
| **Session FTS5 Search** | openfang-memory (расширение) | ~200 | 1 нед. | P1 |
| **Инструменты для агента** | openfang-runtime (3 новых tools) | ~600 | 2 нед. | P0 |
| **Migration scripts + Budget tracker** | scripts/ + reasoning/budget.rs | ~400 | 1 нед. | P0 |
| **Итого** | | ~4400 | 6-8 нед. | |

### Последовательность

```
Week 1-2:  Skill Self-Patching (registry methods + skill_manage tool + security)
Week 3:    FTS5 migration + session_search tool
Week 4-6:  Reasoning Engine (minimal/low/medium/high/max levels)
Week 7-8:  UserProfile + memory_profile tool + integration tests
```

### Ключевые архитектурные решения

1. **Не отдельный сервис, а крейты внутри workspace.** Всё компилируется в один бинарник. Никаких внешних зависимостей.

2. **Reasoning использует существующий LLM-драйвер.** Не нужен отдельный LLM-клиент. Reasoning engine получает `KernelHandle`, через который вызывает ту же модель, что и агент.

3. **FTS5 — bundled в SQLite.** Уже есть в rusqlite с фичей `bundled`. Не нужна отдельная БД.

4. **Security first.** Каждый patch/update навыка проходит prompt injection scan. Reasoning engine не пишет в память автоматически — только через явный `memory_conclude`.

5. **Backward compatible.** Все новые методы — additive. Существующие навыки и память продолжают работать без изменений.

6. **Protected skills + mutable flag.** Двухуровневая защита от accidental self-modification: `protected` (полный запрет) и `mutable` (opt-in). По умолчанию все существующие навыки иммутабельны — миграция явная через скрипты.

7. **Reasoning budget control.** Пользователь контролирует стоимость через: лимит уровня (max_level), месячный бюджет ($20 дефолт), approval на Max, прозрачный трекинг потраченных токенов и долларов.
