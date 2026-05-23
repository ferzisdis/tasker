mod app;
mod input;
mod links;
mod storage;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    style::{Color, Style},
};
use std::io;
use tui_textarea::TextArea;

use app::{App, Mode};
use input::{Action, handle_event};

fn main() -> Result<()> {
    let path = storage::tasks_path();
    let tasks = storage::load(&path)?;
    let mut app = App::new(tasks);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    // Enable keyboard enhancement for terminals that support the Kitty protocol
    // (Kitty, WezTerm, Ghostty, iTerm2). This makes crossterm report the Super/Cmd
    // modifier so Cmd+S and Cmd+V work on macOS. Silently ignored elsewhere.
    let _ = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut textarea = TextArea::default();
    configure_textarea(&mut textarea, &app);

    let result = run_loop(&mut terminal, &mut app, &mut textarea, &path);

    disable_raw_mode()?;
    let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    textarea: &mut TextArea,
    path: &std::path::PathBuf,
) -> Result<()> {
    loop {
        terminal.draw(|f| {
            let ta = if app.mode == Mode::Add || app.mode == Mode::Edit {
                Some(textarea as &tui_textarea::TextArea)
            } else {
                None
            };
            ui::draw(f, app, ta);
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            let ev = event::read()?;
            let prev_mode = app.mode;

            match handle_event(ev, app, textarea) {
                Action::Quit => break,
                Action::Save => {
                    let text = textarea.lines().join("\n");
                    match prev_mode {
                        Mode::Add => app.add_task(text),
                        Mode::Edit => {
                            let task_idx = app.visible_indices().get(app.selected).copied();
                            if let Some(i) = task_idx {
                                app.edit_task(i, text);
                            }
                        }
                        _ => {}
                    }
                    storage::save(path, &app.tasks)?;
                    app.mode = Mode::Normal;
                    *textarea = TextArea::default();
                    configure_textarea(textarea, app);
                }
                Action::Cancel => {
                    app.mode = Mode::Normal;
                    *textarea = TextArea::default();
                    configure_textarea(textarea, app);
                }
                Action::DeleteAndSave | Action::RestoreAndSave | Action::PurgeAndSave => {
                    storage::save(path, &app.tasks)?;
                }
                Action::None => {}
            }

            // pre-fill textarea when entering Edit mode
            if prev_mode == Mode::Normal && app.mode == Mode::Edit {
                let task_idx = app.visible_indices().get(app.selected).copied();
                let text = task_idx
                    .and_then(|i| app.tasks.get(i))
                    .map(|t| t.text.clone())
                    .unwrap_or_default();
                *textarea = TextArea::default();
                configure_textarea(textarea, app);
                let lines: Vec<&str> = text.lines().collect();
                let last = lines.len().saturating_sub(1);
                for (i, line) in lines.iter().enumerate() {
                    textarea.insert_str(line);
                    if i < last {
                        textarea.insert_newline();
                    }
                }
            } else if prev_mode == Mode::Normal && app.mode == Mode::Add {
                *textarea = TextArea::default();
                configure_textarea(textarea, app);
            }
        }
    }
    Ok(())
}

fn configure_textarea(textarea: &mut TextArea, _app: &App) {
    // textarea is not rendered directly — ui.rs draws a custom wrapped view.
    // We only configure style for any potential fallback rendering.
    textarea.set_style(Style::default().fg(Color::White));
    textarea.set_cursor_style(Style::default().fg(Color::Black).bg(Color::White));
}
