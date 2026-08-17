use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub title: String,
    pub subreddit: String,
    pub author: String,
    pub score: i64,
    pub num_comments: i64,
    pub created_utc: u64,
    pub url: String,
    pub selftext: String,
    pub over_18: bool,
}

// Parse HTML fragment with scraper crate - used for unit tests and fallback
pub fn parse_reddit_html(html: &str) -> Vec<Post> {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    let sel = Selector::parse("shreddit-post").unwrap();
    let mut out = Vec::new();
    for el in doc.select(&sel) {
        let id = el.value().attr("id").or_else(|| el.value().attr("post-id")).unwrap_or("").to_string();
        let title = el.value().attr("post-title").or_else(|| el.value().attr("title")).unwrap_or("").to_string();
        let subreddit = el.value().attr("subreddit-prefixed").unwrap_or("r/unknown").trim_start_matches("r/").to_string();
        let author = el.value().attr("author").unwrap_or("unknown").to_string();
        let score = el.value().attr("score").and_then(|v| v.parse().ok()).unwrap_or(0);
        let comments = el.value().attr("comment-count").or_else(|| el.value().attr("num-comments")).and_then(|v| v.parse().ok()).unwrap_or(0);
        let permalink = el.value().attr("permalink").or_else(|| el.value().attr("content-href")).unwrap_or("#").to_string();
        let url = if permalink.starts_with("http") { permalink } else { format!("https://www.reddit.com{}", permalink) };
        let over_18 = el.value().attr("over18").is_some();
        if id.is_empty() && title.is_empty() { continue; }
        out.push(Post {
            id: if id.is_empty() { format!("post-{}", out.len()) } else { id },
            title: title.clone(),
            subreddit,
            author,
            score,
            num_comments: comments,
            created_utc: chrono::Utc::now().timestamp() as u64,
            url,
            selftext: "".into(),
            over_18,
        });
    }
    out
}

pub async fn search_human(page: &chromiumoxide::Page, query: &str, subreddits: &[String], sort: &str) -> anyhow::Result<Vec<Post>> {
    let targets = if subreddits.is_empty() { vec!["".to_string()] } else { subreddits.to_vec() };
    let mut all = Vec::new();
    for sub in targets {
        let url = if sub.is_empty() {
            format!("https://www.reddit.com/search/?q={}&sort={}&t=week", urlencoding(query), sort)
        } else {
            format!("https://www.reddit.com/r/{}/search/?q={}&sort={}&t=week&restrict_sr=1", sub, urlencoding(query), sort)
        };
        tracing::info!("[query:{}] goto {}", if sub.is_empty(){"all".to_string()} else {sub.clone()}, url);
        let nav = page.goto(&url).await;
        tracing::info!("[nav] goto result: {:?}", nav.is_ok());
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if let Ok(u) = page.url().await {
            tracing::info!("[nav] current url: {:?}", u);
        }
        if let Ok(title) = page.get_title().await {
            tracing::info!("[nav] title: {:?}", title);
        }
        // wait for posts - poll find_element up to 15s
        let mut found = false;
        for _ in 0..15 {
            if page.find_element("shreddit-post").await.is_ok() {
                tracing::info!("[extract] shreddit-post found");
                found = true;
                break;
            }
            // also try alternative selectors
            if page.find_element("div[data-testid='post-container']").await.is_ok() || page.find_element("article").await.is_ok() {
                tracing::info!("[extract] alternative post selector found");
                found = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        if !found {
            tracing::warn!("[extract] timeout waiting shreddit-post 15s");
            if let Ok(html) = page.content().await {
                tracing::warn!("[debug] html len={} snippet: {}", html.len(), &html[..html.len().min(500)].replace('\n', " "));
                let _ = std::fs::write("/tmp/reddit_debug.html", &html);
                tracing::warn!("[debug] dumped html to /tmp/reddit_debug.html");
                if is_captcha(&html) {
                    tracing::warn!("[captcha] Prove your humanity detectado - abortando browser para este query, fallback a old.reddit sin reintento para no bannear");
                    // no reintento inmediato, fallback ligero después
                    let fallback = search_fallback_old_reddit(query, sub.clone()).await.unwrap_or_default();
                    tracing::info!("[fallback] old.reddit devolvió {} posts", fallback.len());
                    all.extend(fallback);
                    continue;
                }
                // fallback ligero sin hammering
                let fallback = search_fallback_old_reddit(query, sub.clone()).await.unwrap_or_default();
                if !fallback.is_empty() {
                    tracing::info!("[fallback] old.reddit devolvió {} posts (sin shreddit-post)", fallback.len());
                    all.extend(fallback);
                    continue;
                }
            }
        } else if let Ok(html) = page.content().await {
            if is_captcha(&html) {
                tracing::warn!("[captcha] Prove your humanity detectado - aborto query, fallback ligero");
                let fallback = search_fallback_old_reddit(query, sub.clone()).await.unwrap_or_default();
                tracing::info!("[fallback] old.reddit devolvió {} posts", fallback.len());
                all.extend(fallback);
                continue;
            }
        }
        // dismiss login wall if present
        let _ = try_dismiss_wall(page).await;

        crate::human::human_scroll(page).await?;

        let html = page.content().await.unwrap_or_default();
        let posts = parse_reddit_html(&html);
        tracing::info!("[extract] found {} raw posts for r/{}", posts.len(), if sub.is_empty(){"all"} else {&sub});
        all.extend(posts);
        crate::human::sleep_jitter(1500, 3000).await;
    }
    Ok(all)
}

async fn try_dismiss_wall(page: &chromiumoxide::Page) -> anyhow::Result<()> {
    // try common dismiss selectors
    for sel in ["button:has-text('Continue')", "button[aria-label='Close']", "[data-testid='close']"] {
        if let Ok(el) = page.find_element(sel).await {
            let _ = el.click().await;
            tracing::debug!("[human] dismissed wall via {}", sel);
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            break;
        }
    }
    Ok(())
}

pub fn is_captcha(html: &str) -> bool {
    let low = html.to_lowercase();
    low.contains("captcha") || low.contains("cf-challenge") || low.contains("cf-turnstile") || low.contains("hcaptcha") || low.contains("recaptcha") || low.contains("blocked by network security") || low.contains("prove your humanity")
}

pub async fn search_fallback_old_reddit(query: &str, sub: String) -> anyhow::Result<Vec<Post>> {
    // Fallback ligero sin browser: intenta old.reddit search html parse (no API JSON que pide auth)
    // Usa reqwest con UA humano, sin hammer - 1 request por query
    let url = if sub.is_empty() {
        format!("https://old.reddit.com/search?q={}&sort=new&t=week", urlencoding(query))
    } else {
        format!("https://old.reddit.com/r/{}/search?q={}&sort=new&t=week&restrict_sr=on", sub, urlencoding(query))
    };
    tracing::info!("[fallback] GET {}", url);
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp = client.get(&url).send().await?;
    if resp.status().as_u16() == 429 || resp.status().as_u16() == 403 {
        tracing::warn!("[fallback] {} blocked/rate limited, skip (Reddit bloquea anon sin login)", resp.status());
        return Ok(vec![]);
    }
    let html = resp.text().await?;
    // old.reddit usa div.thing con data-attrs
    let posts = parse_old_reddit_html(&html);
    Ok(posts)
}

fn parse_old_reddit_html(html: &str) -> Vec<Post> {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    let sel = Selector::parse("div.thing[data-fullname]").unwrap();
    let mut out = Vec::new();
    for el in doc.select(&sel) {
        let id = el.value().attr("data-fullname").unwrap_or("").replace("t3_", "");
        let title = el.select(&Selector::parse("a.title").unwrap()).next().map(|e| e.text().collect::<String>().trim().to_string()).unwrap_or_default();
        if title.is_empty() { continue; }
        let subreddit = el.value().attr("data-subreddit").unwrap_or("unknown").to_string();
        let author = el.value().attr("data-author").unwrap_or("unknown").to_string();
        let score = el.value().attr("data-score").and_then(|v| v.parse().ok()).unwrap_or(0);
        let comments = el.value().attr("data-comments-count").and_then(|v| v.parse().ok()).unwrap_or(0);
        let permalink = el.value().attr("data-permalink").unwrap_or("#").to_string();
        let url = format!("https://www.reddit.com{}", permalink);
        let over_18 = el.value().attr("data-nsfw") == Some("true");
        out.push(Post {
            id: if id.is_empty() { format!("old-{}", out.len()) } else { id },
            title,
            subreddit,
            author,
            score,
            num_comments: comments,
            created_utc: chrono::Utc::now().timestamp() as u64,
            url,
            selftext: "".into(),
            over_18,
        });
    }
    out
}

fn urlencoding(s: &str) -> String {
    s.replace(' ', "+")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_html_golden() {
        let html = r#"<shreddit-post id="t3_abc" post-title="Hello Rust" subreddit-prefixed="r/rust" author="testuser" score="42" comment-count="7" permalink="/r/rust/comments/abc/hello/"></shreddit-post>
        <shreddit-post id="t3_def" post-title="Second" subreddit-prefixed="r/rust" author="a2" score="5" comment-count="1" permalink="/r/rust/comments/def/second/"></shreddit-post>"#;
        let posts = parse_reddit_html(html);
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].title, "Hello Rust");
        assert_eq!(posts[0].score, 42);
        assert_eq!(posts[0].subreddit, "rust");
    }
    #[test]
    fn parse_old_reddit_golden() {
        let html = r#"<div class="thing" data-fullname="t3_xyz" data-subreddit="rust" data-author="u1" data-score="10" data-comments-count="3" data-permalink="/r/rust/comments/xyz/title/"><a class="title">Old Title</a></div>"#;
        let posts = parse_old_reddit_html(html);
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].title, "Old Title");
        assert_eq!(posts[0].subreddit, "rust");
    }
    #[test]
    fn captcha_detect() {
        assert!(is_captcha("<div>captcha challenge</div>"));
        assert!(is_captcha("<title>Reddit - Prove your humanity</title><script src=\"recaptcha/api.js\""));
        assert!(!is_captcha("<shreddit-post></shreddit-post>"));
    }
}

// tiny urlencode helper to avoid extra dep
mod urlencoding_helper { pub fn _unused() {} }
