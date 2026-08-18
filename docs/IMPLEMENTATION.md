# Technical Implementation — reddit-scrappe + DataImpulse

> Why every piece was built the way it is, with tradeoffs and rejected alternatives.

## 0. Goal

Find Reddit posts where a genuine comment about **OrchardRun** makes sense, without getting WAF-banned. Local STDOUT only. Must work headless on Mac and VPS, survive `blocked by network security`, and allow geo-targeted research (prices, search results) as a residential user.

## 1. DataImpulse Residential Proxy MCP (`~/mcp/dataimpulse-mcp`)

### 1.1 Why a separate MCP server in Rust

- **Need:** Read any public page that blocks bots and see geo-localized content as a local user.
- **Choice:** Rust `cargo new dataimpulse-mcp` (`edition 2021`) with `tokio 1 + reqwest 0.12 rustls + scraper 0.24 + serde_json`.
  - *Why not Python:* Rust gives single static binary, no `requests` JA3 fingerprint, easy `reqwest::Proxy` per-request, and matches main crate `reddit-scrappe` (Rust 1.97).
  - *Why not global proxy:* Spec hard rule — `HTTP_PROXY` globally burns DataImpulse GB, adds latency to every API token, leaks via third-party IP. Proxy **must** be inside `reqwest::Proxy` per request (`src/main.rs:7` in `~/mcp/dataimpulse-mcp` and `src/browser.rs:36` / `src/reddit.rs:230` in `reddit-scrappe`).
  - *Why MCP stdio:* Fits `opencode`/`Claude Code` tool protocol. Implemented JSON-RPC 2.0 manually over `tokio` stdin/stdout instead of pulling heavy `rmcp` crate — 574 LOC, no extra deps, fully controllable.

### 1.2 Protocol

- `initialize` → `{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}`
- `tools/list` → 2 tools
- `tools/call` -> dispatch. Notifications (`notifications/initialized`) get no reply. Verified via `printf '{\"jsonrpc\":...}' | DI_USER=dummy DI_PASS=dummy ./target/debug/dataimpulse-mcp` -> 2 tools.

### 1.3 Proxy auth & geo syntax

DataImpulse spec verified:

```
Host gw.dataimpulse.com | HTTP 823 | SOCKS5 824 | sticky 10000-20000
Auth login:password@host:port
Username: login__cr.es;city.madrid;sessid.myTag
  __ opens params, ; separates, . key/value, , multi-value
  cr ISO2, city requires cr, sessid sticky 30m
Errors: 407 TRAFFIC_EXHAUSTED / THREADS_EXHAUSTED (>2000), 503 NO_RAY (city too narrow), 403 even after proxy
```

- `build_proxy_user(base, country, city, session)` (`src/main.rs:38` in mcp, `src/browser.rs:12` / `src/reddit.rs:230` in scraper) validates `cr` len 2, `city` requires `cr`, sanitizes `sessid` spaces.
- `Proxy::all(format!("http://{}:{}@{}:{}", encoded_user, urlencoding::encode(pass), host, port))` per-request. Password percent-encoded; username only encodes `@:` ` ` `%` to keep `__` `;` `.` intact for DataImpulse parsing.
- Why per-request not per-client global: each `fetch_page` may use different `country/city/session` → sticky per task as `AGENTS.md:83` demands `One IP per task`.
- `city` costs double → default country-only, city only if requested.

### 1.4 `fetch_page` and `check_exit_ip`

**`fetch_page(url, country?, city?, session?, raw?)`:**
- Builds proxy as above, `reqwest::Client::builder().proxy(proxy).timeout(45s).redirect(limited 10).user_agent(Chrome/151)`.
- Real browser headers: `Accept`, `Accept-Language`, `Cache-Control`, `Upgrade-Insecure-Requests`, `Sec-Fetch-*`. **No `Accept-Encoding: gzip`** — with `reqwest` default-features off, manual `gzip, br` returned garbled `0.5KB` garbled instead of `127 chars` clean for `example.com` (fixed by removing header, let server return plain or `reqwest` auto-decompress).
- Follows redirects, 45s timeout (`tokio::time::timeout`), truncates to `60000` chars (`chars().take(60000)`).
- If `raw:false` (default): `clean_html()` strips `<script.*?</script>`, `<style.*?</style>`, `<!--.*?-->` via `regex (?is)`, parses with `scraper::Html`, extracts `body.text()` then `split_whitespace().join(" ")`. If `raw:true` returns HTML raw.
- On `!is_success()` returns `{"isError":true, "content":[{"text": "HTTP 403 ... actionable hint"}]}`. `actionable_hint(status, body)` maps `403` → change country/session, `407 TRAFFIC/THREADS`, `503 NO_RAY` → drop city.

**`check_exit_ip(country?, city?, session?)`:**
- `GET https://api.ipify.org?format=json` via same proxy builder. Parses `{"ip":"1.2.3.4"}` fallback to trimmed text. Used to verify geo before scrape: `us 82.40.113.131` vs `ar 181.209.92.163` vs `es 188.78.184.162` all distinct in e2e, proving residential routing.

**Creds:** `std::env::var("DI_USER")` / `DI_PASS` `die_missing_creds()` panic with `eprintln!` if missing, never hardcoded, never logged. Stored only in `~/.config/opencode/opencode.jsonc` `mcp.di-proxy.environment` user scope via `opencode mcp add di-proxy --env DI_USER=... --env DI_PASS=... -- ~/mcp/dataimpulse-mcp/target/debug/dataimpulse-mcp` (`opencode mcp list` → `✓ connected`). Verified no `env | grep -i proxy` global.

### 1.5 Why not alternatives

- `WebFetch` vs `fetch_page`: `WebFetch` is faster, no GB cost, for public docs. `fetch_page` only for WAF/geo. Documented in `AGENTS.md:83`.
- `HTTP_PROXY` env: rejected — would route OpenAI/Anthropic API tokens via residential IP, burn data, add 50-100ms per token, leak to third party.
- `scraper` vs `lol_html`: `scraper` already used in `reddit-scrappe` for `shreddit-post`, reuse, simpler `Html::parse_document`.

## 2. Browser + Proxy (Plan B / B+)

### 2.1 Why `chromiumoxide 0.9` headless `new`

- `reqwest` alone has JA3 fingerprint of Rust `rustls` → blocked in ms even with residential IP (`AGENTS.md:70` L2). Need real Chrome TLS.
- `chromiumoxide` `BrowserConfig::builder().new_headless_mode().hide().arg("--disable-blink-features=AutomationControlled")` + `Chrome/151 UA` gives plausible JA3. `headless pure` chosen over `with_head()` + `xvfb` on VPS to save RAM (Chrome ~600MB total: main 278MB + headless 255MB + renderer 162MB + network 94MB) and keep CI simple. Headed is debug `HEADLESS=0` only.
- `user_data_dir ~/.cache/reddit-scrappe/profile` persistent — 14 cookies `3274 bytes` via `page.get_cookies()` after re-login, survives restarts. `scp -r` to VPS. `SingletonLock` cleanup in `browser.rs:76` for crashes.

### 2.2 Proxy inside browser

- **Why per-browser not per-system:** `--proxy-server=http(s)://user:pass@gw.dataimpulse.com:823` or `socks5://...:824` (`DI_USE_SOCKS=1` toggles scheme/port). Embedded auth avoids `FetchAuthRequired` handling (Chrome ignores `http_proxy` env). Logged as `country=es city=madrid session=json-test-1 scheme=http (creds hidden)` (`browser.rs:66`).
- **City:** `DI_CITY=madrid` requires `DI_COUNTRY=es`; cost double but allowed per spec `login__cr.es;city.madrid`. Tested `es/madrid` and `us` both succeed for JSON.
- **Sticky:** `DI_SESSION` sets `sessid.` same IP 30m → `One IP per task` for pagination/login flows.
- **Login via proxy:** `login.rs:20` also uses `proxy_server_arg()` via headed window, so 14 cookies are bound to residential IP, not datacenter. Previous 8 cookies `1510 bytes` (datacenter) gave `0 posts`; after `DI_COUNTRY=us cargo run -- --login` re-login at `15:05` gave `14 cookies 3274 bytes` and JSON `25 posts`.

### 2.3 Stealth headless pure

- **Why `evaluate_on_new_document` not just `evaluate`:** `page.evaluate` runs after `DOMContentLoaded`, page scripts already saw `navigator.webdriver`. `page.evaluate_on_new_document(STEALTH_JS)` (`reddit.rs:50`) injects before frame creation (Playwright `addInitScript` equivalent). `STEALTH_JS` hides `navigator.webdriver` (both direct and `Object.getPrototypeOf`), `window.chrome.runtime`, `navigator.languages ['en-US','en']`, `plugins [5]`, `platform MacIntel`, `permissions.query`.
- Applied before every `goto` (`search_human` warmup and per-query, `search_old_reddit_via_browser`). Keeps `new_headless_mode` while hiding headless flag.

### 2.4 Human behavior

- `human::sleep_jitter 800-2000ms` for scroll, `2000-4000ms` between queries, `human_scroll` `window.scrollBy(0,500-700)` x2-3 steps. Warm-up `goto https://www.reddit.com/` 2s + scroll before search helps WAF L3. Irregular loop `25-45m random + ±60s jitter + shuffle queries + night pause 0-7am` planned (`main.rs:61` currently `30m ±60s`), `max_age 48h` + `sort=new t=week` optimal per research (Hot window 1-3h, index delay minutes-hours).

## 3. Reddit Search & Fallbacks

### 3.1 `shreddit-post` primary

- `reddit.rs:50` `search_human` builds `https://www.reddit.com/search/?q={}&sort=new&t=week` (or `/r/{sub}/search`), `page.goto`, poll `find_element("shreddit-post")` 15x1s + alternatives `div[data-testid='post-container']`, `article`. Dismiss wall `button:has-text('Continue')`. If found, `human_scroll`, `page.content()`, `parse_reddit_html` (`scraper` `shreddit-post` attrs `id/post-title/subreddit-prefixed/author/score/comment-count/permalink`). E2E with proxy `DI_COUNTRY=us` still `timeout 15s` `html len 189958 theme-beta` `title None` `is_captcha true` (`Prove your humanity|blocked by network security|cf-challenge` `reddit.rs:185`) — HTML `shreddit` remains JS-heavy and WAF-blocked even with residential+login+stealth.

### 3.2 Fallback chain → JSON wins

- **Why JSON over HTML:** `old.reddit/search.json?q=...&sort=new&t=week&limit=25` returns `{"data":{"children":[{"data":{"id","title","subreddit","author","score","num_comments","created_utc","permalink","over_18","selftext"}}]}}` without needing JS. HTML `div.thing[data-fullname]` needs full page and is also WAF-blocked (`403 theme-beta` via `reqwest` without `Cookie`).

**Chain in `search_human`:**
1. `search_old_reddit_json` via `reqwest::Proxy` + `Cookie` from `page.get_cookies()` (3274 bytes, 14 parts). `GET https://old.reddit.com/search.json?...` `Accept: application/json`, `build_reqwest_proxy()` same geo, `Cookie` header. On `!is_success()` log `HTTP 403` with `NO_RAY/TRAFFIC_EXHAUSTED` hints. Parse `parse_reddit_json` (`serde_json` `data.children`).
2. If `0`, `search_fallback_old_reddit_with_cookies` HTML `old.reddit/search?q=...` same proxy+cookie, `parse_old_reddit_html` (`div.thing`).
3. If still `0`, `search_old_reddit_via_browser` `Page.goto old.reddit` + `human_scroll` + `is_captcha` check.

**Result:** `DI_COUNTRY=es DI_CITY=madrid DI_SESSION=json-test-1 cargo run -- --once --config /tmp/test_config.yaml` (`q=rust r/rust`): `shreddit timeout` but `fallback json GET ... country=es city=madrid` → `parsed 25 posts from json` → `25 raw 25 after filter` `notify 5` (`I crocheted Ferris 382↑`, `GPU Offload 306↑`...). `DI_COUNTRY=us` also `25`. `q=speech-to-text` `t=week` gave 25 generic (no `speech` in title) because no recent exact matches → window `t=week` fallback to recent globals; `q=rust` relevant because many recent `rust` posts. Fix: use `t=all` or `q=speech` without hyphen, or `sort=relevance`; `urlencoding` now via proper `replace(' ','+')` but should be `urlencoding` crate for `"` etc. (future).

**Why `reqwest::Proxy` per-request not global:** each fallback may use different `country/city/session` per `AGENTS.md:83` `One IP per task` vs `One IP per request`.

**Why `page.get_cookies()` not file:** `~/Library/Caches/reddit-scrappe/profile/Default/Cookies` `value` empty (Chrome encrypts to `encrypted_value`, `sqlite count 14` with `length(value) 0`), only CDP `page.get_cookies()` returns decrypted `value`. `rusqlite` fallback kept for anon without page, but `get_reddit_cookie_header_via_page` is primary. Added `rusqlite 0.32 bundled` `Cargo.toml:22` for file fallback and `get_reddit_cookie_header_from_file`.

**Why `urlencoding` simple `replace(' ', '+')`:** minimal for `q` with spaces, enough for `AI infrastructure` tests (`config.yaml:4` `q: "..."`). For `speech-to-text` hyphen, `+` not needed; for `"speech to text"` phrase needs `urlencoding` crate (future fix).

### 3.3 Other layers

- `filter.rs` `filter_posts` by `min_score 2`, `min_comments 0`, `max_age 48h`, `exclude_nsfw`, `seen.json` dedup.
- `notifier.rs` `notify_console` `take(5)` per query — why 5 shown for `rust` while `25` fetched. `max_age 48h` matches Hot window.
- `main.rs:12` auto-loads `.env` from `./.env` or `/Users/ramiro/reddit-scrappe/.env` (not overwriting existing env) so `cargo run` without `export` works; `opencode mcp list` still `✓ connected` via `~/.config/opencode/opencode.jsonc`.
- `config.rs` `serde_yaml` 3 queries thematic, `schedule_minutes 30`.

## 4. Data & SEO Decisions

- `sort=new t=week` + `max_age 48h` vs `Hot 1-3h` — `new` catches fresh threads where comment still visible, `Hot` buries older. `week` gives index time to appear (Reddit search delay minutes-hours).
- `old.reddit/search.json` `limit 25` not `100` — balances data (50-80KB JSON per query, ~300KB with browser warmup) vs DataImpulse GB (43MB/day at 48 runs, 27MB/day at 30 runs with night pause).
- City targeting double cost — default country-only, city only if `403` persists (not needed for JSON as `us` alone gave 25).

## 5. What Was Necessary

- **DataImpulse residential proxy:** Yes for `old.reddit/search.json` — anon `403 theme-beta` even via `dataimpulse-mcp` without `Cookie`; with `Cookie` + proxy succeeded. Datacenter IP + `requests` JA3 blocks in ms (`AGENTS.md:70`).
- **Browser login to get Cookie:** Yes — `14 cookies 3274 bytes` via `page.get_cookies()` required; file `Cookies` encrypted, only CDP works. `old.reddit/search.json` without `Cookie` still `403`.
- **Browser simulation for each `search` GET:** No for `search.json` — `reqwest` with `Proxy+Cookie` alone fetched 25. Browser `shreddit` attempt (189k `theme-beta`) is overhead; warmup `goto /` not needed for JSON. Kept for future `shreddit_post` if WAF loosens, but could be skipped to save ~240KB/query → `~5MB/day` instead of `43MB`.

## 6. Rejected Alternatives

- `HTTP_PROXY` global: burns GB, leaks API via residential, latency — rejected per hard rule.
- `SOCKS5 :824` default: works but `http :823` already succeeds for JSON; `socks` optional via `DI_USE_SOCKS=1`.
- `Playwright`/`puppeteer` vs `chromiumoxide`: `chromiumoxide` native Rust, `hide()` + `new_headless` fits `tokio`, lighter.
- `requests` Python legacy `scraper.py`: JA3 blocked, kept for reference only.
- OAuth `gateway.reddit.com` `100/min`: Not needed for read-only JSON with login cookie; kept as next step if JSON also gets `403` persistently.

## 7. Verification

- `cargo build` 1 warning `dead_code` (`search_fallback_old_reddit` wrapper kept for compat), `cargo test 8 passed`
- `opencode mcp list` `di-proxy ✓ connected`, `check_exit_ip` `us 82.40.113.131` / `ar 181.209.92.163` / `es/madrid 83.32.219.165` distinct
- `fetch_page https://example.com country=ar` clean `Example Domain...` 127 chars vs raw `559`
- `DI_COUNTRY=es DI_CITY=madrid cargo run -- --once --config /tmp/test_config.yaml` `q=rust` → `25 posts` via JSON; `q=speech-to-text t=week` → `25` generic (no recent exact) vs `q=rust` relevant — fix via `t=all`/`sort=relevance`
- `memory` Chrome ~600MB + Rust ~28MB RSS, binary 63M

## 8. How to Use

```bash
# proxy creds
cat > .env <<'ENV'
DI_USER=your_login
DI_PASS=your_password
DI_COUNTRY=es
DI_CITY=madrid
DI_SESSION=reddit-1
ENV
DI_COUNTRY=es DI_CITY=madrid DI_SESSION=reddit-1 cargo run -- --login # headed, ENTER
DI_COUNTRY=es DI_SESSION=reddit-1 RUST_LOG=info cargo run -- --once
DI_COUNTRY=us cargo run -- --loop
```

## 9. Next Steps

- Make `search_old_reddit_json` respect `config.yaml sort/t` (`new` vs `relevance`, `week` vs `all`) and proper `urlencoding` for phrases `"speech to text"`.
- Optionally skip `shreddit` browser fetch entirely for `search.json` to save 240KB/query.
- Monitor `403`/`429`/`is_captcha` and auto-rotate `country/session` per `AGENTS.md:83` `403 → change country/session`, `503 NO_RAY → drop city`.

