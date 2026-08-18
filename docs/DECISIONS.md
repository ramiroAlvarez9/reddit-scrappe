# ADRs — Why Each Decision

## ADR-001: Hard rule no global HTTP_PROXY
- **Decision:** Proxy only inside `reqwest::Proxy` per request and `BrowserConfig --proxy-server`.
- **Why:** Global `HTTP_PROXY` would route every OpenAI/Anthropic API token via residential IP → burns DataImpulse GB, +50-100ms per token, leaks via third party. Verified `env | grep -i proxy` empty.
- **Rejected:** `export HTTP_PROXY=...` in `.zshrc`.

## ADR-002: Separate `dataimpulse-mcp` binary
- **Decision:** `cargo new dataimpulse-mcp` Rust MCP stdio with 2 tools `fetch_page`/`check_exit_ip`.
- **Why:** Reuse for any geo-sensitive scraping (prices, stock), not only Reddit. Rust gives single binary, no JA3, per-request proxy. Manual JSON-RPC over `tokio` avoids heavy `rmcp` dep.

## ADR-003: Per-request proxy geo syntax
- **Decision:** `login__cr.es;city.madrid;sessid.tag` built per call.
- **Why:** Different queries may need different geo; sticky `sessid` 30m needed for pagination/login flows `One IP per task`.

## ADR-004: DataImpulse host/port
- **Decision:** `gw.dataimpulse.com:823` HTTP default, `824` SOCKS via `DI_USE_SOCKS`.
- **Why:** Rotating 823 is cheapest, 824 SOCKS alternative for JA3. Tested both, HTTP already works for JSON.

## ADR-005: 60k truncation + 45s timeout
- **Decision:** `MAX_CHARS 60000`, `TIMEOUT 45s`.
- **Why:** LLM context window, avoid OOM on large pages; Reddit search HTML 189k truncated, JSON 25 posts ~50KB well under.

## ADR-006: `scraper` for clean text
- **Decision:** `scraper::Html` + `regex` stripping `script/style/comments` → `body.text()`.
- **Why:** `example.com` clean `127` vs raw `559` verified; `lol_html` heavier, `scraper` already used for `shreddit`.

## ADR-007: `chromiumoxide new_headless` vs headed
- **Decision:** `new_headless_mode` + `hide()` headless pure, `with_head()` only for `--login`.
- **Why:** Headless pure saves ~250MB vs headed `xvfb` on VPS, still gets `shreddit` timeout same as headed, so headed not needed for reads.

## ADR-008: Persistent profile `~/.cache/reddit-scrappe/profile`
- **Decision:** `user_data_dir` persistent, `SingletonLock` cleanup, `page.get_cookies()` 3274 bytes.
- **Why:** 14 cookies survive restarts, `scp -r` to VPS. File `Cookies` encrypted (`value` empty), only CDP works.

## ADR-009: `evaluate_on_new_document` stealth
- **Decision:** `STEALTH_JS` via `page.evaluate_on_new_document` before `goto`.
- **Why:** `navigator.webdriver` must be hidden before page scripts run, not after. `evaluate` post-`goto` too late.

## ADR-010: Warm-up `goto /` + `human_scroll`
- **Decision:** `goto https://www.reddit.com/` 2s + `human_scroll 500-700 x2-3` + `sleep_jitter 800-4000ms`.
- **Why:** WAF L3 behavioral — fixed intervals/bursts trigger `theme-beta`. Warm-up establishes session.

## ADR-011: `old.reddit/search.json` over `shreddit-post` HTML
- **Decision:** Fallback chain `shreddit-post` 15s poll → `search_old_reddit_json` (proxy+cookie) → `search_fallback_old_reddit_with_cookies` HTML → `search_old_reddit_via_browser`.
- **Why:** With proxy+login+stealth, `shreddit-post` still `timeout 189k theme-beta` for all `cr` (`us/ar/es`), but `search.json` via `reqwest` + `Cookie` returns `25` immediately. JSON bypasses JS, 50KB vs 240KB browser.

## ADR-012: `rusqlite` for cookie file fallback
- **Decision:** `rusqlite 0.32 bundled` to read `Cookies` sqlite as fallback when no `Page`.
- **Why:** File `value` empty on Chrome 151 (encrypted), but kept for anon without page; primary is `page.get_cookies()`.

## ADR-013: Irregular human schedule
- **Decision:** `schedule 30` + `25-45m random` + `±60s jitter` + `shuffle queries` + night pause `0-7am` planned (`main.rs:61` currently `30m ±60s`).
- **Why:** WAF L3 flags fixed intervals; `t=week` Hot window `1-3h` optimal for OrchardRun per research.

## ADR-014: City targeting double cost
- **Decision:** Default `country` only, `city` only if requested, cost ok per user.
- **Why:** `503 NO_RAY` when city too narrow; `es/madrid` tested `25 posts` but `us` alone also `25`, so city not required for JSON.

## ADR-015: `seen.json` dedup + `take(5)` notifier
- **Decision:** `filter max_age 48h` + `seen.json` + `notify_console take(5)`.
- **Why:** `sort=new t=week` + `48h` matches Hot; `take(5)` keeps STDOUT readable, `25` fetched still stored.

## ADR-016: JSON `sort`/`t` not yet configurable
- **Decision:** `search_old_reddit_json` hardcodes `sort=new&t=week`.
- **Why:** Simplest for `OrchardRun` new threads; `q=speech-to-text t=week` gave 25 generic (no recent exact) vs `q=rust` relevant — next fix is to expose `sort`/`t` from `config.yaml`.

## ADR-017: No `urlencoding` crate for `q`
- **Decision:** `s.replace(' ', '+')` for `q`.
- **Why:** Enough for `AI infrastructure` tests; fails for `"speech to text"` phrase needing `%20`/`%22`. Future fix: `urlencoding` crate.

## ADR-018: `reqwest socks` feature
- **Decision:** `reqwest 0.13 rustls+socks`.
- **Why:** Enable `socks5://:824` alternative; HTTP already works, socks kept optional via `DI_USE_SOCKS=1`.
