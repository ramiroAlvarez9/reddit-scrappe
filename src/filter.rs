use crate::config::Filters;
use crate::reddit::Post;
use chrono::Utc;

pub fn filter_posts(mut posts: Vec<Post>, filters: &Filters) -> Vec<Post> {
    let now = Utc::now().timestamp() as u64;
    let max_age = filters.max_age_hours * 3600;

    posts.retain(|p| {
        let age = now.saturating_sub(p.created_utc);
        if age > max_age { return false; }
        if p.score < filters.min_score as i64 { return false; }
        if p.num_comments < filters.min_comments as i64 { return false; }
        if filters.exclude_nsfw && p.over_18 { return false; }
        true
    });

    // dedup + sort by engagement
    let mut seen = std::collections::HashSet::new();
    let mut uniq = Vec::new();
    for p in posts {
        if seen.insert(p.id.clone()) {
            uniq.push(p);
        }
    }
    uniq.sort_by(|a,b| {
        let sa = a.score + a.num_comments * 2;
        let sb = b.score + b.num_comments * 2;
        sb.cmp(&sa)
    });
    uniq
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reddit::Post;

    fn mk(id: &str, score: i64, comments: i64, age_hours: i64, nsfw: bool) -> Post {
        Post {
            id: id.into(),
            title: "t".into(),
            subreddit: "rust".into(),
            author: "a".into(),
            score,
            num_comments: comments,
            created_utc: (Utc::now().timestamp() - age_hours*3600) as u64,
            url: "https://reddit.com/r/rust/1".into(),
            selftext: "".into(),
            over_18: nsfw,
        }
    }

    #[test]
    fn filters_by_score_and_age() {
        let f = Filters { min_score: 5, min_comments: 0, max_age_hours: 24, exclude_nsfw: true };
        let posts = vec![
            mk("1", 10, 0, 1, false), // keep
            mk("2", 1, 0, 1, false),  // low score
            mk("3", 10, 0, 30, false), // old
            mk("4", 10, 0, 1, true),  // nsfw
        ];
        let out = filter_posts(posts, &f);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "1");
    }

    #[test]
    fn dedup_and_sort() {
        let f = Filters { min_score: 0, min_comments: 0, max_age_hours: 48, exclude_nsfw: false };
        let posts = vec![mk("1", 5, 10, 1, false), mk("1", 5, 10, 1, false), mk("2", 1, 0, 1, false)];
        let out = filter_posts(posts, &f);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "1"); // higher engagement first
    }
}
