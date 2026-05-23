use std::path::PathBuf;

use anyhow::Result;

use crate::app::Task;

pub fn tasks_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("tasks.json")
}

pub fn load(path: &PathBuf) -> Result<Vec<Task>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(path)?;
    let tasks: Vec<Task> = serde_json::from_str(&data)?;
    Ok(tasks)
}

pub fn save(path: &PathBuf, tasks: &[Task]) -> Result<()> {
    let data = serde_json::to_string_pretty(tasks)?;
    std::fs::write(path, data)?;
    Ok(())
}
