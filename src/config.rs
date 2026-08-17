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
}
