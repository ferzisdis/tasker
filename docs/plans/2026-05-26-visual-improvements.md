# Visual Improvements Plan

Date: 2026-05-26

## Цель

Улучшить визуальный стиль приложения: добавить хедер со статистикой и скорректировать цвета рамок карточек.

---

## 1. Хедер со статистикой

### Изменения в `src/ui.rs`

- Добавить строку хедера (1 строка высотой) в верхней части layout в `draw()`
- Текущий layout: `[Constraint::Min(1), Constraint::Length(1)]` (список + хинты)
- Новый layout: `[Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]` (хедер + список + хинты)

### Содержимое хедера

```
 Tasker   Всего: 8   В процессе: 3   Закрыто сегодня: 2
```

- **Всего** — кол-во активных (не удалённых) задач
- **В процессе** — кол-во задач с `in_progress: true`
- **Закрыто сегодня** — кол-во задач с `deleted: true` и `deleted_at == today`

### Изменения в `src/app.rs`

- Добавить поле `deleted_at: Option<NaiveDate>` в `Task`
- Заполнять `deleted_at = Some(today)` в `delete_task()`
- Атрибут `#[serde(default)]` чтобы не ломать существующие задачи без этого поля

---

## 2. Цвета рамок карточек

### Три уровня (приоритет сверху вниз)

| Состояние | Тип рамки | Цвет |
|---|---|---|
| Выбранная | Plain / Thick | `Rgb(255, 165, 0)` — оранжевый (без изменений) |
| In-progress | Thick | `Rgb(100, 150, 200)` — приглушённый стальной синий |
| Обычная | Plain | `DarkGray` — еле видна, фон для текста |

### Изменения в `src/ui.rs`, функция `draw_task_list()`

Текущая логика цвета:
```rust
let border_color = if is_trash {
    if is_selected { Color::Gray } else { Color::DarkGray }
} else if is_selected {
    Color::Rgb(255, 165, 0)
} else {
    Color::Reset  // <-- заменить
};
```

Новая логика:
```rust
let border_color = if is_trash {
    if is_selected { Color::Gray } else { Color::DarkGray }
} else if is_selected {
    Color::Rgb(255, 165, 0)
} else if task.in_progress {
    Color::Rgb(100, 150, 200)  // стальной синий для in-progress
} else {
    Color::DarkGray  // приглушённый для обычных
};
```

---

## Порядок реализации

1. Добавить `deleted_at` в `Task` (`app.rs`)
2. Заполнять `deleted_at` в `delete_task()` (`app.rs`)
3. Скорректировать цвета рамок (`ui.rs`)
4. Добавить хедер: layout + функция `draw_header()` (`ui.rs`)
