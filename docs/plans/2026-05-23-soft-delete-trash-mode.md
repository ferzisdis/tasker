# Plan: Soft Delete и режим корзины

## Цель

1. Исправить баг: удалённые задачи не сохраняются — появляются снова после перезапуска.
2. Добавить мягкое удаление (soft delete): задачи помечаются удалёнными, но остаются в JSON.
3. Добавить режим корзины (`t`): показывает только удалённые задачи с возможностью восстановить или удалить навсегда.
4. При старте приложения и при входе в режим корзины — скролл и курсор на последнюю задачу.

---

## Шаг 1 — Модель данных (`src/app.rs`)

### 1.1 Обновить структуру `Task`

Добавить поле `deleted: bool` с дефолтным значением `false` (для обратной совместимости со старыми JSON):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub text: String,
    pub in_progress: bool,
    pub created_at: NaiveDate,
    #[serde(default)]
    pub deleted: bool,
}
```

### 1.2 Обновить перечисление `Mode`

Добавить вариант `Trash`:

```rust
pub enum Mode {
    Normal,
    Add,
    Edit,
    Trash,
}
```

### 1.3 Обновить структуру `App`

Добавить флаг скролла вниз:

```rust
pub struct App {
    pub tasks: Vec<Task>,
    pub selected: usize,
    pub scroll_offset: u16,
    pub mode: Mode,
    pub next_id: u64,
    pub card_rows: Vec<(u16, u16)>,
    pub needs_scroll_to_bottom: bool,   // NEW
}
```

В `App::new()` установить `needs_scroll_to_bottom: true` — чтобы при старте скролл шёл вниз.

### 1.4 Обновить методы `App`

**`delete_task`** — мягкое удаление вместо `Vec::remove`:
```rust
pub fn delete_task(&mut self, index: usize) {
    // index — позиция в отфильтрованном списке visible_tasks()
    if let Some(task) = self.visible_tasks_mut().nth(index) {
        task.deleted = true;
    }
    // скорректировать selected, если он вышел за границу
}
```

**`restore_task`** — снять флаг `deleted` (для режима корзины):
```rust
pub fn restore_task(&mut self, index: usize) {
    // index — позиция в отфильтрованном списке deleted_tasks()
    ...
}
```

**`purge_task`** — физически удалить из `tasks` (безвозвратно, из корзины):
```rust
pub fn purge_task(&mut self, index: usize) {
    ...
}
```

**Вспомогательный метод** — индексы видимых задач (чтобы не хранить отдельный Vec):
```rust
pub fn visible_indices(&self) -> Vec<usize> {
    // Normal/Add/Edit: задачи где !deleted
    // Trash: задачи где deleted
}
```

---

## Шаг 2 — Хранилище (`src/storage.rs`)

Изменений не требуется. Сохраняем `app.tasks` как есть — `deleted: true` записывается в JSON автоматически через serde.

---

## Шаг 3 — Ввод (`src/input.rs`)

### 3.1 Обновить `handle_normal`

Добавить обработку клавиш:

| Клавиша | Действие |
|---------|----------|
| `t` / `е` (рус.) | Переключить режим Normal ↔ Trash; установить `needs_scroll_to_bottom = true` |
| `d` / `в` | Soft delete (пометить `deleted = true`) + немедленно сохранить |

### 3.2 Добавить `handle_trash`

Новая функция для обработки событий в режиме корзины:

| Клавиша | Действие |
|---------|----------|
| `t` / `е` | Выйти из корзины (вернуться в Normal) |
| `r` / `к` | Restore выбранной задачи + сохранить |
| `d` / `в` | Purge (безвозвратное удаление) + сохранить |
| `↑` / `k` / `л` | Переместиться вверх |
| `↓` / `j` / `о` | Переместиться вниз |
| `q` / `й` | Выйти из приложения |

### 3.3 Новые варианты `Action`

Добавить варианты для сохранения после delete/restore/purge:

```rust
pub enum Action {
    None,
    Quit,
    Save,
    Cancel,
    DeleteAndSave,   // soft delete → записать на диск
    RestoreAndSave,  // restore → записать на диск
    PurgeAndSave,    // physical delete → записать на диск
}
```

---

## Шаг 4 — UI (`src/ui.rs`)

### 4.1 Фильтрация задач

В `draw_task_list` работать не с `app.tasks` напрямую, а через индексы:

```rust
let indices = app.visible_indices(); // [usize] — индексы в app.tasks
```

Рендерить только `app.tasks[i]` для `i in indices`.

### 4.2 Визуальный стиль удалённых задач (режим корзины)

- Рамка: `Color::DarkGray` (вместо Reset или Orange)
- Текст задачи: `Color::DarkGray`
- Заголовок карточки: добавить `[удалено]` рядом с датой
- Выбранная задача в корзине: рамка `Color::Gray` (светлее)

### 4.3 Скролл вниз

В начале `draw_task_list`, после вычисления `total_height`:

```rust
if app.needs_scroll_to_bottom {
    app.scroll_offset = total_height.saturating_sub(list_area.height);
    app.selected = indices.len().saturating_sub(1);
    app.needs_scroll_to_bottom = false;
}
```

### 4.4 Подсказки (`draw_hints`)

| Режим | Строка подсказок |
|-------|-----------------|
| Normal | `[a] добавить  [e] изменить  [d] удалить  [space] в процессе  [t] корзина  [q] выйти` |
| Trash | `[r] восстановить  [d] удалить навсегда  [t] назад  [q] выйти` |
| Add/Edit | `[Cmd+S] сохранить  [Esc] отменить` |

### 4.5 Пустой список

Когда в корзине нет задач:

```
Paragraph::new("Корзина пуста")
```

---

## Шаг 5 — Основной цикл (`src/main.rs`)

Обработать новые варианты `Action` в `run_loop`:

```rust
Action::DeleteAndSave | Action::RestoreAndSave | Action::PurgeAndSave => {
    // app уже обновлён в handle_event
    storage::save(path, &app.tasks)?;
}
```

---

## Порядок реализации

1. `app.rs` — структуры, методы (Шаг 1)
2. `input.rs` — новые Action, handle_trash, обновить handle_normal (Шаг 3)
3. `main.rs` — обработка новых Action (Шаг 5)
4. `ui.rs` — фильтрация, визуал, скролл, подсказки (Шаг 4)
5. Проверка компиляции: `cargo build`
6. Ручное тестирование: удалить → перезапустить → войти в корзину → восстановить → purge

---

## Граничные случаи

- Если в Normal после delete список стал пустым → `selected = 0`, empty-state
- Если в Trash список пустых → показать "Корзина пуста", автоматически не выходить из режима
- `scroll_offset` сбрасывается при переключении режима (через `needs_scroll_to_bottom`)
- Старые JSON без поля `deleted` читаются корректно через `#[serde(default)]`
