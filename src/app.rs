use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub text: String,
    pub in_progress: bool,
    pub created_at: NaiveDate,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub deleted_at: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Add,
    Edit,
    Trash,
}

pub struct App {
    pub tasks: Vec<Task>,
    pub selected: usize,
    /// persistent scroll position in content rows; adjusted minimally to keep selected visible
    pub scroll_offset: u16,
    pub mode: Mode,
    pub next_id: u64,
    /// (start_row, end_row) per visible task in terminal coordinates; rebuilt every frame
    pub card_rows: Vec<(u16, u16)>,
    /// (row, col_start, col_end, url) clickable link rects in terminal coords; rebuilt every frame
    pub link_rects: Vec<(u16, u16, u16, String)>,
    /// set true to snap scroll + cursor to last item on next draw
    pub needs_scroll_to_bottom: bool,
    /// editor popup geometry: (x, y, width, height); set each frame by draw_editor
    pub editor_popup: Option<(u16, u16, u16, u16)>,
    /// visual scroll offset inside the editor popup; set each frame by draw_editor
    pub editor_vscroll: usize,
    /// inner content width of the editor popup (popup_w - 2); set each frame by draw_editor
    pub editor_inner_w: usize,
}

impl App {
    pub fn new(tasks: Vec<Task>) -> Self {
        let next_id = tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        App {
            tasks,
            selected: 0,
            scroll_offset: 0,
            mode: Mode::Normal,
            next_id,
            card_rows: Vec::new(),
            link_rects: Vec::new(),
            needs_scroll_to_bottom: true,
            editor_popup: None,
            editor_vscroll: 0,
            editor_inner_w: 0,
        }
    }

    /// Indices into `self.tasks` that are visible in the current mode.
    /// Normal/Add/Edit → non-deleted; Trash → deleted.
    pub fn visible_indices(&self) -> Vec<usize> {
        match self.mode {
            Mode::Normal | Mode::Add | Mode::Edit => self
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| !t.deleted)
                .map(|(i, _)| i)
                .collect(),
            Mode::Trash => self
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.deleted)
                .map(|(i, _)| i)
                .collect(),
        }
    }

    pub fn add_task(&mut self, text: String) {
        let task = Task {
            id: self.next_id,
            text,
            in_progress: false,
            created_at: chrono::Local::now().date_naive(),
            deleted: false,
            deleted_at: None,
        };
        self.next_id += 1;
        self.tasks.push(task);
        let vis_len = self.visible_indices().len();
        self.selected = vis_len.saturating_sub(1);
    }

    /// Soft-delete: mark the task at visible position `vis_index` as deleted.
    pub fn delete_task(&mut self, vis_index: usize) {
        let indices = self.visible_indices();
        if let Some(&task_i) = indices.get(vis_index) {
            self.tasks[task_i].deleted = true;
            self.tasks[task_i].deleted_at = Some(chrono::Local::now().date_naive());
        }
        self.clamp_selected();
    }

    /// Restore: clear the deleted flag for the task at visible position `vis_index` (Trash mode).
    pub fn restore_task(&mut self, vis_index: usize) {
        let indices = self.visible_indices();
        if let Some(&task_i) = indices.get(vis_index) {
            self.tasks[task_i].deleted = false;
        }
        self.clamp_selected();
    }

    /// Purge: physically remove the task at visible position `vis_index` from storage (Trash mode).
    pub fn purge_task(&mut self, vis_index: usize) {
        let indices = self.visible_indices();
        if let Some(&task_i) = indices.get(vis_index) {
            self.tasks.remove(task_i);
        }
        self.clamp_selected();
    }

    pub fn toggle_in_progress(&mut self, vis_index: usize) {
        let indices = self.visible_indices();
        if let Some(&task_i) = indices.get(vis_index) {
            self.tasks[task_i].in_progress = !self.tasks[task_i].in_progress;
        }
    }

    /// `task_index` is a direct index into `self.tasks` (caller resolves via visible_indices).
    pub fn edit_task(&mut self, task_index: usize, text: String) {
        if let Some(task) = self.tasks.get_mut(task_index) {
            task.text = text;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let vis_len = self.visible_indices().len();
        if vis_len > 0 && self.selected < vis_len - 1 {
            self.selected += 1;
        }
    }

    fn clamp_selected(&mut self) {
        let vis_len = self.visible_indices().len();
        if vis_len == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(vis_len - 1);
        }
    }
}
