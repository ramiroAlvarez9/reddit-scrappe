# reddit-scrappe

> Rust bot that finds Reddit posts where you can talk about **OrchardRun** — headless Chrome + residential proxy + human behavior. Local STDOUT for now.

It logs in once as a real user, keeps the session, and scans Reddit every ~30 minutes looking for fresh, relevant threads (e.g. `AI infrastructure`, `seed round`, `orchard`) where a genuine comment about OrchardRun makes sense. No spam, no auto-posting — it just surfaces opportunities.

## Features

- **Headless Chrome** (`chromiumoxide 0.9` `new` mode) with stealth (`hide()`, `--disable-blink-features=AutomationControlled`, `evaluate_on_new_document` hiding `navigator.webdriver`/`chrome`/`plugins`), real UA `Chrome/151`
- **Persistent login** — `cargo run -- --login` opens a headed window, you log in, `ENTER` saves `~/.cache/reddit-scrappe/profile` (14 cookies, `reddit_session` etc., ~3 KB) for all future headless runs
- **Residential proxy** (DataImpulse `gw.dataimpulse.com:823` HTTP / `824` SOCKS) per-request via `reqwest::Proxy` and per-browser via `--proxy-server` — never via global `HTTP_PROXY`
- **Human behavior** — `800-4000ms` jitter, `human_scroll` 2-3 wheel steps `500-700px`, warm-up `goto reddit.com/` before each search, shuffle + `25-45m` loop
- **Reddit search with fallbacks** — `www.reddit.com/search?sort=new&t=week` poll `shreddit-post` 15s, `is_captcha` (`Prove your humanity|blocked by network security|cf-challenge|hcaptcha|recaptcha`), then `old.reddit/search.json?q=...&sort=new&limit=25` via proxy + login cookies (`page.get_cookies()`), then HTML `div.thing` and browser `old.reddit` via `Page`
- **Filters** — `min_score`, `min_comments`, `max_age_hours`, `exclude_nsfw`, `seen.json` dedup
- **Notifier** — console `max 5/query` (Telegram disabled)
- **MCP for geo research** — `~/mcp/dataimpulse-mcp` exposes `fetch_page` and `check_exit_ip` via `opencode`

## Stack

- **Rust 1.97** · `tokio 1` `chromiumoxide 0.9` `scraper 0.27` `reqwest 0.13 rustls+socks` `rusqlite 0.32 bundled` `serde_yaml 0.9` `clap 4` `tracing 0.1` `rand 0.8` `dirs 5`
- Chrome 151 — Mac `brew install --cask google-chrome` · VPS `apt install chromium`

## Project Structure

```
src/
  main.rs      # CLI --login/--logout/--once/--loop --config --seen, loop 25-45m + jitter, auto .env
  browser.rs   # launch headless/headed, proxy per DI_* env, singleton lock cleanup, UA
  login.rs     # login_flow() headed + ENTER, logout_flow() rm profile
  reddit.rs    # search_human() + stealth JS, fallback JSON/HTML/browser, is_captcha, parse
  human.rs     # sleep_jitter, human_scroll 500-700 x2-3
  filter.rs    # filter_posts
  config.rs    # serde_yaml config.yaml
  notifier.rs  # notify_console
tests/
  fixtures/reddit_search_sample.html
  e2e_browser.rs  # #[ignore] smoke example.com
  e2e_reddit.rs   # #[ignore] REDIDT_E2E=1 real reddit
config.yaml    # 3 thematic queries, filters, schedule 30
```

## Installation

```bash
# Chrome (Mac)
brew install --cask google-chrome
# Chrome (Ubuntu VPS)
sudo apt update && sudo apt install -y chromium-browser

# Build
cargo build

# Verify
cargo test -- --nocapture  # 8 unit passed
opencode mcp list          # after proxy setup, shows di-proxy ✓ connected
```

## Quick Start

```bash
# 1. Create .env with residential proxy creds (never commit, already .gitignored)
cat > .env <<'ENV'
DI_USER=your_dataimpulse_login
DI_PASS=your_dataimpulse_password
# optional geo/sticky per run
DI_COUNTRY=es
DI_CITY=madrid   # city requires country, costs double
DI_SESSION=reddit-1
# DI_USE_SOCKS=1 # set to 1 for socks5://:824 else http://:823
ENV

# 2. Login once via residential IP (binds cookies to proxy)
DI_COUNTRY=es DI_CITY=madrid DI_SESSION=reddit-1 cargo run -- --login
# -> headed Chrome at https://www.reddit.com/login, log in manually, ENTER

# 3. Run once (headless, proxy, JSON fallback)
DI_COUNTRY=es DI_SESSION=reddit-1 RUST_LOG=info cargo run -- --once

# 4. Loop
DI_COUNTRY=us cargo run -- --loop
```

Credentials are read from `std::env::var("DI_USER")` / `DI_PASS` and fail fast if missing. They are stored only in `~/.config/opencode/opencode.jsonc` (MCP user scope) or your `.env`, never hardcoded.

## Configuration

`config.yaml`:

```yaml
queries:
  - name: "orchard ai infra"
    q: "AI infrastructure OR MLOps OR vector database"
    subreddits: ["MachineLearning", "artificial", "LocalLLaMA", "datascience"]
    sort: "new" # new | relevance | top
    limit: 20
  - name: "startup fundraising"
    q: "seed round OR fundraising OR VC"
    subreddits: ["startups", "Entrepreneur", "vc"]
    sort: "new"
    limit: 20
  - name: "orchardrun directo"
    q: "orchard"
    subreddits: [] # empty = whole reddit
    sort: "new"
    limit: 10

filters:
  min_score: 2
  min_comments: 0
  max_age_hours: 48
  exclude_nsfw: true

schedule_minutes: 30
notifier: "console"
```

`--config` and `--seen` are CLI flags (`--seen seen.json` dedup store).

## Usage

```bash
cargo run -- --login                    # headed login
cargo run -- --logout                   # rm profile + revocation hint
cargo run -- --once                     # single scan
cargo run -- --loop                     # loop every 30m + jitter ±60s (irregular 25-45m planned)
RUST_LOG=debug cargo run -- --once      # verbose
HEADLESS=0 cargo run -- --once          # headed debug
cargo test -- --ignored --nocapture     # e2e (needs Chrome)
REDIDT_E2E=1 cargo test e2e_reddit -- --ignored

# Debug
ls ~/Library/Caches/reddit-scrappe/profile/Default/Cookies
cat /tmp/reddit_debug.html | grep -o "<title>.*</title>"
DI_COUNTRY=es DI_SESSION=test cargo run -- --once --config /tmp/test.yaml --seen /tmp/fresh.json
```

VPS headless after local login:

```bash
scp -r ~/Library/Caches/reddit-scrappe/profile user@vps:~/.cache/reddit-scrappe/profile
DI_COUNTRY=us cargo run -- --once
```

## How It Finds Posts

1. `Browser::launch` with persistent `user_data_dir`, `new_headless_mode` or `with_head()` + `hide()`, `proxy_server_arg()` from `DI_*`
2. For each `query` in `config.yaml`: `browser.new_page("about:blank")` -> `apply_stealth()` (`evaluate_on_new_document` hiding `webdriver`/`chrome`/`plugins`) -> warm-up `goto https://www.reddit.com/` + `human_scroll` -> `goto https://www.reddit.com/search/?q=...&sort=new&t=week` (or `/r/{sub}/search`)
3. Poll `shreddit-post` 15s (1s interval) + `div[data-testid='post-container']` / `article`
4. If `is_captcha` (`blocked by network security` etc.) or timeout: `search_old_reddit_json` (`/search.json?q=...&sort=new&limit=25` via `reqwest::Proxy` + `Cookie` from `page.get_cookies()` 3 KB) -> `parse_reddit_json` (`data.children`) -> if empty, `search_fallback_old_reddit_with_cookies` HTML `div.thing` -> if still empty, `search_old_reddit_via_browser` (`Page.goto old.reddit`)
5. `filter_posts` (`score/comments/age/nsfw`) + `seen.json` dedup -> `notify_console` max 5/query STDOUT
6. `sleep_jitter 2-4s` between queries

`sort=new t=week` + `max_age 48h` is optimal per research: Reddit index delay minutes-hours, Hot window 1-3h.

## Anti-Blocking

Reddit WAF (research 2026-08-15): **L1** ASN datacenter, **L2** TLS JA3, **L3** behavioral (fixed intervals, bursts), **L4** OAuth 100/min. `old.reddit/.json` anon `403` today. `blocked by network security` = CDN/IP auto 1-24h. Datacenter + `requests` JA3 blocks in ms even at 30m rate.

Current mitigations:

- Residential proxy bypasses L1, `hide()+new_headless+UA Chrome/151` + `STEALTH_JS` helps L2, `800-4000ms` jitter + `human_scroll` `500-700 x2-3` + warm-up `goto /` + `25-45m` irregular + shuffle + night pause `0-7am` helps L3, persistent login profile helps L4. **E2E with proxy `DI_COUNTRY=es DI_CITY=madrid` headless still `timeout shreddit-post` `theme-beta 189k` but `fallback json` via proxy `parsed 25 posts` succeeds** — HTML `shreddit` remains JS-heavy, but JSON `children` bypasses it.

If `429`/`403` on `old.reddit` with proxy, change country/session; `407 TRAFFIC_EXHAUSTED` = out of data, `407 THREADS_EXHAUSTED` >2000 conns, `503 NO_RAY` = no IPs for that city (drop city). City targeting costs double — use country-only unless needed.

## Residential Proxy (DataImpulse)

**Hard rule:** never set `HTTP_PROXY/HTTPS_PROXY` globally — it burns plan data, adds latency, and leaks API traffic via residential IP. Proxy only inside `reqwest::Proxy` (`~/mcp/dataimpulse-mcp`) and `browser.rs:36` `--proxy-server`.

Host `gw.dataimpulse.com` · HTTP `823` · SOCKS5 `824` · sticky `10000-20000` · Auth `login:password@host:port` · Syntax `login__cr.es;city.madrid;sessid.myTag` (`__` opens params, `;` separates, `.` key/value, `,` multi) · `cr` ISO2, `city` requires `cr`, `sessid` sticky 30min · Limit 2000 threads

Env:

```
DI_USER, DI_PASS (required)
DI_COUNTRY=es    # ISO2
DI_CITY=madrid   # requires DI_COUNTRY
DI_SESSION=myTag # sticky
DI_USE_SOCKS=1   # socks5://:824 else http://:823
```

MCP server `~/mcp/dataimpulse-mcp` (Rust `tokio` + `reqwest` + `scraper`) is registered as `di-proxy` user scope `~/.config/opencode/opencode.jsonc`:

```bash
cargo build # in ~/mcp/dataimpulse-mcp
opencode mcp add di-proxy --env DI_USER=... --env DI_PASS=... -- ~/mcp/dataimpulse-mcp/target/debug/dataimpulse-mcp
opencode mcp list # must show ✓ connected
```

MCP tools:

- `fetch_page(url, country?, city?, session?, raw?)` — browser headers, 45s timeout, 60k chars, `raw:true` = HTML, default = clean text without scripts/styles. Returns `isError` with actionable hint for `403/407/503`.
- `check_exit_ip(country?, session?)` — hits `https://api.ipify.org?format=json` via same proxy to verify geo.

AGENTS rules (`AGENTS.md:83`): use `fetch_page` for any public page that blocks or varies by region, not `WebFetch`; for `prices/stock/search` always pass `country`; same `session` per task; on `403` change country/session, on `503 NO_RAY` drop city.

## Environment Variables

| Var | Required | Description |
|-----|----------|-------------|
| `DI_USER` / `DI_PASS` | proxy only | DataImpulse login |
| `DI_COUNTRY` | no | ISO2 `ar,es,us,de` |
| `DI_CITY` | no | requires `DI_COUNTRY` |
| `DI_SESSION` | no | sticky label |
| `DI_USE_SOCKS` | no | `1` = `socks5://:824` |
| `RUST_LOG` | no | `debug` `info` |
| `HEADLESS` | no | `0` = headed |
| `CAPTCHA_WAIT_SECS` | no | default 30 |
| `CHROME_PATH` | no | override Chrome binary |

`.env` auto-loaded by `main.rs:12` from `./.env` or `/Users/ramiro/reddit-scrappe/.env`.

## Development

```bash
cargo fmt
cargo clippy
cargo test -- --nocapture
# e2e needs Chrome and optionally proxy
DI_COUNTRY=es DI_CITY=madrid cargo run -- --once --config /tmp/test.yaml --seen /tmp/fresh.json
```

## Troubleshooting

- `timeout shreddit-post 15s` + `theme-beta 189k` -> `is_captcha` true, check `/tmp/reddit_debug.html`. Expected with WAF; JSON fallback should still yield `parsed 25 posts`.
- `old.reddit 403` anonymously even with `t=week` -> use `DI_USER` + login cookies; `check_exit_ip` should show different IPs for `ar/es/us`.
- `407 TRAFFIC_EXHAUSTED` -> refill DataImpulse. `THREADS_EXHAUSTED` -> reduce concurrency. `503 NO_RAY` -> drop `DI_CITY`.
- `Cookies` `length(value)==0` in sqlite is normal (Chrome encrypts `value` -> `encrypted_value`); bot uses `page.get_cookies()` via CDP (3 KB) — don't read sqlite directly.
- Singleton lock errors -> `browser.rs` cleans `/tmp/chromiumoxide-runner` and `profile/Singleton*`.

## Notes

- Python `scraper.py`/`main.py` legacy remains, Rust is primary (`bin reddit-scrappe`)
- Never commit `profile/`, `cookies.json`, `seen.json`, `.env`
- Revoke after logout: `reddit.com/settings/account` change password or `reddit.com/prefs/apps`

## License

MIT — see `LICENSE` if present. Not affiliated with Reddit, Inc.

## Status (2026-08-17)

- Login headed OK (14 cookies, 3274 bytes, `page.get_cookies()`), `cargo build 1 warning`, `cargo test 8 passed`
- `Plan B+` browser+proxy+JSON live: `e2e DI_COUNTRY=es DI_CITY=madrid` headless still `theme-beta` for `shreddit` but `fallback json` via `old.reddit/search.json` with proxy+cookies returns `25 posts` (HTML still `0` for `div.thing`). `DI_COUNTRY=us` also `25`. Full bot now uses `search.json` + proxy + cookies, no JS needed.
- `dataimpulse-mcp` `check_exit_ip` `us 82.40.113.131` `ar 181.209.92.163` `es 188.78.184.162` `fetch_page example.com` 127 chars OK
