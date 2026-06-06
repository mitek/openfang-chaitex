# ChaiTex Fork of OpenFang

Это форк [OpenFang](https://github.com/RightNow-AI/openfang) — Agent Operating System на Rust, сделанный для разработки self-learning функциональности в интересах компании ChaiTex (Дмитрий Шилов, CTO).

## Почему форк

OpenFang — единственная агентская система, способная работать на слабом ARM-железе (pc162: Allwinner Cortex-A7, 1.9 GB RAM). Потребление: 116 MB против 432 MB у Hermes. При этом архитектура OpenFang в ключевых аспектах превосходит Hermes: 16 систем безопасности, 6 слоёв памяти, workflow engine, 40 каналов.

Цель форка — добавить в OpenFang возможности self-learning, которые есть в Hermes Agent System (Nous Research), но отсутствуют в upstream OpenFang.

## Дорожная карта

### Фаза 1: Self-Learning Core (6-8 недель)

Три компонента:

1. **Skill Self-Patching** — агент может создавать, патчить и удалять свои навыки на лету (аналог `skill_manage` в Hermes)
2. **Memory Reasoning** — активный вывод о пользователе на основе накопленной истории (аналог Honcho в Hermes)
3. **Session FTS5 Search** — полнотекстовый поиск по всей истории диалогов (аналог `session_search` в Hermes)

Детальный архитектурный дизайн: [`docs/chaitex/phase1-self-learning-architecture.md`](docs/chaitex/phase1-self-learning-architecture.md)
Корректировки по результатам codebase mapping: [`docs/chaitex/phase1-addendum-codebase-grounding.md`](docs/chaitex/phase1-addendum-codebase-grounding.md)

### Фаза 2: Расширение инструментов (4-8 недель)

- Увеличение числа встроенных инструментов с 23 до 50+
- Auto-discovery инструментов
- Cron chaining

### Фаза 3: Координация (4-6 недель)

- Batch agent delegation
- Улучшенная оркестрация

## Документы

| Документ | Описание |
|----------|----------|
| [phase1-self-learning-architecture.md](docs/chaitex/phase1-self-learning-architecture.md) | Архитектурный дизайн Фазы 1: Skill Self-Patching, Memory Reasoning, FTS5 |
| [phase1-addendum-codebase-grounding.md](docs/chaitex/phase1-addendum-codebase-grounding.md) | Аддендум: правки дизайна Фазы 1 по результатам реального чтения кода (FTS5 feature flag, BLOB sessions → flat table + v9 migration, tool-dispatch pattern, snapshot refresh) |
| [openfang-vs-hermes-analysis.md](docs/chaitex/openfang-vs-hermes-analysis.md) | Полный сравнительный анализ OpenFang vs Hermes по 12 измерениям |
| [hermes-on-pc162-rust-port.md](docs/chaitex/hermes-on-pc162-rust-port.md) | Анализ возможности запуска Hermes на pc162 и перспективы Rust-порта |

## Связанные репозитории

- [hermes-company](https://github.com/mitek/hermes-company) — оргструктура агентской компании ChaiTex
- [RightNow-AI/openfang](https://github.com/RightNow-AI/openfang) — upstream репозиторий

## Контакты

Дмитрий Шилов, CTO ООО «Чайтекс»
