use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub queries: Vec<Query>,
    #[serde(default = "default_filters")]
    pub filters: Filters,
    #[serde(default = "default_schedule")]
    pub schedule_minutes: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub notifier: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Query {
    pub name: String,
    pub q: String,
    #[serde(default)]
    pub subreddits: Vec<String>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_limit")]
    #[allow(dead_code)]
    pub limit: u32,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Filters {
    pub min_score: i32,
    pub min_comments: u32,
    pub max_age_hours: u64,
    pub exclude_nsfw: bool,
}

fn default_filters() -> Filters {
    Filters {
        min_score: 2,
        min_comments: 0,
        max_age_hours: 48,
        exclude_nsfw: true,
    }
}
fn default_schedule() -> u64 { 30 }
fn default_sort() -> String { "new".into() }
fn default_limit() -> u32 { 20 }

impl Default for Config {
    fn default() -> Self {
        Self {
            queries: vec![],
            filters: default_filters(),
            schedule_minutes: 30,
            notifier: "console".into(),
        }
    }
}

pub fn load_config(path: &str) -> anyhow::Result<Config> {
    let raw = std::fs::read_to_string(path)?;
    let cfg: Config = serde_yaml::from_str(&raw)?;
    Ok(cfg)
}

/// Arguments for the direct `search` subcommand, deserialized from inline YAML.
/// Reuses the same schema as `config.yaml` queries, but as a single query.
#[derive(Debug, Deserialize, Clone)]
pub struct SearchArgs {
    pub q: String,
    #[serde(default)]
    pub subreddits: Vec<String>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Optional filters. When None, permissive defaults are used so results always show.
    #[serde(default)]
    pub filters: Option<Filters>,
}

impl SearchArgs {
    /// Filters with human-friendly defaults so a bare `q:` still returns posts.
    pub fn effective_filters(&self) -> Filters {
        self.filters.clone().unwrap_or_else(|| Filters {
            min_score: 0,
            min_comments: 0,
            max_age_hours: 720,
            exclude_nsfw: false,
        })
    }
}

pub fn parse_search_args(yaml: &str) -> anyhow::Result<SearchArgs> {
    // Tolerate the human-friendly one-line form `key: v, key2: v2` by wrapping in braces
    // (YAML flow mapping). Multi-line block form and explicit `{...}` work as-is.
    let trimmed = yaml.trim();
    let input = if !trimmed.is_empty() && !trimmed.starts_with('{') && !trimmed.contains('\n') {
        format!("{{{}}}", trimmed)
    } else {
        trimmed.to_string()
    };
    let args: SearchArgs = serde_yaml::from_str(&input)?;
    if args.q.trim().is_empty() {
        anyhow::bail!("'q' (the search query) is required and cannot be empty");
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_config_yaml() {
        let yaml = r#"
queries:
  - name: "test"
    q: "rust"
    subreddits: ["rust"]
    sort: "new"
    limit: 10
filters:
  min_score: 5
  min_comments: 3
  max_age_hours: 24
  exclude_nsfw: true
schedule_minutes: 15
notifier: "console"
"#;
        let cfg: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.queries[0].name, "test");
        assert_eq!(cfg.filters.min_score, 5);
        assert_eq!(cfg.schedule_minutes, 15);
    }
    #[test]
    fn parse_search_args_minimal() {
        let args = parse_search_args("q: rust, subreddits: [rust], sort: new, limit: 10").unwrap();
        assert_eq!(args.q, "rust");
        assert_eq!(args.subreddits, vec!["rust"]);
        assert_eq!(args.sort, "new");
        assert_eq!(args.limit, 10);
        assert!(args.filters.is_none());
        let f = args.effective_filters();
        assert_eq!(f.min_score, 0);
        assert_eq!(f.max_age_hours, 720);
        assert!(!f.exclude_nsfw);
    }
    #[test]
    fn parse_search_args_defaults_and_blank_q() {
        let args = parse_search_args("q: rust").unwrap();
        assert!(args.subreddits.is_empty());
        assert_eq!(args.sort, "new");
        assert_eq!(args.limit, 20);
        assert!(parse_search_args("q: ").is_err());
    }
}
