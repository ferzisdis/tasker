use std::ops::Range;

pub struct Link {
    pub range: Range<usize>,
    pub text: String,
    pub url: String,
}

pub fn parse_links(text: &str) -> Vec<Link> {
    let mut links = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();

    while i < chars.len() {
        if chars[i] == '[' {
            // find closing ]
            if let Some(rel_close) = chars[i..].iter().position(|&c| c == ']') {
                let j = i + rel_close;
                if j + 1 < chars.len() && chars[j + 1] == '(' {
                    if let Some(rel_paren) = chars[j + 1..].iter().position(|&c| c == ')') {
                        let k = j + 1 + rel_paren;
                        let byte_start = char_offset(text, i);
                        let byte_end = char_offset(text, k) + chars[k].len_utf8();
                        let link_text: String = chars[i + 1..j].iter().collect();
                        let url: String = chars[j + 2..k].iter().collect();
                        links.push(Link {
                            range: byte_start..byte_end,
                            text: link_text,
                            url,
                        });
                        i = k + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    links
}

fn char_offset(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// A clickable region within the rendered display text, in (display line, char column) coords.
pub struct InlineLink {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub url: String,
    /// true for a bare URL typed in the text; false for the inline text of a markdown link.
    pub is_bare: bool,
    /// sequential color index (appearance order); the UI maps it onto its palette.
    pub color: usize,
}

pub struct RenderedText {
    pub display: String,
    pub footnotes: Vec<(usize, String, String, usize)>, // (n, link_text, url, color)
    pub inline: Vec<InlineLink>,
}

pub fn render_with_footnotes(text: &str) -> RenderedText {
    let links = parse_links(text);

    let mut display = String::new();
    let mut footnotes_tmp: Vec<(usize, String, String)> = Vec::new();
    // (char_start, char_end, url) of each markdown link's inline text within `display`
    let mut md_spans: Vec<(usize, usize, String)> = Vec::new();
    let mut last = 0;

    for (n, link) in links.iter().enumerate() {
        display.push_str(&text[last..link.range.start]);
        let start = display.chars().count();
        display.push_str(&link.text);
        let end = display.chars().count();
        md_spans.push((start, end, link.url.clone()));
        display.push_str(&format!("[{}]", n + 1));
        footnotes_tmp.push((n + 1, link.text.clone(), link.url.clone()));
        last = link.range.end;
    }
    display.push_str(&text[last..]);

    // Bare URLs sitting directly in the text (not part of markdown), excluding the
    // inline-text spans we just produced.
    let exclude: Vec<(usize, usize)> = md_spans.iter().map(|&(s, e, _)| (s, e)).collect();
    let bare = find_bare_urls(&display, &exclude);

    let starts = line_start_offsets(&display);
    let mut inline = Vec::new();
    for (s, e, url) in md_spans {
        let (line, col) = to_line_col(&starts, s);
        inline.push(InlineLink { line, col_start: col, col_end: col + (e - s), url, is_bare: false, color: 0 });
    }
    for (s, e, url) in bare {
        let (line, col) = to_line_col(&starts, s);
        inline.push(InlineLink { line, col_start: col, col_end: col + (e - s), url, is_bare: true, color: 0 });
    }

    // Shared, rotating color index assigned by appearance order (position in text),
    // so links in the body and in footnotes draw from one rotating pool.
    let mut order: Vec<usize> = (0..inline.len()).collect();
    order.sort_by_key(|&i| (inline[i].line, inline[i].col_start));
    for (seq, &i) in order.iter().enumerate() {
        inline[i].color = seq;
    }

    // Each footnote shares the color of its markdown link. The first `footnotes_tmp.len()`
    // inline entries are the markdown links, in footnote order.
    let footnotes = footnotes_tmp
        .into_iter()
        .enumerate()
        .map(|(k, (n, t, u))| (n, t, u, inline[k].color))
        .collect();

    RenderedText { display, footnotes, inline }
}

fn line_start_offsets(s: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, ch) in s.chars().enumerate() {
        if ch == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn to_line_col(starts: &[usize], off: usize) -> (usize, usize) {
    let mut line = 0;
    for (i, &st) in starts.iter().enumerate() {
        if st <= off {
            line = i;
        } else {
            break;
        }
    }
    (line, off - starts[line])
}

fn starts_with_at(chars: &[char], i: usize, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    if i + p.len() > chars.len() {
        return false;
    }
    chars[i..i + p.len()] == p[..]
}

/// Find bare http(s) URLs in `display`. Returns (char_start, char_end, url) char offsets.
/// Skips any URL overlapping a span in `exclude` (markdown inline text already handled).
fn find_bare_urls(display: &str, exclude: &[(usize, usize)]) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = display.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if starts_with_at(&chars, i, "https://") || starts_with_at(&chars, i, "http://") {
            let mut j = i;
            while j < chars.len() && !chars[j].is_whitespace() {
                j += 1;
            }
            // strip trailing punctuation that's usually sentence/wrapping, not part of the URL
            while j > i
                && matches!(
                    chars[j - 1],
                    '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\'' | '>'
                )
            {
                j -= 1;
            }
            let overlaps = exclude.iter().any(|&(s, e)| i < e && j > s);
            if !overlaps && j > i {
                out.push((i, j, chars[i..j].iter().collect()));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

pub fn html_to_markdown(html: &str) -> String {
    let mut result = String::new();
    let mut rest = html;

    while !rest.is_empty() {
        if let Some(a_start) = find_tag_start(rest, "a") {
            result.push_str(&strip_all_tags(&rest[..a_start]));
            rest = &rest[a_start..];

            let tag_end = rest.find('>').unwrap_or(rest.len() - 1);
            let tag = &rest[..tag_end + 1];
            let href = extract_href(tag);

            rest = &rest[tag_end + 1..];

            let close_tag = rest.to_lowercase();
            let close = close_tag.find("</a>").unwrap_or(rest.len());
            let link_text = strip_all_tags(&rest[..close]).trim().to_string();

            if let Some(url) = href {
                result.push('[');
                result.push_str(&link_text);
                result.push_str("](");
                result.push_str(&url);
                result.push(')');
            } else {
                result.push_str(&link_text);
            }

            rest = if close + 4 <= rest.len() {
                &rest[close + 4..]
            } else {
                ""
            };
        } else {
            result.push_str(&strip_all_tags(rest));
            break;
        }
    }

    result.trim().to_string()
}

fn find_tag_start(s: &str, tag: &str) -> Option<usize> {
    let lower = s.to_lowercase();
    let pattern = format!("<{}", tag);
    lower.find(&pattern).filter(|&pos| {
        let after = s[pos + pattern.len()..].chars().next();
        matches!(after, Some(' ') | Some('>') | Some('\n') | Some('\t'))
    })
}

fn strip_all_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn extract_href(tag: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let href_pos = lower.find("href=")?;
    let after = &tag[href_pos + 5..];
    let (quote, rest) = if after.starts_with('"') {
        ('"', &after[1..])
    } else if after.starts_with('\'') {
        ('\'', &after[1..])
    } else {
        return None;
    };
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_url_detected_inline() {
        let r = render_with_footnotes("see https://example.com/x done");
        assert_eq!(r.display, "see https://example.com/x done");
        assert!(r.footnotes.is_empty());
        assert_eq!(r.inline.len(), 1);
        let il = &r.inline[0];
        assert!(il.is_bare);
        assert_eq!(il.url, "https://example.com/x");
        assert_eq!(il.line, 0);
        assert_eq!(il.col_start, 4);
        assert_eq!(il.col_end, 4 + "https://example.com/x".chars().count());
    }

    #[test]
    fn trailing_punctuation_stripped() {
        let r = render_with_footnotes("go https://a.com.");
        assert_eq!(r.inline[0].url, "https://a.com");
    }

    #[test]
    fn markdown_link_inline_and_footnote() {
        let r = render_with_footnotes("[foo](https://bar)");
        assert_eq!(r.display, "foo[1]");
        assert_eq!(r.footnotes.len(), 1);
        assert_eq!(r.footnotes[0].2, "https://bar");
        // the only inline span is the markdown text, not a bare URL
        assert_eq!(r.inline.len(), 1);
        assert!(!r.inline[0].is_bare);
        assert_eq!(r.inline[0].col_start, 0);
        assert_eq!(r.inline[0].col_end, 3);
        assert_eq!(r.inline[0].url, "https://bar");
    }

    #[test]
    fn bare_url_on_second_line() {
        let r = render_with_footnotes("first\nhttps://b.com here");
        let il = r.inline.iter().find(|i| i.is_bare).unwrap();
        assert_eq!(il.line, 1);
        assert_eq!(il.col_start, 0);
        assert_eq!(il.url, "https://b.com");
    }

    #[test]
    fn colors_rotate_across_text_and_footnotes() {
        // appearance order: md "a" (0), bare (1), md "c" (2)
        let r = render_with_footnotes("[a](u1) https://b [c](u2)");
        let bare = r.inline.iter().find(|i| i.is_bare).unwrap();
        assert_eq!(bare.color, 1);
        // footnotes share their markdown link's color
        assert_eq!(r.footnotes[0].3, 0); // footnote [1] -> "a"
        assert_eq!(r.footnotes[1].3, 2); // footnote [2] -> "c"
    }

    #[test]
    fn markdown_and_bare_mixed() {
        let r = render_with_footnotes("[a](https://md) and https://bare.com");
        // display: "a[1] and https://bare.com"
        assert_eq!(r.display, "a[1] and https://bare.com");
        let bare: Vec<_> = r.inline.iter().filter(|i| i.is_bare).collect();
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].url, "https://bare.com");
    }
}
