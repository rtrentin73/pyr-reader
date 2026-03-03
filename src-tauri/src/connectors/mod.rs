// Data source connectors module
pub mod gmail;
pub mod rss;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub source: DataSource,
    pub author: String,
    pub content: String,
    pub url: Option<String>,
    pub timestamp: i64,
    pub raw_data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    RSS,
    Email,
}

pub trait Connector {
    async fn fetch_posts(&self) -> anyhow::Result<Vec<Post>>;
    fn is_authenticated(&self) -> bool;
}

// ── Shared content normalization ──────────────────────────────────────────

/// Strip HTML tags, decode HTML entities, and collapse whitespace.
/// Used by all connectors to produce clean plain-text content.
pub fn normalize_content(input: &str) -> String {
    let stripped = strip_html_tags(input);
    let decoded = decode_html_entities(&stripped);
    // Collapse runs of whitespace (incl. newlines) to single spaces, then trim
    decoded
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Remove HTML/XML tags and invisible element content (style, script, head).
fn strip_html_tags(input: &str) -> String {
    // First, remove entire <style>…</style>, <script>…</script>, <head>…</head> blocks
    // (case-insensitive) so their inner text doesn't leak into output.
    let mut cleaned = input.to_string();
    for tag in &["style", "script", "head"] {
        loop {
            let lower = cleaned.to_ascii_lowercase();
            let open = format!("<{}", tag);
            let close = format!("</{}>", tag);
            if let Some(start) = lower.find(&open) {
                if let Some(end_rel) = lower[start..].find(&close) {
                    let end = start + end_rel + close.len();
                    cleaned.replace_range(start..end, " ");
                    continue;
                }
            }
            break;
        }
    }

    // Now strip remaining tags with a simple state machine.
    let mut result = String::with_capacity(cleaned.len());
    let mut inside_tag = false;

    for ch in cleaned.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                result.push(' ');
            }
            _ if !inside_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

/// Decode common HTML/XML character entities and numeric references.
fn decode_html_entities(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '&' {
            result.push(ch);
            continue;
        }

        // Collect the entity reference up to ';' (max 10 chars to avoid runaway)
        let mut entity = String::new();
        let mut found_semi = false;
        for _ in 0..10 {
            match chars.peek() {
                Some(&';') => {
                    chars.next();
                    found_semi = true;
                    break;
                }
                Some(&c) if c.is_alphanumeric() || c == '#' => {
                    entity.push(c);
                    chars.next();
                }
                _ => break,
            }
        }

        if !found_semi || entity.is_empty() {
            // Not a valid entity — emit the raw characters
            result.push('&');
            result.push_str(&entity);
            continue;
        }

        match entity.as_str() {
            "amp" => result.push('&'),
            "lt" => result.push('<'),
            "gt" => result.push('>'),
            "quot" => result.push('"'),
            "apos" => result.push('\''),
            "nbsp" => result.push(' '),
            "mdash" => result.push('\u{2014}'),
            "ndash" => result.push('\u{2013}'),
            "lsquo" => result.push('\u{2018}'),
            "rsquo" => result.push('\u{2019}'),
            "ldquo" => result.push('\u{201C}'),
            "rdquo" => result.push('\u{201D}'),
            "hellip" => result.push_str("..."),
            "bull" => result.push('\u{2022}'),
            "copy" => result.push('\u{00A9}'),
            "reg" => result.push('\u{00AE}'),
            "trade" => result.push('\u{2122}'),
            other => {
                // Numeric entity: &#123; or &#x1F4A;
                if let Some(rest) = other.strip_prefix('#') {
                    let codepoint = if let Some(hex) = rest.strip_prefix('x') {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        rest.parse::<u32>().ok()
                    };
                    if let Some(c) = codepoint.and_then(char::from_u32) {
                        result.push(c);
                    } else {
                        result.push('&');
                        result.push_str(other);
                        result.push(';');
                    }
                } else {
                    // Unknown named entity — keep as-is
                    result.push('&');
                    result.push_str(other);
                    result.push(';');
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_tags_basic() {
        assert_eq!(normalize_content("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn test_html_entities() {
        assert_eq!(normalize_content("AT&amp;T"), "AT&T");
        assert_eq!(normalize_content("a &lt; b &gt; c"), "a < b > c");
        assert_eq!(normalize_content("&quot;hi&quot;"), "\"hi\"");
    }

    #[test]
    fn test_numeric_entities() {
        assert_eq!(normalize_content("&#8217;"), "\u{2019}"); // right single quote
        assert_eq!(normalize_content("&#x2014;"), "\u{2014}"); // em dash
        assert_eq!(normalize_content("&#39;"), "'");
    }

    #[test]
    fn test_nbsp_collapses() {
        assert_eq!(normalize_content("hello&nbsp;&nbsp;world"), "hello world");
    }

    #[test]
    fn test_mixed_html_and_entities() {
        let input = "<div>Breaking: AT&amp;T &amp; Verizon&mdash;a new deal</div>";
        assert_eq!(normalize_content(input), "Breaking: AT&T & Verizon\u{2014}a new deal");
    }

    #[test]
    fn test_whitespace_collapse() {
        let input = "<div>  Hello   <span>  world  </span>  </div>";
        assert_eq!(normalize_content(input), "Hello world");
    }

    #[test]
    fn test_plain_text_passthrough() {
        assert_eq!(normalize_content("just plain text"), "just plain text");
    }

    #[test]
    fn test_empty() {
        assert_eq!(normalize_content(""), "");
    }

    #[test]
    fn test_malformed_entity_passthrough() {
        assert_eq!(normalize_content("5 &bananas"), "5 &bananas");
        assert_eq!(normalize_content("a & b"), "a & b");
    }

    #[test]
    fn test_style_script_head_stripped() {
        let input = "<html><head><title>T</title></head><body><style>.x{color:red}</style><p>Hello</p><script>alert(1)</script></body></html>";
        assert_eq!(normalize_content(input), "Hello");
    }

    #[test]
    fn test_full_html_email() {
        let input = "<!doctype html><html><head><meta charset=\"utf-8\"><style>body{font:14px Arial}</style></head><body><div>Check out this article</div></body></html>";
        assert_eq!(normalize_content(input), "Check out this article");
    }
}
