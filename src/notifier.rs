use crate::reddit::Post;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
}

pub fn notify_console(posts: &[Post], query_name: &str, limit: u32) {
    notify(posts, query_name, limit, Format::Table);
}

pub fn notify(posts: &[Post], query_name: &str, limit: u32, format: Format) {
    match format {
        Format::Json => notify_jsonl(posts, query_name, limit),
        Format::Table => notify_table(posts, query_name, limit),
    }
}

// ── ANSI helpers (hand-rolled, 0 deps) ──────────────────────────────────────
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";

fn bold(s: &str) -> String {
    format!("{}{}{}", BOLD, s, RESET)
}
fn dim(s: &str) -> String {
    format!("{}{}{}", DIM, s, RESET)
}
fn cyan(s: &str) -> String {
    format!("{}{}{}", CYAN, s, RESET)
}
fn score_colored(score: i64) -> String {
    let txt = format!("{}", score);
    if score >= 100 {
        format!("{}{}{}", GREEN, BOLD, format!("{}{}", txt, RESET))
    } else if score >= 20 {
        format!("{}{}{}", YELLOW, txt, RESET)
    } else {
        txt
    }
}

fn pad_right(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.chars().take(width).collect()
    } else {
        format!("{}{}", s, " ".repeat(width - len))
    }
}
fn pad_left(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.chars().take(width).collect()
    } else {
        format!("{}{}", " ".repeat(width - len), s)
    }
}
fn pad_center(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.chars().take(width).collect()
    } else {
        let left = (width - len) / 2;
        let right = width - len - left;
        format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
    }
}

fn truncate_plain(s: &str, max_chars: usize) -> String {
    let t: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        format!("{}…", t)
    } else {
        t
    }
}

fn notify_table(posts: &[Post], query_name: &str, limit: u32) {
    if posts.is_empty() {
        tracing::info!("[notify:{}] sin resultados nuevos", query_name);
        return;
    }
    let n = (limit.max(1) as usize).min(posts.len());
    tracing::info!("[notify:{}] {} nuevos:", query_name, n);

    // column widths (content width, not counting borders)
    const W_NUM: usize = 3;
    const W_SUB: usize = 16;
    const W_SCORE: usize = 7;
    const W_COMM: usize = 8;
    const W_TITLE: usize = 60;

    let top = format!("┌{}┬{}┬{}┬{}┬{}┐", "─".repeat(W_NUM), "─".repeat(W_SUB), "─".repeat(W_SCORE), "─".repeat(W_COMM), "─".repeat(W_TITLE));
    let sep = format!("├{}┼{}┼{}┼{}┼{}┤", "─".repeat(W_NUM), "─".repeat(W_SUB), "─".repeat(W_SCORE), "─".repeat(W_COMM), "─".repeat(W_TITLE));
    let bot = format!("└{}┴{}┴{}┴{}┴{}┘", "─".repeat(W_NUM), "─".repeat(W_SUB), "─".repeat(W_SCORE), "─".repeat(W_COMM), "─".repeat(W_TITLE));

    // header
    println!("{}", dim(&top));
    println!(
        "{} {} {} {} {} {} {} {} {} {} {}",
        dim("│"),
        bold(&pad_center("#", W_NUM)),
        dim("│"),
        bold(&pad_center("subreddit", W_SUB)),
        dim("│"),
        bold(&pad_center("score", W_SCORE)),
        dim("│"),
        bold(&pad_center("comments", W_COMM)),
        dim("│"),
        bold(&pad_right("title", W_TITLE)),
        dim("│")
    );
    println!("{}", dim(&sep));

    for (i, p) in posts.iter().take(n).enumerate() {
        let sub = format!("r/{}", p.subreddit.replace('|', "/").replace('\n', " "));
        let sub_padded = pad_right(&truncate_plain(&sub, W_SUB), W_SUB);
        let sub_colored = cyan(&sub_padded);

        let score_raw = format!("{}", p.score);
        let score_padded = pad_left(&score_raw, W_SCORE);
        // color after padding would break width, so color the trimmed then pad with spaces outside color?
        // Simpler: color the score string and then pad the colored string's visible length.
        // We cheat: pad_left on raw, then replace raw with colored version padded same way.
        let score_display = if p.score >= 100 {
            let c = score_colored(p.score);
            // pad_left for colored: we need visible width W_SCORE, so add spaces outside.
            let visible_len = score_raw.chars().count();
            let pad = " ".repeat(W_SCORE - visible_len);
            format!("{}{}", pad, c)
        } else if p.score >= 20 {
            let c = score_colored(p.score);
            let visible_len = score_raw.chars().count();
            let pad = " ".repeat(W_SCORE - visible_len);
            format!("{}{}", pad, c)
        } else {
            score_padded
        };

        let comm = format!("{}", p.num_comments);
        let comm_padded = pad_left(&comm, W_COMM);

        let title_raw = p.title.replace('|', "/").replace('\n', " ");
        let title_trunc = truncate_plain(&title_raw, W_TITLE);
        let title_padded = pad_right(&title_trunc, W_TITLE);

        // row 1: data
        println!(
            "{} {} {} {} {} {} {} {} {} {} {}",
            dim("│"),
            pad_left(&format!("{}", i + 1), W_NUM),
            dim("│"),
            sub_colored,
            dim("│"),
            score_display,
            dim("│"),
            comm_padded,
            dim("│"),
            title_padded,
            dim("│")
        );
        // row 2: full url on its own line — no truncation so it remains copy-pasteable
        // Previously this was truncated to W_TITLE (60 chars) inside the table cell,
        // which cut links like https://www.reddit.com/r/.../comments/... and broke copy-paste.
        let url_raw = p.url.replace('\n', " ");
        // Print outside table borders for clean selection/copy. Using plain URL
        // keeps `> file` clean; modern terminals still make https:// clickable
        // via auto-link detection, and we could wrap with OSC 8 if needed.
        println!("  {} {}", dim("↳"), url_raw);
        if i + 1 < n {
            println!("{}", dim(&sep));
        }
    }
    println!("{}", dim(&bot));
}

fn notify_jsonl(posts: &[Post], query_name: &str, limit: u32) {
    if posts.is_empty() {
        tracing::info!("[notify:{}] sin resultados nuevos", query_name);
        return;
    }
    let n = (limit.max(1) as usize).min(posts.len());
    tracing::info!("[notify:{}] {} nuevos (jsonl):", query_name, n);
    for p in posts.iter().take(n) {
        if let Ok(line) = serde_json::to_string(p) {
            println!("{}", line);
        }
    }
}

#[allow(dead_code)]
fn escape_pipe(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[allow(dead_code)]
fn truncate_and_escape(s: &str, max_chars: usize) -> String {
    let escaped = escape_pipe(s);
    let truncated: String = escaped.chars().take(max_chars).collect();
    if escaped.chars().count() > max_chars {
        format!("{}…", truncated)
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reddit::Post;

    fn sample_post(id: &str, title: &str) -> Post {
        Post {
            id: id.to_string(),
            title: title.to_string(),
            subreddit: "rust".to_string(),
            author: "testuser".to_string(),
            score: 10,
            num_comments: 2,
            created_utc: 1_700_000_000,
            url: "https://www.reddit.com/r/rust/comments/abc/title/".to_string(),
            selftext: "".to_string(),
            over_18: false,
        }
    }

    #[test]
    fn jsonl_serializes() {
        let p = sample_post("abc", "hello | world");
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"title\":\"hello | world\""));
        assert!(json.contains("\"subreddit\":\"rust\""));
    }

    #[test]
    fn table_escapes_pipe() {
        assert_eq!(escape_pipe("a|b|c"), "a\\|b\\|c");
        let t = truncate_and_escape("a|b|c", 10);
        assert_eq!(t, "a\\|b\\|c");
    }

    #[test]
    fn truncate_adds_ellipsis() {
        let long = "a".repeat(100);
        let t = truncate_and_escape(&long, 80);
        assert!(t.ends_with('…'));
        assert_eq!(t.chars().count(), 81);
    }

    #[test]
    fn pad_helpers() {
        assert_eq!(pad_right("hi", 5), "hi   ");
        assert_eq!(pad_left("hi", 5), "   hi");
        assert_eq!(pad_center("hi", 6), "  hi  ");
    }
}
