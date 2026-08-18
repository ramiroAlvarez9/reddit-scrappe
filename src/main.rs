mod browser;
mod config;
mod cookies;
mod filter;
mod human;
mod login;
mod notifier;
mod reddit;

use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::path::Path;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name="reddit-scrappe")]
struct Args {
    #[arg(long = "loop", default_value_t = false)]
    loop_mode: bool,
    #[arg(long, default_value_t = false)]
    once: bool,
    #[arg(long)]
    login: bool,
    #[arg(long)]
    logout: bool,
    #[arg(long, default_value_t = false)]
    no_browser: bool,
    #[arg(long, default_value="config.yaml")]
    config: String,
    #[arg(long, default_value="seen.json")]
    seen: String,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
enum Format {
    Table,
    Json,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Direct declarative search. Pass a single query as inline YAML.
    /// Example:
    ///   cargo run -- search 'q: rust, subreddits: [rust], sort: new, limit: 10'
    ///   cargo run -- search 'q: rust, limit: 10' --format json
    /// Reuses the config.yaml query schema (q, subreddits, sort, limit, filters).
    /// Always shows `limit` posts, no dedup. Requires login cookies (see --login).
    Search {
        yaml: String,
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
    },
}

fn load_dotenv() {
    for path in [".env", "/Users/ramiro/reddit-scrappe/.env"] {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if std::env::var(k).is_err() {
                        std::env::set_var(k, v);
                    }
                }
            }
            break;
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_dotenv();
    let args = Args::parse();
    init_tracing();

    if args.login {
        login::login_flow().await?;
        return Ok(());
    }
    if args.logout {
        login::logout_flow()?;
        return Ok(());
    }

    if let Some(Command::Search { yaml, format }) = args.command {
        return run_search(&yaml, format).await;
    }

    let cfg_path = args.config;
    let seen_path = args.seen;

    let do_loop = args.loop_mode || (!args.once && std::env::args().any(|a| a=="--loop"));
    let no_browser = args.no_browser || std::env::var("NO_BROWSER").map(|v| v=="1" || v.to_lowercase()=="true").unwrap_or(false);
    // default once
    if do_loop {
        let cfg = config::load_config(&cfg_path)?;
        let minutes = cfg.schedule_minutes;
        if no_browser {
            tracing::info!("[loop][no-browser] cada {} min (+ jitter 2m) sin navegador — reqwest+cookies {}", minutes, cookies::cookies_file_path().display());
            if !cookies::has_valid_cookies() {
                tracing::warn!("[loop][no-browser] {}", cookies::cookies_status());
                tracing::warn!("[loop][no-browser] Ejecuta `DI_COUNTRY=es DI_SESSION=... cargo run -- --login` primero para generar cookies residenciales");
            }
            loop {
                if let Err(e) = run_once_no_browser(&cfg_path, &seen_path).await {
                    tracing::error!("[loop][no-browser] error: {:?}", e);
                }
                let jitter: i64 = rand::random::<i64>() % 120 - 60; // +-60s
                let sleep_secs = (minutes as i64 * 60 + jitter).max(30) as u64;
                tracing::info!("[loop] durmiendo {}s...", sleep_secs);
                tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)).await;
            }
        } else {
            tracing::info!("[loop] cada {} min (+ jitter 2m) Secuencial anónimo STDOUT", minutes);
            // launch browser once
            let handle = browser::launch_browser().await?;
            let browser = &handle.browser;
            loop {
                if let Err(e) = run_once(browser, &cfg_path, &seen_path).await {
                    tracing::error!("[loop] error: {:?}", e);
                }
                // save fresh cookies for future --no-browser runs
                if let Ok(cookies) = browser.get_cookies().await {
                    let v = serde_json::to_value(&cookies).unwrap_or(serde_json::Value::Null);
                    let _ = cookies::save_cookies(&v);
                }
                let jitter: i64 = rand::random::<i64>() % 120 - 60; // +-60s
                let sleep_secs = (minutes as i64 * 60 + jitter).max(30) as u64;
                tracing::info!("[loop] durmiendo {}s...", sleep_secs);
                tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)).await;
            }
        }
    } else {
        if no_browser {
            tracing::info!("[once][no-browser] reqwest+cookies {}", cookies::cookies_file_path().display());
            if !cookies::has_valid_cookies() {
                tracing::warn!("[once][no-browser] {}", cookies::cookies_status());
                tracing::warn!("[once][no-browser] Ejecuta `DI_COUNTRY=es DI_SESSION=... cargo run -- --login` primero");
            }
            run_once_no_browser(&cfg_path, &seen_path).await?;
        } else {
            // --once: need browser
            let handle = browser::launch_browser().await?;
            let res = run_once(&handle.browser, &cfg_path, &seen_path).await;
            // save fresh cookies for future --no-browser runs
            if let Ok(cookies) = handle.browser.get_cookies().await {
                let v = serde_json::to_value(&cookies).unwrap_or(serde_json::Value::Null);
                let _ = cookies::save_cookies(&v);
            }
            res?;
            // browser closes on drop
        }
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).init();
}

async fn run_once(browser: &chromiumoxide::browser::Browser, cfg_path: &str, seen_path: &str) -> anyhow::Result<()> {
    let cfg = config::load_config(cfg_path)?;
    let seen = load_seen(seen_path);
    let mut new_seen = seen.clone();
    let mut total_new = 0;

    for q in &cfg.queries {
        tracing::info!("[query:{}] buscando \"{}\" en {:?} sort={}", q.name, q.q, if q.subreddits.is_empty(){"all".to_string()} else {q.subreddits.join(",")}, q.sort);
        let page = browser.new_page("about:blank").await?;
        let posts = match reddit::search_human(&page, &q.q, &q.subreddits, &q.sort).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[query:{}] search error: {:?}", q.name, e);
                let _ = page.close().await;
                continue;
            }
        };
        let _ = page.close().await;
        tracing::info!("[query:{}] {} raw posts", q.name, posts.len());
        let filtered = filter::filter_posts(posts, &cfg.filters);
        tracing::info!("[query:{}] {} after filter (min_score={} max_age={}h)", q.name, filtered.len(), cfg.filters.min_score, cfg.filters.max_age_hours);
        let fresh: Vec<_> = filtered.into_iter().filter(|p| !seen.contains(&p.id)).collect();
        for p in &fresh { new_seen.insert(p.id.clone()); }
        total_new += fresh.len();
        notifier::notify_console(&fresh, &q.name, q.limit);
        // sequential jitter between queries
        human::sleep_jitter(2000, 4000).await;
    }
    save_seen(seen_path, &new_seen)?;
    tracing::info!("[done] total vistos: {} nuevos esta corrida: {}", new_seen.len(), total_new);
    Ok(())
}

async fn run_once_no_browser(cfg_path: &str, seen_path: &str) -> anyhow::Result<()> {
    let cfg = config::load_config(cfg_path)?;
    let seen = load_seen(seen_path);
    let mut new_seen = seen.clone();
    let mut total_new = 0;

    for q in &cfg.queries {
        tracing::info!("[query:{}][no-browser] buscando \"{}\" en {:?} sort={}", q.name, q.q, if q.subreddits.is_empty(){"all".to_string()} else {q.subreddits.join(",")}, q.sort);
        let posts = match reddit::search_no_browser(&q.q, &q.subreddits, &q.sort, q.limit).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[query:{}][no-browser] search error: {:?}", q.name, e);
                continue;
            }
        };
        tracing::info!("[query:{}][no-browser] {} raw posts", q.name, posts.len());
        let filtered = filter::filter_posts(posts, &cfg.filters);
        tracing::info!("[query:{}][no-browser] {} after filter (min_score={} max_age={}h)", q.name, filtered.len(), cfg.filters.min_score, cfg.filters.max_age_hours);
        let fresh: Vec<_> = filtered.into_iter().filter(|p| !seen.contains(&p.id)).collect();
        for p in &fresh { new_seen.insert(p.id.clone()); }
        total_new += fresh.len();
        notifier::notify_console(&fresh, &q.name, q.limit);
        // lighter jitter since no browser (still shuffle human)
        human::sleep_jitter(800, 1500).await;
    }
    save_seen(seen_path, &new_seen)?;
    tracing::info!("[done][no-browser] total vistos: {} nuevos esta corrida: {}", new_seen.len(), total_new);
    if total_new == 0 && !cookies::has_valid_cookies() {
        tracing::warn!("[no-browser] 0 nuevos y cookies inválidas — {}", cookies::cookies_status());
        tracing::warn!("[no-browser] Si ves 0 posts consistentes, ejecuta `cargo run -- --login` con residencial y reintenta --no-browser");
    }
    Ok(())
}

async fn run_search(yaml: &str, format: Format) -> anyhow::Result<()> {
    let args = config::parse_search_args(yaml)?;
    let filters = args.effective_filters();
    tracing::info!("[search] q=\"{}\" subreddits={:?} sort={} limit={} format={:?}", args.q, args.subreddits, args.sort, args.limit, format);
    tracing::info!("[search] filters: min_score={} max_age={}h exclude_nsfw={}", filters.min_score, filters.max_age_hours, filters.exclude_nsfw);

    if !cookies::has_valid_cookies() {
        let msg = format!(
            "Error: no valid login cookies found at {}.\n\nYou must log in once first. Run:\n\n    DI_COUNTRY=<iso2> cargo run -- --login\n\nThen log in in the Chrome window and press ENTER. This binds your session to a residential IP.\n\nCurrent cookie status: {}",
            cookies::cookies_file_path().display(),
            cookies::cookies_status()
        );
        eprintln!("{}", msg);
        std::process::exit(1);
    }

    let posts = reddit::search_no_browser(&args.q, &args.subreddits, &args.sort, args.limit).await?;
    tracing::info!("[search] {} raw posts", posts.len());
    let filtered = filter::filter_posts(posts, &filters);
    tracing::info!("[search] {} after filter", filtered.len());
    let fmt = match format {
        Format::Table => notifier::Format::Table,
        Format::Json => notifier::Format::Json,
    };
    notifier::notify(&filtered, &args.q, args.limit, fmt);
    Ok(())
}

fn load_seen(path: &str) -> HashSet<String> {
    if Path::new(path).exists() {
        if let Ok(raw) = std::fs::read_to_string(path) {
            if let Ok(v) = serde_json::from_str::<Vec<String>>(&raw) {
                return v.into_iter().collect();
            }
        }
    }
    HashSet::new()
}
fn save_seen(path: &str, seen: &HashSet<String>) -> anyhow::Result<()> {
    let v: Vec<_> = seen.iter().cloned().collect();
    std::fs::write(path, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}
