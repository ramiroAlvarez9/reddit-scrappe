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

fn notify_table(posts: &[Post], query_name: &str, limit: u32) {
    if posts.is_empty() {
        tracing::info!("[notify:{}] sin resultados nuevos", query_name);
        return;
    }
    let n = (limit.max(1) as usize).min(posts.len());
    tracing::info!("[notify:{}] {} nuevos:", query_name, n);
    // markdown table header
    println!("| # | subreddit | score | comments | title | url |");
    println!("|---|-----------|-------|----------|-------|-----|");
    for (i, p) in posts.iter().take(n).enumerate() {
        let title = truncate_and_escape(&p.title, 80);
        let url = &p.url;
        // escape pipes in title/url (markdown table separator)
        println!(
            "| {} | r/{} | {} | {} | {} | {} |",
            i + 1,
            escape_pipe(&p.subreddit),
            p.score,
            p.num_comments,
            title,
            escape_pipe(url)
        );
    }
}

fn notify_jsonl(posts: &[Post], query_name: &str, limit: u32) {
    if posts.is_empty() {
        tracing::info!("[notify:{}] sin resultados nuevos", query_name);
        return;
    }
    let n = (limit.max(1) as usize).min(posts.len());
    tracing::info!("[notify:{}] {} nuevos (jsonl):", query_name, n);
    for p in posts.iter().take(n) {
        // serde_json handles escaping
        if let Ok(line) = serde_json::to_string(p) {
            println!("{}", line);
        }
    }
}

fn escape_pipe(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

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
}
