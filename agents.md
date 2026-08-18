# AGENTS - reddit-scrappe

> Rust scraper that simulates an anonymous human with persistent login to find posts where to talk about OrchardRun. Local STDOUT only for now, irregular human anti-blocking.
> For more technical context (implementation + why each decision) see `/docs/IMPLEMENTATION.md`, `/docs/DECISIONS.md`, `/docs/RESULTS.md`.
> **Command reference:** [`docs/COMMANDS.md`](docs/COMMANDS.md) — all commands for humans (`table`) and agents (`--format json` / `--no-browser`), with every parameter explained (`q`, `subreddits`, `sort`, `limit`, `filters: {min_score, min_comments, max_age_hours, exclude_nsfw}`, `--config`, `--seen`, `DI_*` env).

## Stack

- **Rust 1.97** `tokio 1` + `chromiumoxide 0.9` headless `new` + `scraper 0.27` + `reqwest 0.13 rustls` fallback
- `serde_yaml 0.9` `clap 4` `tracing 0.1` `rand 0.8` `dirs 5` `anyhow 1`
- Chrome 151 via `brew install --cask google-chrome` (Mac) / `apt install chromium` (VPS)

## Structure

```
src/
  main.rs      # CLI --login/--logout/--once/--loop --no-browser --config --seen, search <inline-yaml> subcommand, loop 25-45m irregular shuffle (planned)
  browser.rs   # launch chromiumoxide hide()+new_headless+UA Chrome/151 + user_data_dir ~/.cache/reddit-scrappe/profile
  login.rs     # login_flow() headed + ENTER, logout_flow() rm profile, profile_dir()
  reddit.rs    # search_human() goto search?sort=new&t=week + human_scroll + parse shreddit-post + is_captcha + fallback old.reddit + search_no_browser()
  cookies.rs   # save/load decrypted cookies.json via CDP (for --no-browser), expiry filtering  human.rs     # sleep_jitter 800-2000ms, human_scroll 2-3 steps
  filter.rs    # filter_posts score/comments/age/nsfw/dedup
  config.rs    # serde_yaml config.yaml queries/filters/schedule_minutes
  notifier.rs  # console STDOUT visual ANSI table (default) / JSONL (--format json)
tests/
  fixtures/reddit_search_sample.html  # golden shreddit-post + old.reddit thing
  e2e_browser.rs  # #[ignore] smoke example.com
  e2e_reddit.rs   # #[ignore] REDIDT_E2E=1 real reddit (fixture-based)
config.yaml    # 3 thematic queries, filters min_score 2 max_age 48h, schedule 30
```

## Agents / Roles

- **browser agent**: launches 1 Browser reuse, 1 Page sequential per query, cleans SingletonLock (`/tmp/chromiumoxide-runner` + `~/.cache/reddit-scrappe/profile`), stealth `hide()` + `--disable-blink-features=AutomationControlled` + `new_headless_mode` + real UA
- **human agent**: delays `800-4000ms` jitter, `human_scroll` wheel 500-700 x2-3, warm-up `goto reddit.com/` before search (planned)
- **reddit agent**: poll `shreddit-post` 15s (1s interval) + fallback `old.reddit` via `reqwest` on timeout/captcha, detects `Prove your humanity|blocked by network security|cf-challenge|hcaptcha|recaptcha` -> light fallback without hammering
- **filter agent**: `filter_posts` + `seen.json` dedup
- **notifier agent**: `notify` visual ANSI table with `┌─┬─┐` + cyan subreddit + score colors (default) or JSONL `--format json`, `take(limit)`

## Commands

> Full reference: [`docs/COMMANDS.md`](docs/COMMANDS.md) — global flags, `search` params, `config.yaml`, env vars, human vs agent examples.

```bash
# setup
brew install --cask google-chrome
cargo build

# login terminal spawns headed (once)
cargo run -- --login
# -> log in via window, ENTER -> saves ~/.cache/reddit-scrappe/profile (11 reddit cookies)

# run
cargo run -- --once                        # headless new with persistent profile
cargo run -- --once --no-browser           # no Chromium: reqwest + proxy + cookies.json (28MB vs 630MB, 60KB vs 300KB/query)
cargo run -- search 'q: rust, subreddits: [rust], limit: 10'  # direct declarative search (YAML inline), shows exactly `limit` posts, no dedup
cargo run -- search 'q: rust, limit: 10' --format json        # JSONL for agents (1 JSON per line)
cargo run -- search '{q: rust, filters: {min_score: 3, max_age_hours: 24}}'  # full YAML query schema
cargo run -- --loop                        # every 30m + jitter ±60s (planned irregular 25-45m)
cargo run -- --loop --no-browser           # loop without browser (light VPS)
RUST_LOG=debug cargo run -- --once
HEADLESS=0 cargo run -- --once             # headed debug
NO_BROWSER=1 cargo run -- --once           # env alternative to --no-browser
cargo run -- --logout                      # rm profile + revoke: reddit.com/settings/account

# test
cargo test -- --nocapture                  # 14 unit passed (filter/config/parse/captcha/human/login/cookies/notifier)
cargo test -- --ignored --nocapture        # e2e (requires Chrome)
REDIDT_E2E=1 cargo test e2e_reddit -- --ignored --nocapture

# util
ls ~/Library/Caches/reddit-scrappe/profile/Default/Cookies  # 11 reddit cookies after login
cat /tmp/reddit_debug.html | grep -o "<title>.*</title>"     # debug blocked page
```

## Env

None required locally. Optional: `RUST_LOG=debug` `HEADLESS=0` `CAPTCHA_WAIT_SECS=30` `CHROME_PATH`

## Anti-blocking (research 2026-08-15)

- Reddit WAF 4 layers: L1 ASN datacenter, L2 TLS JA3, L3 behavioral (fixed intervals, bursts), L4 OAuth 100/min. `old.reddit/.json` anon `403` today.
- `blocked by network security` = CDN/IP reputation, auto 1-24h. Datacenter IP + `requests` JA3 blocks in ms even at 30m rate.
- Irregular human plan: `25-45m random` + `shuffle queries` + `sleep 5-15s` between queries + night pause `0-7am` + warm-up `goto /` + `stealth JS` `navigator.webdriver=undefined` (future) + `hide()+new_headless+UA` already + persistent login profile. `sort=new t=week` + `max_age 48h` optimal for OrchardRun (index delay minutes-hours, Hot window 1-3h).

## Notes

- Python legacy `scraper.py`/`main.py` remains but Rust is primary (bin `reddit-scrappe`)
- Do not commit `profile/`, `cookies.json`, `seen.json` (`.gitignore`)
- VPS headless: `scp -r ~/Library/Caches/reddit-scrappe/profile user@vps:~/.cache/reddit-scrappe/profile` after local login
- Revoke: `cargo run -- --logout` + `reddit.com/settings/account` change password

## Web Research

> Residential proxy DataImpulse – WAF bypass and geo-localized content. **HARD RULE:** never set `HTTP_PROXY/HTTPS_PROXY` globally (burns plan data, adds latency, leaks API traffic through third-party residential IP). Proxy only inside `reqwest::Proxy` in `~/mcp/dataimpulse-mcp` (`src/main.rs:7`).

- **MCP:** `di-proxy` in `~/.config/opencode/opencode.jsonc` (user scope, bin `~/mcp/dataimpulse-mcp/target/debug/dataimpulse-mcp`, env `DI_USER/DI_PASS`). `opencode mcp list` must show `✓ connected`. Creds stay in `opencode.jsonc`, never hardcoded.
- **To read any public page, use `fetch_page(url, country?, city?, session?, raw?)` instead of `WebFetch`.**
- **Exception:** if it's public docs that don't block, `WebFetch` is faster and doesn't burn data. Proxy is for what blocks or varies by region.
- **If content depends on country (prices, stock, availability, searches), always pass explicit `country` (ISO2 `ar, es, us, de`). Never assume default country.**
- **If there is more than one request to the same site (pagination, login, 2-step flow), use the same `session` for all. One IP per task, not one IP per request (sticky 30min via `sessid`).**
- **On 403 do not retry the same: change country or pin `session`.**
- **On 503 `NO_RAY`, drop `city` targeting and keep only `country` (city costs double).**
- **DataImpulse errors:** `407 TRAFFIC_EXHAUSTED` out of data, `407 THREADS_EXHAUSTED` >2000 conns, `503 NO_RAY` no IPs for that targeting.
- **DataImpulse syntax (`gw.dataimpulse.com:823` rotating, `:824` SOCKS):** `login__cr.es;city.madrid;sessid.myTag` (`__` opens params, `;` separates, `.` key/value, `,` multi-value). `city` requires `cr`.

Example:
```bash
# verify exit IPs before scraping
# via MCP: check_exit_ip(country="ar"), check_exit_ip(country="es")
# via CLI: DI_USER=... DI_PASS=... printf '...tools/call check_exit_ip...' | ./target/debug/dataimpulse-mcp
```

## Status

- Headed login OK (14 cookies 3274 bytes after re-login 2026-08-17, `page.get_cookies()` 14 parts), `cargo build 1 warning dead_code`, `cargo test 9 passed` (`cookies` new).
- Plan B+ (browser+proxy+JSON) implemented: `browser.rs:36` `proxy_server_arg()` reads `DI_USER/DI_PASS/DI_COUNTRY/DI_CITY/DI_SESSION/DI_USE_SOCKS` -> `--proxy-server=http(s)/socks5://user:pass@gw.dataimpulse.com:823/824` + `city` (cost ok), `login.rs:20` headed login via proxy, `reddit.rs:50` `apply_stealth()` via `evaluate_on_new_document` headless puro (`STEALTH_JS` `webdriver/chrome/plugins`), warm-up `goto reddit.com/` + `human_scroll`, `reddit.rs:271` `search_old_reddit_json` + `search_fallback_old_reddit_with_cookies` via `reqwest::Proxy` + login cookies (3274 bytes) + `search_old_reddit_via_browser`. `main.rs:12` auto-loads `.env`, `Cargo.toml:21` adds `rusqlite` + `reqwest socks`.
- E2E headless puro `DI_COUNTRY=es DI_CITY=madrid DI_SESSION=json-test-1 cargo run -- --once --config /tmp/test_config.yaml` still `timeout shreddit-post 15s 189k theme-beta` `is_captcha` -> but `fallback json GET https://old.reddit.com/r/rust/search.json?q=rust...` via proxy `country=es city=madrid` -> `parsed 25 posts from json` -> `25 raw posts 25 after filter` `notify` 5 posts OK. Same for `DI_COUNTRY=us` `25 posts`. `old.reddit` scrape as bot now works via **JSON** (HTML `div.thing 0` still blocked, but JSON `children` bypasses). `dataimpulse-mcp` `check_exit_ip` `us 82.40.113.131` `ar 181.209.92.163` ok, `fetch_page example.com` 127 chars.
- Complete reddit bot: `DI_COUNTRY=... DI_SESSION=... cargo run -- --login` (bind to residential) once, then `cargo run -- --once` / `--loop` headless uses `old.reddit/search.json` + proxy + cookies, no need for `shreddit-post` JS. HTML fallback kept for non-JSON.
- No-browser mode (branch `feat/no-browser-mode`): `cookies.rs` saves `page.get_cookies()` to `~/Library/Caches/reddit-scrappe/profile/cookies.json` (`7774 bytes` 10 reddit parts `2907 bytes`) via `save_cookies()` on `--login` + after each `--once` browser run; `--no-browser` (or `NO_BROWSER=1`) skips `launch_browser` and uses `reddit::search_no_browser()` via `reqwest::Proxy` + `cookies::load_cookie_header()` (expiry filter). E2E `DI_COUNTRY=es cargo run -- --no-browser --once` `25 posts` in `~2s` vs `~25s` with browser (`60KB` vs `300KB/query`, `28MB` vs `630MB RSS`). `cargo test 9 passed`. Anonymous without cookies still `403 theme-beta` `0 posts` as expected.
- Direct `search` subcommand: `cargo run -- search 'q: rust, subreddits: [rust], limit: 10'` parses inline YAML (`config::parse_search_args`, tolerant of `key: v, key2: v2` and block/`{}`), uses permissive default filters (`min_score 0`, `max_age 720h`) so results always show, threads `limit` through URL + notifier (`take(limit)`), no dedup. No cookies -> clear English error + exit 1. `cargo test 11 passed`.
- Search output formats: default `table` → markdown table `| # | subreddit | score | ... |` for humans; `--format json` → JSONL (1 JSON per line) for agents via `notifier::Format`. `cargo test 14 passed`.
- MCP DataImpulse OK (`~/mcp/dataimpulse-mcp` build ok, `di-proxy` `✓ connected`).
