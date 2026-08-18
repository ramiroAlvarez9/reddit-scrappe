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

    #[test]
    fn parse_search_args_braced() {
        let args = parse_search_args("{q: rust, limit: 5}").unwrap();
        assert_eq!(args.q, "rust");
        assert_eq!(args.limit, 5);
        assert_eq!(args.sort, "new");
        // braced form must not double-wrap
        let args2 = parse_search_args("{q: rust, subreddits: [rust], sort: relevance, limit: 7}").unwrap();
        assert_eq!(args2.subreddits, vec!["rust"]);
        assert_eq!(args2.sort, "relevance");
    }

    #[test]
    fn parse_search_args_multiline_block() {
        let yaml = "q: rust\nsubreddits: [rust, programming]\nlimit: 10\nsort: new";
        let args = parse_search_args(yaml).unwrap();
        assert_eq!(args.q, "rust");
        assert_eq!(args.subreddits, vec!["rust", "programming"]);
        assert_eq!(args.limit, 10);
        assert_eq!(args.sort, "new");
    }

    #[test]
    fn parse_search_args_with_filters_inline() {
        let args = parse_search_args(
            "q: rust, filters: {min_score: 5, min_comments: 2, max_age_hours: 24, exclude_nsfw: true}",
        )
        .unwrap();
        assert!(args.filters.is_some());
        let f = args.effective_filters();
        assert_eq!(f.min_score, 5);
        assert_eq!(f.min_comments, 2);
        assert_eq!(f.max_age_hours, 24);
        assert!(f.exclude_nsfw);
        // without filters -> permissive
        let permissive = parse_search_args("q: rust").unwrap().effective_filters();
        assert_eq!(permissive.min_score, 0);
        assert_eq!(permissive.max_age_hours, 720);
        assert!(!permissive.exclude_nsfw);
    }

    #[test]
    fn parse_search_args_rejects_empty_yamls() {
        assert!(parse_search_args("").is_err());
        assert!(parse_search_args("   ").is_err());
        assert!(parse_search_args("{q: \"\"}").is_err());
        assert!(parse_search_args("q: \"   \"").is_err());
    }

    #[test]
    fn parse_search_args_rejects_invalid_yaml_type() {
        assert!(parse_search_args("q: rust, limit: not_a_number").is_err());
        assert!(parse_search_args("q: rust, subreddits: not_a_list").is_err());
        // missing q
        assert!(parse_search_args("limit: 10").is_err());
        assert!(parse_search_args("subreddits: [rust]").is_err());
    }

    #[test]
    fn parse_search_args_allows_q_with_comma_and_spaces() {
        let args = parse_search_args("q: \"rust lang, async\", limit: 3").unwrap();
        assert_eq!(args.q, "rust lang, async");
        assert_eq!(args.limit, 3);
        // q with OR and spaces, typical orchard query
        let args2 = parse_search_args("q: \"AI infrastructure OR MLOps\", limit: 5").unwrap();
        assert_eq!(args2.q, "AI infrastructure OR MLOps");
    }

    #[test]
    fn effective_filters_permissive_vs_strict() {
        let permissive = SearchArgs {
            q: "rust".into(),
            subreddits: vec![],
            sort: "new".into(),
            limit: 10,
            filters: None,
        }
        .effective_filters();
        assert_eq!(permissive.min_score, 0);
        assert_eq!(permissive.min_comments, 0);
        assert_eq!(permissive.max_age_hours, 720);
        assert!(!permissive.exclude_nsfw);

        let strict = SearchArgs {
            q: "rust".into(),
            subreddits: vec![],
            sort: "new".into(),
            limit: 10,
            filters: Some(Filters {
                min_score: 10,
                min_comments: 3,
                max_age_hours: 12,
                exclude_nsfw: true,
            }),
        }
        .effective_filters();
        assert_eq!(strict.min_score, 10);
        assert_eq!(strict.min_comments, 3);
        assert_eq!(strict.max_age_hours, 12);
        assert!(strict.exclude_nsfw);
    }

    #[test]
    fn parse_search_args_filters_partial_fails() {
        // Filters requires all 4 fields; partial must error
        assert!(parse_search_args("q: rust, filters: {min_score: 5}").is_err());
        assert!(parse_search_args("q: rust, filters: {min_score: 5, max_age_hours: 24}").is_err());
        // full filters ok
        let full = parse_search_args(
            "q: rust, filters: {min_score: 0, min_comments: 0, max_age_hours: 720, exclude_nsfw: false}",
        );
        assert!(full.is_ok());
    }

    #[test]
    fn parse_search_args_subreddits_type_and_limit_edge() {
        // subreddits as plain string -> error (expects array)
        assert!(parse_search_args("q: rust, subreddits: rust").is_err());
        // limit 0 is technically allowed (u32) -> effective notify will clamp to 1 later
        let args = parse_search_args("q: rust, limit: 0").unwrap();
        assert_eq!(args.limit, 0);
        // limit negative -> parse error for u32
        assert!(parse_search_args("q: rust, limit: -1").is_err());
        // extra unknown field is ignored by serde (should not error)
        let args = parse_search_args("q: rust, limit: 5, unknown_field: 123").unwrap();
        assert_eq!(args.limit, 5);
    }

    #[test]
    fn parse_search_args_wrapping_edge_cases() {
        // braces + newline should NOT double-wrap (already starts with '{')
        let yaml = "{q: rust,\nlimit: 5}";
        let args = parse_search_args(yaml).unwrap();
        assert_eq!(args.q, "rust");
        assert_eq!(args.limit, 5);
        // leading/trailing spaces trimmed
        let args = parse_search_args("   q: rust, limit: 3   ").unwrap();
        assert_eq!(args.q, "rust");
        // single-line with explicit braces and spaces
        let args = parse_search_args(" { q: rust, limit: 2 } ").unwrap();
        assert_eq!(args.limit, 2);
    }

    #[test]
    fn load_config_from_tempfile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_config.yaml");
        let yaml = r#"
queries:
  - name: "a"
    q: "hello"
    subreddits: [rust]
    sort: new
    limit: 5
filters:
  min_score: 3
  min_comments: 1
  max_age_hours: 12
  exclude_nsfw: false
schedule_minutes: 15
"#;
        std::fs::write(&path, yaml).unwrap();
        let cfg = load_config(path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.queries.len(), 1);
        assert_eq!(cfg.queries[0].q, "hello");
        assert_eq!(cfg.filters.min_score, 3);
        assert_eq!(cfg.schedule_minutes, 15);
        // missing file -> error
        assert!(load_config("/nonexistent/path.yaml").is_err());
        // invalid yaml -> error
        let bad_path = dir.path().join("bad.yaml");
        std::fs::write(&bad_path, "::: not yaml :::").unwrap();
        assert!(load_config(bad_path.to_str().unwrap()).is_err());
    }
}
