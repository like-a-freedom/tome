# ADR-0002: Single YAML Parser (Eliminate Fallback)

## Status
Accepted — implemented with frontmatter preprocessing before `serde_yaml`

## Context
`core/spec.rs` реализует два парсера YAML frontmatter:
1. Строгий `serde_yaml::from_str()` (основной)
2. Кастомный fallback-парсер `parse_frontmatter_fallback()` (~100 строк)

Fallback нужен для обработки описаний, содержащих `: ` — символы, которые ломают строгий парсер. Однако fallback-парсер сам не обрабатывает все edge-case'ы (см. тест на строке 348: `fm.description[..fm.description.len().min(200)]`).

## Decision
Заменить dual-parser на preprocessing-подход:
1. Извлечь YAML-секцию из SKILL.md (между `---` разделителями)
2. Применить preprocessing — экранировать значения ключа `description`, содержащие `: ` (пробел после двоеточия)
3. Передать результат в `serde_yaml::from_str()`

Fallback-парсер полностью удаляется (~100 строк).

## Consequences

### Положительные
- spec.rs уменьшается с ~385 до ~120 строк (уменьшение на 69%)
- serde_yaml — зрелая, хорошо протестированная библиотека
- Один парсер вместо двух с дублирующейся логикой валидации (name/description)
- Устраняется известное edge-case поведение fallback

### Отрицательные
- Требует написания preprocessing-логики (экранирование значений `description`)
- Необходимо пересмотреть существующие тесты fallback (удаляются)

### Совместимость с ADR-0001
- Не влияет: preprocessing — это часть spec module, а не service layer

## Files Affected
- `src/core/spec.rs` (385 → ~120 lines)
