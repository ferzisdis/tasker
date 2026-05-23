use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
};
use tui_textarea::TextArea;

use crate::{
    app::{App, Mode},
    links::render_with_footnotes,
};

const LINK_PALETTE: [(u8, u8, u8); 6] = [
    (100, 210, 220),
    (180, 140, 230),
    (100, 220, 160),
    (230, 140, 180),
    (190, 220, 100),
    (230, 180, 120),
];

pub fn draw(f: &mut Frame, app: &mut App, textarea: Option<&TextArea>) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(size);

    draw_task_list(f, app, chunks[0]);
    draw_hints(f, app, chunks[1]);

    if app.mode == Mode::Add || app.mode == Mode::Edit {
        if let Some(ta) = textarea {
            draw_editor(f, app, ta, size);
        }
    }
}

fn draw_task_list(f: &mut Frame, app: &mut App, area: Rect) {
    app.card_rows.clear();

    let indices = app.visible_indices();

    if indices.is_empty() {
        let msg = if app.mode == Mode::Trash {
            "Корзина пуста"
        } else {
            "Нажмите [a] чтобы добавить задачу"
        };
        let p = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        app.needs_scroll_to_bottom = false;
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    let list_area = chunks[0];
    let scroll_area = chunks[1];

    // inner width inside card borders
    let inner_w = list_area.width.saturating_sub(2);
    let today = chrono::Local::now().date_naive();

    let rendered_tasks: Vec<_> = indices
        .iter()
        .map(|&i| render_with_footnotes(&app.tasks[i].text))
        .collect();

    let card_heights: Vec<u16> = rendered_tasks
        .iter()
        .map(|r| {
            let text_rows: u16 = r
                .display
                .lines()
                .map(|l| wrapped_rows(l, inner_w))
                .sum::<u16>()
                .max(1);
            let fn_rows: u16 = r
                .footnotes
                .iter()
                .map(|(n, txt, url)| wrapped_rows(&format!("[{}] {}: {}", n, txt, url), inner_w))
                .sum();
            2 + text_rows + fn_rows
        })
        .collect();

    let total_height: u16 = card_heights.iter().sum();

    // Snap scroll and cursor to last item (startup or entering Trash).
    if app.needs_scroll_to_bottom {
        app.scroll_offset = total_height.saturating_sub(list_area.height);
        app.selected = indices.len().saturating_sub(1);
        app.needs_scroll_to_bottom = false;
    }

    let selected = app.selected;

    // Clamp persistent offset minimally — only move when selected card leaves viewport.
    app.scroll_offset = clamp_scroll_to_selected(
        app.scroll_offset,
        &card_heights,
        selected,
        list_area.height,
        total_height,
    );
    let offset_y = app.scroll_offset;

    app.card_rows.clear();

    let is_trash = app.mode == Mode::Trash;
    let mut scrolled_y: i32 = -(offset_y as i32);

    for (vis_i, &task_i) in indices.iter().enumerate() {
        let task = &app.tasks[task_i];
        let card_h = card_heights[vis_i];
        let abs_y = list_area.y as i32 + scrolled_y;

        if abs_y + card_h as i32 <= list_area.y as i32 {
            app.card_rows.push((0, 0));
            scrolled_y += card_h as i32;
            continue;
        }
        if abs_y >= list_area.bottom() as i32 {
            app.card_rows.push((0, 0));
            scrolled_y += card_h as i32;
            continue;
        }

        let render_y = abs_y.max(list_area.y as i32) as u16;
        let render_h = ((abs_y + card_h as i32)
            .min(list_area.bottom() as i32)
            - abs_y.max(list_area.y as i32))
            .max(0) as u16;

        if render_h == 0 {
            app.card_rows.push((0, 0));
            scrolled_y += card_h as i32;
            continue;
        }

        app.card_rows.push((render_y, render_y + render_h));

        let card_rect = Rect {
            x: list_area.x,
            y: render_y,
            width: list_area.width,
            height: render_h,
        };

        let days = (today - task.created_at).num_days();
        let is_selected = vis_i == selected;

        let date_str = if is_trash {
            format!("{} ({}д) [удалено]", task.created_at.format("%d.%m.%Y"), days)
        } else {
            format!("{} ({}д)", task.created_at.format("%d.%m.%Y"), days)
        };

        let date_style = if is_trash || days <= 30 {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Red)
        };

        let border_type = if task.in_progress && !is_trash {
            BorderType::Thick
        } else {
            BorderType::Plain
        };

        let border_color = if is_trash {
            if is_selected { Color::Gray } else { Color::DarkGray }
        } else if is_selected {
            Color::Rgb(255, 165, 0)
        } else {
            Color::Reset
        };

        let rendered = &rendered_tasks[vis_i];
        let title_line = Line::from(vec![Span::raw(" "), Span::styled(date_str, date_style)]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .title_top(title_line);

        let mut lines: Vec<Line> = Vec::new();
        for l in rendered.display.lines() {
            if is_trash {
                lines.push(Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                lines.push(build_display_line(l, &rendered.footnotes));
            }
        }
        for (n, link_text, url) in &rendered.footnotes {
            let c = if is_trash {
                Color::DarkGray
            } else {
                let (r, g, b) = LINK_PALETTE[(n - 1) % LINK_PALETTE.len()];
                Color::Rgb(r, g, b)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("[{}] ", n), Style::default().fg(c)),
                Span::styled(format!("{}: {}", link_text, url), Style::default().fg(c)),
            ]));
        }

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, card_rect);

        scrolled_y += card_h as i32;
    }

    // content_length = max scrollable rows (total minus viewport).
    let max_scroll = total_height.saturating_sub(list_area.height) as usize;
    if max_scroll > 0 {
        let mut sb_state = ScrollbarState::new(max_scroll).position(offset_y as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scroll_area,
            &mut sb_state,
        );
    }
}

fn wrapped_rows(line: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }
    let len = line.chars().count() as u16;
    if len == 0 { 1 } else { (len + width - 1) / width }
}

fn build_display_line<'a>(line: &'a str, footnotes: &[(usize, String, String)]) -> Line<'a> {
    if footnotes.is_empty() {
        return Line::from(line);
    }
    let mut spans: Vec<Span> = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        if let Some(pos) = rest.find('[') {
            if pos > 0 {
                spans.push(Span::raw(&rest[..pos]));
            }
            rest = &rest[pos..];
            if let Some(end) = rest[1..].find(']') {
                let marker = &rest[..end + 2];
                if let Ok(n) = rest[1..end + 1].parse::<usize>() {
                    let (r, g, b) = LINK_PALETTE[(n - 1) % LINK_PALETTE.len()];
                    spans.push(Span::styled(
                        marker.to_string(),
                        Style::default()
                            .fg(Color::Rgb(r, g, b))
                            .add_modifier(Modifier::BOLD),
                    ));
                    rest = &rest[end + 2..];
                    continue;
                }
            }
            spans.push(Span::raw(&rest[..1]));
            rest = &rest[1..];
        } else {
            spans.push(Span::raw(rest));
            break;
        }
    }
    Line::from(spans)
}

/// Adjust `current` offset only enough to keep the selected card fully visible.
/// Never resets to 0 — preserves the user's scroll position on click.
fn clamp_scroll_to_selected(
    current: u16,
    card_heights: &[u16],
    selected: usize,
    visible: u16,
    total_height: u16,
) -> u16 {
    let max_offset = total_height.saturating_sub(visible);
    let top: u16 = card_heights[..selected.min(card_heights.len())].iter().sum();
    let bottom = top.saturating_add(card_heights.get(selected).copied().unwrap_or(0));

    let adjusted = if top < current {
        // card scrolled above viewport — bring top into view
        top
    } else if bottom > current.saturating_add(visible) {
        // card scrolled below viewport — bring bottom into view
        bottom.saturating_sub(visible)
    } else {
        current
    };

    adjusted.min(max_offset)
}

fn draw_hints(f: &mut Frame, app: &App, area: Rect) {
    let text = match app.mode {
        Mode::Normal => {
            "[a] добавить  [e] изменить  [d] удалить  [space] в процессе  [t] корзина  [q] выйти"
        }
        Mode::Trash => "[r] восстановить  [d] удалить навсегда  [t] назад  [q] выйти",
        Mode::Add | Mode::Edit => "[Cmd+S] сохранить  [Esc] отменить",
    };
    let p = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

fn draw_editor(f: &mut Frame, app: &mut App, textarea: &TextArea, area: Rect) {
    let popup_w = area.width.saturating_sub(4).max(40).min(area.width);
    let popup_h = (area.height / 2).max(10).min(area.height);
    let popup = centered_rect(popup_w, popup_h, area);
    f.render_widget(Clear, popup);

    let inner_w = popup_w.saturating_sub(2) as usize;
    let inner_h = popup_h.saturating_sub(2) as usize;

    app.editor_popup = Some((popup.x, popup.y, popup.width, popup.height));
    app.editor_inner_w = inner_w;
    let (cur_row, cur_col) = textarea.cursor();
    let lines = textarea.lines();

    // compute visual row of cursor for scroll
    let mut vcur = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if i < cur_row {
            vcur += visual_line_count(line.chars().count(), inner_w);
        } else {
            vcur += if inner_w > 0 { cur_col / inner_w } else { 0 };
            break;
        }
    }
    let vscroll = if inner_h > 0 && vcur >= inner_h { vcur - inner_h + 1 } else { 0 };
    app.editor_vscroll = vscroll;

    let mut display_lines: Vec<Line<'static>> = Vec::new();
    let mut vrow = 0usize;

    for (li, logical_line) in lines.iter().enumerate() {
        let chunks = split_visual_chunks(logical_line, inner_w);
        for (ci, chunk) in chunks.iter().enumerate() {
            if vrow >= vscroll && vrow < vscroll + inner_h {
                let chunk_start = ci * inner_w;
                let cursor_here = li == cur_row
                    && cur_col >= chunk_start
                    && (cur_col < chunk_start + inner_w || ci + 1 == chunks.len());
                if cursor_here {
                    display_lines.push(line_with_cursor(chunk, cur_col - chunk_start));
                } else {
                    display_lines.push(Line::from(chunk.clone()));
                }
            }
            vrow += 1;
        }
    }

    let title = if app.mode == Mode::Edit { " Редактировать " } else { " Новая задача " };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(255, 165, 0)))
        .title(title)
        .title_bottom(
            Line::from(" [Cmd+S] сохранить  [Esc] отменить ")
                .style(Style::default().fg(Color::DarkGray)),
        );

    let para = Paragraph::new(display_lines).block(block);
    f.render_widget(para, popup);
}

fn visual_line_count(char_len: usize, width: usize) -> usize {
    if width == 0 || char_len == 0 { 1 } else { (char_len + width - 1) / width }
}

fn split_visual_chunks(line: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![line.to_string()];
    }
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars.chunks(width)
        .map(|c| c.iter().collect())
        .collect()
}

fn line_with_cursor(chunk: &str, col: usize) -> Line<'static> {
    let cursor_style = Style::default().fg(Color::Black).bg(Color::White);
    let chars: Vec<char> = chunk.chars().collect();
    if col >= chars.len() {
        Line::from(vec![
            Span::raw(chunk.to_string()),
            Span::styled(" ", cursor_style),
        ])
    } else {
        let before: String = chars[..col].iter().collect();
        let at: String = std::iter::once(chars[col]).collect();
        let after: String = chars[col + 1..].iter().collect();
        Line::from(vec![
            Span::raw(before),
            Span::styled(at, cursor_style),
            Span::raw(after),
        ])
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
