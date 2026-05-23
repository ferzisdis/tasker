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

pub struct RenderedText {
    pub display: String,
    pub footnotes: Vec<(usize, String, String)>, // (n, link_text, url)
}

pub fn render_with_footnotes(text: &str) -> RenderedText {
    let links = parse_links(text);
    if links.is_empty() {
        return RenderedText {
            display: text.to_string(),
            footnotes: Vec::new(),
        };
    }

    let mut display = String::new();
    let mut footnotes = Vec::new();
    let mut last = 0;

    for (n, link) in links.iter().enumerate() {
        display.push_str(&text[last..link.range.start]);
        display.push_str(&link.text);
        display.push_str(&format!("[{}]", n + 1));
        footnotes.push((n + 1, link.text.clone(), link.url.clone()));
        last = link.range.end;
    }
    display.push_str(&text[last..]);

    RenderedText { display, footnotes }
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
