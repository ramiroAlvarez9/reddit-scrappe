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

const STEALTH_JS: &str = r#"
    try { Object.defineProperty(navigator, 'webdriver', {get: () => undefined}); } catch(e) {}
    try { Object.defineProperty(Object.getPrototypeOf(navigator), 'webdriver', {get: () => false}); } catch(e) {}
    try { window.chrome = {runtime: {}, loadTimes: function(){}, csi: function(){}}; } catch(e) {}
    try { Object.defineProperty(navigator, 'languages', {get: () => ['en-US', 'en']}); } catch(e) {}
    try { Object.defineProperty(navigator, 'plugins', {get: () => [{}, {}, {}, {}, {}]}); } catch(e) {}
    try { Object.defineProperty(navigator, 'platform', {get: () => 'MacIntel'}); } catch(e) {}
    try { window.navigator.permissions.query = (p) => p.name==='notifications' ? Promise.resolve({state: Notification.permission}) : Promise.resolve({state: 'granted'}); } catch(e) {}
"#;

async fn apply_stealth(page: &chromiumoxide::Page) {
    let _ = page.evaluate_on_new_document(STEALTH_JS).await;
    let _ = page.evaluate(STEALTH_JS).await;
}

pub async fn search_human(page: &chromiumoxide::Page, query: &str, subreddits: &[String], sort: &str) -> anyhow::Result<Vec<Post>> {
    // stealth JS before any navigation
    apply_stealth(page).await;
    // warm-up via proxy+human to establish session (helps WAF L2/L3)
    if std::env::var("DI_USER").is_ok() {
        tracing::info!("[warmup] goto https://www.reddit.com/ via proxy before search");
        let _ = page.goto("https://www.reddit.com/").await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        apply_stealth(page).await;
        let _ = crate::human::human_scroll(page).await;
        crate::human::sleep_jitter(1000, 2000).await;
    }
    let targets = if subreddits.is_empty() { vec!["".to_string()] } else { subreddits.to_vec() };
    let mut all = Vec::new();
    for sub in targets {
        let url = if sub.is_empty() {
            format!("https://www.reddit.com/search/?q={}&sort={}&t=week", urlencoding(query), sort)
        } else {
            format!("https://www.reddit.com/r/{}/search/?q={}&sort={}&t=week&restrict_sr=1", sub, urlencoding(query), sort)
        };
        tracing::info!("[query:{}] goto {}", if sub.is_empty(){"all".to_string()} else {sub.clone()}, url);
        apply_stealth(page).await;
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
                    // fallback json -> html -> browser con cookies + proxy
                    let cookie_header = get_reddit_cookie_header_via_page(page).await;
                    let json_fallback = search_old_reddit_json(query, &sub, cookie_header.clone(), 25).await.unwrap_or_default();
                    if !json_fallback.is_empty() {
                        tracing::info!("[fallback json] old.reddit json devolvió {} posts", json_fallback.len());
                        all.extend(json_fallback);
                        continue;
                    }
                    let fallback = search_fallback_old_reddit_with_cookies(query, sub.clone(), cookie_header.clone()).await.unwrap_or_default();
                    if !fallback.is_empty() {
                        tracing::info!("[fallback] old.reddit reqwest devolvió {} posts", fallback.len());
                        all.extend(fallback);
                        continue;
                    }
                    let browser_fallback = search_old_reddit_via_browser(page, query, &sub).await.unwrap_or_default();
                    tracing::info!("[fallback browser] old.reddit via browser devolvió {} posts", browser_fallback.len());
                    all.extend(browser_fallback);
                    continue;
                }
                // fallback ligero json -> html -> browser
                let cookie_header = get_reddit_cookie_header_via_page(page).await;
                let json_fallback = search_old_reddit_json(query, &sub, cookie_header.clone(), 25).await.unwrap_or_default();
                if !json_fallback.is_empty() {
                    tracing::info!("[fallback json] old.reddit json devolvió {} posts (sin shreddit-post)", json_fallback.len());
                    all.extend(json_fallback);
                    continue;
                }
                let fallback = search_fallback_old_reddit_with_cookies(query, sub.clone(), cookie_header.clone()).await.unwrap_or_default();
                if !fallback.is_empty() {
                    tracing::info!("[fallback] old.reddit reqwest devolvió {} posts (sin shreddit-post)", fallback.len());
                    all.extend(fallback);
                    continue;
                }
                let browser_fallback = search_old_reddit_via_browser(page, query, &sub).await.unwrap_or_default();
                if !browser_fallback.is_empty() {
                    tracing::info!("[fallback browser] old.reddit via browser (sin shreddit) devolvió {} posts", browser_fallback.len());
                    all.extend(browser_fallback);
                    continue;
                }
            }
        } else if let Ok(html) = page.content().await {
            if is_captcha(&html) {
                tracing::warn!("[captcha] Prove your humanity detectado - aborto query, fallback ligero");
                let cookie_header = get_reddit_cookie_header_via_page(page).await;
                let json_fallback = search_old_reddit_json(query, &sub, cookie_header.clone(), 25).await.unwrap_or_default();
                if !json_fallback.is_empty() {
                    tracing::info!("[fallback json] old.reddit json devolvió {} posts", json_fallback.len());
                    all.extend(json_fallback);
                    continue;
                }
                let fallback = search_fallback_old_reddit_with_cookies(query, sub.clone(), cookie_header.clone()).await.unwrap_or_default();
                if !fallback.is_empty() {
                    tracing::info!("[fallback] old.reddit reqwest devolvió {} posts", fallback.len());
                    all.extend(fallback);
                    continue;
                }
                let browser_fallback = search_old_reddit_via_browser(page, query, &sub).await.unwrap_or_default();
                tracing::info!("[fallback browser] old.reddit via browser devolvió {} posts", browser_fallback.len());
                all.extend(browser_fallback);
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

/// No-browser search: uses only reqwest Proxy + decrypted cookies from `cookies.json`.
/// This is the light path: no Chromium, ~60KB/query vs ~300KB with browser, ~28MB RSS vs ~630MB.
pub async fn search_no_browser(query: &str, subreddits: &[String], sort: &str, limit: u32) -> anyhow::Result<Vec<Post>> {
    let cookie_header = get_cookie_header_for_no_browser();
    if cookie_header.is_none() {
        tracing::warn!("[no-browser] no valid cookies at {} — run `cargo run -- --login` (with DI_COUNTRY/DI_SESSION to bind to residential IP). Trying anonymous (likely 403)", crate::cookies::cookies_file_path().display());
        tracing::warn!("[no-browser] cookies status: {}", crate::cookies::cookies_status());
    } else if let Some(ref c) = cookie_header {
        tracing::info!("[no-browser] using {} bytes reddit cookies ({} parts) from {}", c.len(), c.split(';').count(), crate::cookies::cookies_file_path().display());
    }
    let targets = if subreddits.is_empty() { vec!["".to_string()] } else { subreddits.to_vec() };
    let mut all = Vec::new();
    for sub in targets {
        let label = if sub.is_empty() { "all".to_string() } else { sub.clone() };
        tracing::info!("[no-browser:{}] search \"{}\" sort={} limit={}", label, query, sort, limit);
        // 1) JSON endpoint (primary) — includes selftext, bypasses shreddit JS
        let json_posts = search_old_reddit_json(query, &sub, cookie_header.clone(), limit).await.unwrap_or_default();
        if !json_posts.is_empty() {
            tracing::info!("[no-browser:{}] json returned {} posts", label, json_posts.len());
            all.extend(json_posts);
            continue;
        }
        tracing::warn!("[no-browser:{}] json returned 0, trying html fallback", label);
        // 2) HTML fallback via old.reddit reqwest + cookies
        let html_posts = search_fallback_old_reddit_with_cookies(query, sub.clone(), cookie_header.clone()).await.unwrap_or_default();
        if !html_posts.is_empty() {
            tracing::info!("[no-browser:{}] html fallback returned {} posts", label, html_posts.len());
            all.extend(html_posts);
            continue;
        }
        tracing::warn!("[no-browser:{}] both json+html 0 — cookie may be expired/invalid or IP not residential. Try `DI_COUNTRY=us cargo run -- --login` then retry. Status: {}", label, crate::cookies::cookies_status());
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

async fn get_reddit_cookie_header_via_page(page: &chromiumoxide::Page) -> Option<String> {
    // Use CDP get_cookies via page (handles encrypted cookies via browser)
    if let Ok(cookies) = page.get_cookies().await {
        let parts: Vec<String> = cookies.iter()
            .filter(|c| c.domain.contains("reddit"))
            .filter(|c| !c.value.is_empty())
            .map(|c| format!("{}={}", c.name, c.value))
            .collect();
        if !parts.is_empty() { return Some(parts.join("; ")); }
    }
    // fallback to file (for cases without page, but values may be encrypted)
    get_reddit_cookie_header_from_file()
}

fn get_reddit_cookie_header_from_file() -> Option<String> {
    let profile = crate::login::profile_dir();
    let db_path = profile.join("Default").join("Cookies");
    if !db_path.exists() { return None; }
    // copy to temp to avoid lock
    let tmp = std::env::temp_dir().join(format!("reddit_cookies_{}.db", std::process::id()));
    let _ = std::fs::copy(&db_path, &tmp);
    let conn = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    // Try both value and encrypted_value (Chrome 151 encrypts, so value may be empty)
    let mut stmt = conn.prepare("SELECT name, value, host_key FROM cookies WHERE host_key LIKE '%reddit%'").ok()?;
    let mut rows = stmt.query([]).ok()?;
    let mut cookies = Vec::new();
    while let Ok(Some(row)) = rows.next() {
        let name: String = row.get(0).unwrap_or_default();
        let value: String = row.get(1).unwrap_or_default();
        if name.is_empty() || value.is_empty() { continue; }
        cookies.push(format!("{}={}", name, value));
    }
    let _ = std::fs::remove_file(&tmp);
    if cookies.is_empty() { None } else { Some(cookies.join("; ")) }
}

fn get_reddit_cookie_header() -> Option<String> {
    // Priority: decrypted JSON via cookies module (CDP) -> sqlite file fallback
    if let Some(h) = crate::cookies::load_cookie_header() {
        return Some(h);
    }
    get_reddit_cookie_header_from_file()
}

/// Cookie header for no-browser mode: only the decrypted JSON file (no Page available).
/// sqlite fallback kept for legacy but will be empty on Chrome 151 due to encryption.
pub fn get_cookie_header_for_no_browser() -> Option<String> {
    if let Some(h) = crate::cookies::load_cookie_header() {
        return Some(h);
    }
    // legacy sqlite fallback (rarely contains valid values on Chrome 151)
    get_reddit_cookie_header_from_file()
}

fn build_reqwest_proxy() -> Option<reqwest::Proxy> {
    let user = std::env::var("DI_USER").ok()?;
    let pass = std::env::var("DI_PASS").ok()?;
    if user.trim().is_empty() || pass.trim().is_empty() { return None; }
    let country = std::env::var("DI_COUNTRY").ok();
    let city = std::env::var("DI_CITY").ok();
    let session = std::env::var("DI_SESSION").ok();
    let use_socks = std::env::var("DI_USE_SOCKS").map(|v| v=="1" || v.to_lowercase()=="true").unwrap_or(false);
    let scheme = if use_socks { "socks5" } else { "http" };
    let port = if use_socks { 824 } else { 823 };
    // build proxy username like dataimpulse-mcp
    let mut params: Vec<String> = Vec::new();
    if let Some(c) = country.as_deref() {
        params.push(format!("cr.{}", c.to_lowercase()));
    }
    if let Some(city) = city.as_deref() {
        if !country.is_none() { params.push(format!("city.{}", city.to_lowercase())); }
    }
    if let Some(sess) = session.as_deref() {
        params.push(format!("sessid.{}", sess.replace(' ', "_")));
    }
    let proxy_user = if params.is_empty() { user.clone() } else { format!("{}__{}", user, params.join(";")) };
    let mut encoded_user = proxy_user.clone();
    if proxy_user.contains('@') || proxy_user.contains(':') || proxy_user.contains(' ') {
        encoded_user = proxy_user.replace('%', "%25").replace('@', "%40").replace(':', "%3A").replace(' ', "%20");
    }
    let mut encoded_pass = String::new();
    for b in pass.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') { encoded_pass.push(c); }
        else { encoded_pass.push_str(&format!("%{:02X}", b)); }
    }
    let proxy_url = format!("{}://{}:{}@gw.dataimpulse.com:{}", scheme, encoded_user, encoded_pass, port);
    match reqwest::Proxy::all(proxy_url) {
        Ok(p) => {
            tracing::info!("[fallback proxy] enabled via gw.dataimpulse.com:{} country={} city={} session={} scheme={}", port, country.as_deref().unwrap_or("-"), city.as_deref().unwrap_or("-"), session.as_deref().unwrap_or("-"), scheme);
            Some(p)
        },
        Err(e) => { tracing::warn!("[fallback proxy] invalid proxy: {}", e); None }
    }
}

#[allow(dead_code)]
pub async fn search_fallback_old_reddit(query: &str, sub: String) -> anyhow::Result<Vec<Post>> {
    search_fallback_old_reddit_with_cookies(query, sub, None).await
}

pub async fn search_fallback_old_reddit_with_cookies(query: &str, sub: String, cookie_header: Option<String>) -> anyhow::Result<Vec<Post>> {
    // Fallback ligero: old.reddit via reqwest + proxy + login cookies
    let url = if sub.is_empty() {
        format!("https://old.reddit.com/search?q={}&sort=new&t=week", urlencoding(query))
    } else {
        format!("https://old.reddit.com/r/{}/search?q={}&sort=new&t=week&restrict_sr=on", sub, urlencoding(query))
    };
    tracing::info!("[fallback] GET {}", url);
    let mut builder = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(15));
    if let Some(proxy) = build_reqwest_proxy() {
        builder = builder.proxy(proxy);
    }
    let client = builder.build()?;
    let mut req = client.get(&url)
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Referer", "https://old.reddit.com/");
    let cookie_to_use = cookie_header.or_else(get_reddit_cookie_header);
    if let Some(ref cookie_header) = cookie_to_use {
        tracing::info!("[fallback] using {} bytes reddit cookies ({} parts)", cookie_header.len(), cookie_header.split(';').count());
        req = req.header("Cookie", cookie_header.clone());
    } else {
        tracing::info!("[fallback] no reddit cookies found, anonymous");
    }
    let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{:?}", e);
                tracing::warn!("[fallback] request failed debug: {}", msg);
                let msg_str = e.to_string();
                if msg_str.contains("407") {
                    if msg_str.contains("TRAFFIC_EXHAUSTED") { tracing::warn!("[fallback] 407 TRAFFIC_EXHAUSTED sin gigas"); }
                    else if msg_str.contains("THREADS_EXHAUSTED") { tracing::warn!("[fallback] 407 THREADS_EXHAUSTED >2000 conns"); }
                    else { tracing::warn!("[fallback] 407 proxy auth failed: {}", msg_str); }
                } else if msg_str.contains("503") || msg_str.contains("NO_RAY") {
                    tracing::warn!("[fallback] 503 NO_RAY sin IPs para ese targeting (quitá city) - {}", msg_str);
                } else {
                    tracing::warn!("[fallback] request failed: {}", msg_str);
                }
                // try without proxy as fallback if proxy was used
                if build_reqwest_proxy().is_some() {
                    tracing::info!("[fallback] retry without proxy but with cookies");
                    let builder2 = reqwest::Client::builder()
                        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36")
                        .timeout(std::time::Duration::from_secs(15));
                    let client2 = builder2.build()?;
                    let mut req2 = client2.get(&url)
                        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                        .header("Accept-Language", "en-US,en;q=0.9")
                        .header("Referer", "https://old.reddit.com/");
                    if let Some(cookie) = cookie_to_use.clone() {
                        req2 = req2.header("Cookie", cookie);
                    }
                    match req2.send().await {
                        Ok(r2) => {
                            let status2 = r2.status();
                            if status2.is_success() {
                                let html2 = r2.text().await?;
                                if !html2.to_lowercase().contains("blocked by network security") {
                                    let posts2 = parse_old_reddit_html(&html2);
                                    if !posts2.is_empty() {
                                        tracing::info!("[fallback] retry without proxy succeeded {} posts", posts2.len());
                                        return Ok(posts2);
                                    }
                                }
                            }
                            tracing::warn!("[fallback] retry without proxy status {}", status2);
                        },
                        Err(e2) => tracing::warn!("[fallback] retry without proxy failed: {:?}", e2),
                    }
                }
                return Ok(vec![]);
            }
        };
    let status = resp.status().as_u16();
    if status == 429 || status == 403 {
        tracing::warn!("[fallback] {} blocked/rate limited, skip (Reddit bloquea anon sin login; probá cambiar country/session o usar login + proxy browser)", resp.status());
        if status == 403 {
            if let Ok(body) = resp.text().await {
                if body.to_lowercase().contains("traffic_exhausted") { tracing::warn!("[fallback] 407 TRAFFIC_EXHAUSTED"); }
                else if body.contains("NO_RAY") { tracing::warn!("[fallback] 503 NO_RAY - quitá city"); }
                tracing::debug!("[fallback] 403 body preview: {}", body.chars().take(500).collect::<String>());
            }
        }
        return Ok(vec![]);
    }
    if status == 407 {
        tracing::warn!("[fallback] 407 proxy auth (TRAFFIC/THREADS) - revisá DI_USER/DI_PASS y gigas");
        return Ok(vec![]);
    }
    if status == 503 {
        tracing::warn!("[fallback] 503 NO_RAY - quitá city targeting");
        return Ok(vec![]);
    }
    if !resp.status().is_success() {
        tracing::warn!("[fallback] HTTP {} unexpected", resp.status());
        return Ok(vec![]);
    }
    let html = resp.text().await?;
    if html.to_lowercase().contains("blocked by network security") || html.to_lowercase().contains("prove your humanity") {
        tracing::warn!("[fallback] captcha/block detectado en old.reddit html");
        return Ok(vec![]);
    }
    // old.reddit usa div.thing con data-attrs
    let posts = parse_old_reddit_html(&html);
    Ok(posts)
}

/// Fallback via browser (chromiumoxide) for old.reddit when reqwest 403
pub async fn search_old_reddit_via_browser(page: &chromiumoxide::Page, query: &str, sub: &str) -> anyhow::Result<Vec<Post>> {
    let url = if sub.is_empty() {
        format!("https://old.reddit.com/search?q={}&sort=new&t=week", urlencoding(query))
    } else {
        format!("https://old.reddit.com/r/{}/search?q={}&sort=new&t=week&restrict_sr=on", sub, urlencoding(query))
    };
    tracing::info!("[fallback browser] goto {}", url);
    apply_stealth(page).await;
    page.goto(&url).await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    crate::human::human_scroll(page).await?;
    for _ in 0..5 {
        if page.find_element("div.thing").await.is_ok() { break; }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    let html = page.content().await.unwrap_or_default();
    if is_captcha(&html) {
        tracing::warn!("[fallback browser] captcha en old.reddit via browser");
        return Ok(vec![]);
    }
    Ok(parse_old_reddit_html(&html))
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

fn parse_reddit_json(json_str: &str) -> Vec<Post> {
    let v: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(j) => j,
        Err(_) => return vec![],
    };
    let children = v.get("data").and_then(|d| d.get("children")).and_then(|c| c.as_array());
    let Some(children) = children else { return vec![]; };
    let mut out = Vec::new();
    for child in children {
        let data = match child.get("data") { Some(d) => d, None => continue };
        let id = data.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if id.is_empty() { continue; }
        let title = data.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if title.is_empty() { continue; }
        let subreddit = data.get("subreddit").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let author = data.get("author").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let score = data.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let comments = data.get("num_comments").and_then(|v| v.as_i64()).unwrap_or(0);
        let permalink = data.get("permalink").and_then(|v| v.as_str()).unwrap_or("#").to_string();
        let url = if permalink.starts_with("http") { permalink } else { format!("https://www.reddit.com{}", permalink) };
        let over_18 = data.get("over_18").and_then(|v| v.as_bool()).unwrap_or(false);
        let created = data.get("created_utc").and_then(|v| v.as_f64()).unwrap_or(chrono::Utc::now().timestamp() as f64) as u64;
        out.push(Post {
            id,
            title,
            subreddit,
            author,
            score,
            num_comments: comments,
            created_utc: created,
            url,
            selftext: data.get("selftext").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            over_18,
        });
    }
    out
}

pub async fn search_old_reddit_json(query: &str, sub: &str, cookie_header: Option<String>, limit: u32) -> anyhow::Result<Vec<Post>> {
    let url = if sub.is_empty() {
        format!("https://old.reddit.com/search.json?q={}&sort=new&t=week&limit={}", urlencoding(query), limit)
    } else {
        format!("https://old.reddit.com/r/{}/search.json?q={}&sort=new&t=week&restrict_sr=on&limit={}", sub, urlencoding(query), limit)
    };
    tracing::info!("[fallback json] GET {}", url);
    let mut builder = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(15));
    if let Some(proxy) = build_reqwest_proxy() {
        builder = builder.proxy(proxy);
    }
    let client = builder.build()?;
    let mut req = client.get(&url)
        .header("Accept", "application/json")
        .header("Accept-Language", "en-US,en;q=0.9");
    let cookie = cookie_header.or_else(get_reddit_cookie_header);
    if let Some(c) = cookie {
        req = req.header("Cookie", c);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("[fallback json] request failed: {:?}", e);
            return Ok(vec![]);
        }
    };
    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!("[fallback json] HTTP {} body {}", status, body.chars().take(500).collect::<String>());
        return Ok(vec![]);
    }
    let body = resp.text().await?;
    if body.to_lowercase().contains("blocked by network security") || body.to_lowercase().contains("prove your humanity") {
        tracing::warn!("[fallback json] captcha in json body");
        return Ok(vec![]);
    }
    let posts = parse_reddit_json(&body);
    tracing::info!("[fallback json] parsed {} posts from json", posts.len());
    Ok(posts)
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
