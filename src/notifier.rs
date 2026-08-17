use crate::reddit::Post;

pub fn notify_console(posts: &[Post], query_name: &str) {
    if posts.is_empty() {
        tracing::info!("[notify:{}] sin resultados nuevos", query_name);
        return;
    }
    tracing::info!("[notify:{}] {} nuevos:", query_name, posts.len());
    for p in posts.iter().take(5) {
        println!("  >> r/{} | {}↑ {}💬 | {}", p.subreddit, p.score, p.num_comments, p.title);
        println!("     https://www.reddit.com{}", p.url.replace("https://www.reddit.com", ""));
        // ensure full url printed
        if !p.url.contains("reddit.com") {
            println!("     {}", p.url);
        } else {
            println!("     {}", p.url);
        }
        if !p.selftext.is_empty() {
            println!("     {}", &p.selftext[..p.selftext.len().min(120)]);
        }
        println!();
    }
}
