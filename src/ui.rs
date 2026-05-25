use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
};
use tui_textarea::TextArea;

use crate::{
    app::{App, Mode},
    links::{RenderedText, render_with_footnotes},
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
    app.link_rects.clear();

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
    let is_trash = app.mode == Mode::Trash;

    let rendered_tasks: Vec<_> = indices
        .iter()
        .map(|&i| render_with_footnotes(&app.tasks[i].text))
        .collect();

    // Lay out each card into explicit visual rows (manual char wrapping) so click
    // hit-testing of links matches the render exactly.
    let card_layouts: Vec<Vec<VisualRow>> = rendered_tasks
        .iter()
        .map(|r| layout_card(r, inner_w as usize, is_trash))
        .collect();

    let card_heights: Vec<u16> = card_layouts.iter().map(|l| l.len() as u16 + 2).collect();

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

        let title_line = Line::from(vec![Span::raw(" "), Span::styled(date_str, date_style)]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .title_top(title_line);

        let layout = &card_layouts[vis_i];
        let lines: Vec<Line> = layout.iter().map(|vr| vr.line.clone()).collect();
        let paragraph = Paragraph::new(lines).block(block);
        f.render_widget(paragraph, card_rect);

        // Register clickable link rects (terminal coords) for the visible rows.
        // Content sits one cell inside the border; Paragraph draws rows top-down.
        let content_x = card_rect.x + 1;
        let content_y = card_rect.y + 1;
        let content_rows = render_h.saturating_sub(2);
        let max_x = content_x + inner_w;
        for (ri, vr) in layout.iter().enumerate() {
            if ri as u16 >= content_rows {
                break;
            }
            let y = content_y + ri as u16;
            for (cs, ce, url) in &vr.links {
                let x_start = content_x + cs;
                let x_end = (content_x + ce).min(max_x);
                if x_start < x_end {
                    app.link_rects.push((y, x_start, x_end, url.clone()));
                }
            }
        }

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

struct VisualRow {
    line: Line<'static>,
    /// (col_start, col_end, url) clickable spans within the content area (col 0 = first content cell)
    links: Vec<(u16, u16, String)>,
}

fn palette(seq: usize) -> Color {
    let (r, g, b) = LINK_PALETTE[seq % LINK_PALETTE.len()];
    Color::Rgb(r, g, b)
}

/// Lay a card out into explicit visual rows using manual char-based wrapping, so the
/// rendered geometry is deterministic and matches click hit-testing.
fn layout_card(rendered: &RenderedText, inner_w: usize, is_trash: bool) -> Vec<VisualRow> {
    let w = inner_w.max(1);
    let mut rows: Vec<VisualRow> = Vec::new();

    // Footnote color by number n (n -> shared palette index).
    let fc: Vec<usize> = rendered.footnotes.iter().map(|(_, _, _, c)| *c).collect();

    let display_lines: Vec<&str> = rendered.display.split('\n').collect();
    for (li, dline) in display_lines.iter().enumerate() {
        let chars: Vec<char> = dline.chars().collect();
        let n = chars.len();

        // Per-char style. Plain text stays default (white); only links/markers get color.
        let mut styles = vec![Style::default(); n];
        if is_trash {
            for s in styles.iter_mut() {
                *s = Style::default().fg(Color::DarkGray);
            }
        } else {
            // Bare URLs: colored, no underline. (markdown inline text stays white.)
            for il in rendered.inline.iter().filter(|il| il.line == li && il.is_bare) {
                let st = Style::default().fg(palette(il.color));
                for p in il.col_start..il.col_end.min(n) {
                    styles[p] = st;
                }
            }
            // `[n]` markers: bold, in the link's shared color.
            for (s, e, num) in marker_spans(dline) {
                let color = fc.get(num.saturating_sub(1)).copied().unwrap_or(0);
                let st = Style::default().fg(palette(color)).add_modifier(Modifier::BOLD);
                for p in s..e.min(n) {
                    styles[p] = st;
                }
            }
        }

        // Clickable rects: every inline link on this line (markdown text + bare URLs).
        let links: Vec<(usize, usize, String)> = rendered
            .inline
            .iter()
            .filter(|il| il.line == li)
            .map(|il| (il.col_start, il.col_end, il.url.clone()))
            .collect();

        emit_rows(&chars, &styles, &links, w, &mut rows);
    }

    // Footnotes: color only `[n]` and the URL; the link text in between stays white.
    for (num, text, url, color) in &rendered.footnotes {
        let full = format!("[{}] {}: {}", num, text, url);
        let chars: Vec<char> = full.chars().collect();
        let n = chars.len();
        let marker_len = format!("[{}]", num).chars().count();
        let url_start = n.saturating_sub(url.chars().count());

        let mut styles = vec![Style::default(); n];
        if is_trash {
            for s in styles.iter_mut() {
                *s = Style::default().fg(Color::DarkGray);
            }
        } else {
            let c = palette(*color);
            for p in 0..marker_len.min(n) {
                styles[p] = Style::default().fg(c);
            }
            for p in url_start..n {
                styles[p] = Style::default().fg(c);
            }
        }

        let links = vec![(0usize, n, url.clone())];
        emit_rows(&chars, &styles, &links, w, &mut rows);
    }

    rows
}

/// Wrap a logical line (chars + per-char styles + clickable char ranges) into visual rows
/// of `w` columns, producing styled `Line`s and content-relative link rects.
fn emit_rows(
    chars: &[char],
    styles: &[Style],
    links: &[(usize, usize, String)],
    w: usize,
    out: &mut Vec<VisualRow>,
) {
    let n = chars.len();
    let nrows = if n == 0 { 1 } else { (n + w - 1) / w };
    for ci in 0..nrows {
        let base = ci * w;
        let end = (base + w).min(n);

        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut cur = String::new();
        let mut cur_style: Option<Style> = None;
        for pos in base..end {
            let st = styles[pos];
            if cur_style != Some(st) {
                if !cur.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut cur), cur_style.unwrap()));
                }
                cur_style = Some(st);
            }
            cur.push(chars[pos]);
        }
        if !cur.is_empty() {
            spans.push(Span::styled(cur, cur_style.unwrap_or_default()));
        }
        let line = if spans.is_empty() {
            Line::from(String::new())
        } else {
            Line::from(spans)
        };

        let mut rects = Vec::new();
        for (s, e, url) in links {
            let cs = (*s).max(base);
            let ce = (*e).min(end);
            if cs < ce {
                rects.push(((cs - base) as u16, (ce - base) as u16, url.clone()));
            }
        }

        out.push(VisualRow { line, links: rects });
    }
}

/// Char ranges of `[n]` footnote markers within a line: (char_start, char_end_excl, n).
fn marker_spans(line: &str) -> Vec<(usize, usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == ']') {
                let close = i + 1 + rel;
                let inner: String = chars[i + 1..close].iter().collect();
                if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(n) = inner.parse::<usize>() {
                        out.push((i, close + 1, n));
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
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
