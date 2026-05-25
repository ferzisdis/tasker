use arboard::Clipboard;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use tui_textarea::{CursorMove, TextArea};

use crate::{
    app::{App, Mode},
    links::html_to_markdown,
};

pub enum Action {
    None,
    Quit,
    Save,
    Cancel,
    DeleteAndSave,
    RestoreAndSave,
    PurgeAndSave,
}

pub fn handle_event(event: Event, app: &mut App, textarea: &mut TextArea) -> Action {
    // On Windows, crossterm reports both Press and Release for each keystroke.
    // Acting on Release too would double-execute every key, so ignore non-Press.
    if let Event::Key(KeyEvent { kind, .. }) = &event {
        if *kind != KeyEventKind::Press {
            return Action::None;
        }
    }

    match app.mode {
        Mode::Normal => handle_normal(event, app),
        Mode::Add | Mode::Edit => handle_editor(event, app, textarea),
        Mode::Trash => handle_trash(event, app),
    }
}

fn handle_normal(event: Event, app: &mut App) -> Action {
    match event {
        Event::Key(KeyEvent { code, .. }) => match code {
            // quit — q or й (Russian)
            KeyCode::Char('q') | KeyCode::Char('й') => return Action::Quit,
            // add — a or ф
            KeyCode::Char('a') | KeyCode::Char('ф') => app.mode = Mode::Add,
            // edit — e or у
            KeyCode::Char('e') | KeyCode::Char('у') => {
                if !app.visible_indices().is_empty() {
                    app.mode = Mode::Edit;
                }
            }
            // soft delete — d or в
            KeyCode::Char('d') | KeyCode::Char('в') => {
                if !app.visible_indices().is_empty() {
                    let sel = app.selected;
                    app.delete_task(sel);
                    return Action::DeleteAndSave;
                }
            }
            KeyCode::Char(' ') => {
                if !app.visible_indices().is_empty() {
                    let sel = app.selected;
                    app.toggle_in_progress(sel);
                }
            }
            // trash — t or е (Russian)
            KeyCode::Char('t') | KeyCode::Char('е') => {
                app.mode = Mode::Trash;
                app.selected = 0;
                app.scroll_offset = 0;
                app.needs_scroll_to_bottom = true;
            }
            // move up — ↑, k, or л
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('л') => app.move_up(),
            // move down — ↓, j, or о
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('о') => app.move_down(),
            _ => {}
        },
        Event::Mouse(MouseEvent { kind, column, row, .. }) => match kind {
            MouseEventKind::ScrollUp => app.move_up(),
            MouseEventKind::ScrollDown => app.move_down(),
            MouseEventKind::Down(MouseButton::Left) => {
                // Link hit-test takes priority over card selection.
                for (ly, xs, xe, url) in &app.link_rects {
                    if row == *ly && column >= *xs && column < *xe {
                        open_url(url);
                        return Action::None;
                    }
                }
                for (i, &(start, end)) in app.card_rows.iter().enumerate() {
                    if start < end && row >= start && row < end {
                        app.selected = i;
                        break;
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
    Action::None
}

fn handle_trash(event: Event, app: &mut App) -> Action {
    match event {
        Event::Key(KeyEvent { code, .. }) => match code {
            // quit — q or й
            KeyCode::Char('q') | KeyCode::Char('й') => return Action::Quit,
            // back to normal — t or е
            KeyCode::Char('t') | KeyCode::Char('е') => {
                app.mode = Mode::Normal;
                app.selected = 0;
                app.scroll_offset = 0;
                app.needs_scroll_to_bottom = true;
            }
            // restore — r or к (Russian)
            KeyCode::Char('r') | KeyCode::Char('к') => {
                if !app.visible_indices().is_empty() {
                    let sel = app.selected;
                    app.restore_task(sel);
                    return Action::RestoreAndSave;
                }
            }
            // purge (permanent delete) — d or в
            KeyCode::Char('d') | KeyCode::Char('в') => {
                if !app.visible_indices().is_empty() {
                    let sel = app.selected;
                    app.purge_task(sel);
                    return Action::PurgeAndSave;
                }
            }
            // move up — ↑, k, or л
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('л') => app.move_up(),
            // move down — ↓, j, or о
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('о') => app.move_down(),
            _ => {}
        },
        Event::Mouse(MouseEvent { kind, column, row, .. }) => match kind {
            MouseEventKind::ScrollUp => app.move_up(),
            MouseEventKind::ScrollDown => app.move_down(),
            MouseEventKind::Down(MouseButton::Left) => {
                // Link hit-test takes priority over card selection.
                for (ly, xs, xe, url) in &app.link_rects {
                    if row == *ly && column >= *xs && column < *xe {
                        open_url(url);
                        return Action::None;
                    }
                }
                for (i, &(start, end)) in app.card_rows.iter().enumerate() {
                    if start < end && row >= start && row < end {
                        app.selected = i;
                        break;
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
    Action::None
}

fn handle_editor(event: Event, app: &App, textarea: &mut TextArea) -> Action {
    if let Event::Key(KeyEvent { code, modifiers, .. }) = &event {
        let ctrl_or_cmd = modifiers.contains(KeyModifiers::CONTROL)
            || modifiers.contains(KeyModifiers::SUPER);

        // save: Cmd/Ctrl + S (also ы for Russian layout)
        if ctrl_or_cmd && matches!(code, KeyCode::Char('s') | KeyCode::Char('ы')) {
            return Action::Save;
        }

        if *code == KeyCode::Esc {
            return Action::Cancel;
        }
    }

    // Paste arrives as a bracketed-paste event (terminal's own paste shortcut,
    // right-click, etc.). Re-read the clipboard via arboard so HTML links survive;
    // fall back to the event's plain-text payload if that fails.
    if let Event::Paste(pasted) = &event {
        textarea.insert_str(&smart_paste(pasted));
        return Action::None;
    }

    // Mouse clicks: move cursor to clicked position, don't pass to textarea.input
    // (textarea renders via a custom Paragraph, so its internal geometry is wrong)
    if let Event::Mouse(MouseEvent { kind, column, row, .. }) = &event {
        if let MouseEventKind::Down(MouseButton::Left) = kind {
            if let Some((px, py, pw, ph)) = app.editor_popup {
                if let Some((lr, lc)) = editor_click_to_cursor(
                    *column,
                    *row,
                    px,
                    py,
                    pw,
                    ph,
                    app.editor_vscroll,
                    app.editor_inner_w,
                    textarea.lines(),
                ) {
                    textarea.move_cursor(CursorMove::Jump(lr, lc));
                }
            }
        }
        return Action::None;
    }

    textarea.input(event);
    Action::None
}

/// Convert a screen click at (col, row) into a logical (line, col) inside the textarea.
/// Returns None if the click is outside the popup content area.
fn editor_click_to_cursor(
    col: u16,
    row: u16,
    px: u16,
    py: u16,
    pw: u16,
    ph: u16,
    vscroll: usize,
    inner_w: usize,
    lines: &[impl AsRef<str>],
) -> Option<(u16, u16)> {
    // Content area starts one cell inside the border
    let cx = px + 1;
    let cy = py + 1;
    let cw = pw.saturating_sub(2);
    let ch = ph.saturating_sub(2);

    if col < cx || col >= cx + cw || row < cy || row >= cy + ch {
        return None;
    }

    let rel_col = (col - cx) as usize;
    let rel_row = (row - cy) as usize;
    let abs_vrow = rel_row + vscroll;

    let mut vrow = 0usize;
    for (li, line) in lines.iter().enumerate() {
        let line = line.as_ref();
        let char_count = line.chars().count();
        let visual_rows = if inner_w == 0 || char_count == 0 {
            1
        } else {
            (char_count + inner_w - 1) / inner_w
        };

        if abs_vrow < vrow + visual_rows {
            let chunk_idx = abs_vrow - vrow;
            let col_start = chunk_idx * inner_w;
            let logical_col = (col_start + rel_col).min(char_count);
            return Some((li as u16, logical_col as u16));
        }
        vrow += visual_rows;
    }

    // Click below all lines — jump to end of last line
    let last_line = lines.len().saturating_sub(1) as u16;
    let last_col = lines.last().map(|l| l.as_ref().chars().count()).unwrap_or(0) as u16;
    Some((last_line, last_col))
}

fn open_url(url: &str) {
    // Only http(s): both a UX guard and protection against shell injection
    // (notably the Windows `start` builtin). URL is passed as a separate arg.
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn smart_paste(fallback: &str) -> String {
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(_) => return fallback.to_string(),
    };

    // Browsers set both text/html and text/plain when copying from a web page.
    // get_text() only returns the plain-text flavour — HTML tags are lost.
    // Try the HTML flavour first so links survive the paste.
    if let Ok(html) = clipboard.get().html() {
        if html.contains("<a ") || html.contains("<A ") {
            let md = html_to_markdown(&html);
            if !md.is_empty() {
                return md;
            }
        }
    }

    match clipboard.get_text() {
        Ok(text) if !text.is_empty() => text,
        _ => fallback.to_string(),
    }
}
