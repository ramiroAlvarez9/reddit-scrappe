# Results — What Works After Re-login

## DataImpulse MCP
- `cargo build` in `~/mcp/dataimpulse-mcp` OK, `di-proxy` `✓ connected` (`opencode mcp list`)
- `check_exit_ip`:
  - no country → `14.233.157.231`
  - `ar` → `181.209.92.163`
  - `es` → `188.78.184.162`
  - `us` → `82.40.113.131` (distinct, geo works)
- `fetch_page https://example.com country=ar` → clean `127` `Example Domain...` vs raw `559`
- `fetch_page https://www.reddit.com/ country=us` → `8393` `Reddit` JS challenge (`snoo` SVG + `document.addEventListener... solution`) `is_captcha` true
- `fetch_page https://old.reddit.com/search.json?q=speech-to-text country=es` anon → `403 theme-beta` (expected L4) — needs `Cookie`

## Reddit-scrappe after Plan B+ (browser+proxy+JSON)

**Profile:** `~/Library/Caches/reddit-scrappe/profile/Default/Cookies` `Aug 17 15:05` `14` reddit cookies `count 14`, `page.get_cookies()` `3274 bytes 14 parts` `reddit_session/lo...` (re-login via residential `DI_COUNTRY=us`).

**Test `/tmp/test_config.yaml` `q=rust r/rust sort=new limit=5`:**

| Run | `DI_COUNTRY` `DI_CITY` | `shreddit-post` | Fallback | Result |
|-----|------------------------|-----------------|----------|--------|
| `es/madrid json-test-1` | `es madrid` | `timeout 15s 189958 theme-beta title None is_captcha` | `GET https://old.reddit.com/r/rust/search.json?... limit=25` via `proxy country=es city=madrid session=json-test-1` `parsed 25 posts from json` | `25 raw 25 after filter 5 notify` `r/rust 382↑ I crocheted Ferris...` `306↑ GPU Offload...` |
| `us json-test-2` | `us` | `timeout 189k` | `json GET ... country=us` `parsed 25` | `25 raw 25 filter` same |
| `ar probe-ar` `es probe-es` pre-JSON | `ar/es` | `timeout 189k` | `old.reddit HTML div.thing 0` + `browser old.reddit captcha 0` | `0 raw 0 filter` (HTML still blocked) |

**Speech-to-text:**
- `q=speech-to-text t=week sort=new` via `json` + `proxy es/madrid` + `cookies` → `25` generic `r/teenagers` etc. (no `speech` in title) because no exact recent matches in `week` — `q=rust` same `new` gave relevant. Fix: `q=speech t=all sort=relevance` or `q="speech to text"`.
- Via `dataimpulse-mcp` anon `old.reddit/search.json?q=speech-to-text` → `403 theme-beta` (needs `Cookie`).

**Rust ordered by `new` (requested):**
```
r/rust 382↑ 11💬 I crocheted Ferris for my boyfriend! https://www.reddit.com/r/rust/comments/1vq2fjm/...
r/rust 306↑ 24💬 [2608.13759] GPU Offload in Rust https://www.reddit.com/r/rust/comments/1vqfsdi/...
r/rust 67↑ 11💬 burli: Brotli codec https://www.reddit.com/r/rust/comments/1vqny3x/...
r/rust 12↑ 30💬 What's everyone working on this week https://www.reddit.com/r/rust/comments/1vqmlm5/...
r/rust 18↑ 25💬 Mutable Global State https://www.reddit.com/r/rust/comments/1vqlres/...
```
`25 raw 25 filter` but `notify take(5)` keeps STDOUT readable, ordered by Reddit `sort=new` (most recent first). `fresh_seen.json` needed to bypass `seen.json` `51` dedup.

**Resource:**
- Binary `63M`, `cargo test 8 passed` 1 warning `dead_code`
- `RUST_LOG=info` shows `proxy enabled country=es city=madrid session=... scheme=http` + `using 3274 bytes reddit cookies`
- Memory `Chrome ~600MB` (main 278M + headless 255M + renderer 162M) + Rust `~28MB RSS` = `~630MB`, `loop` reuses 1 `Browser`.

## What Is Complete
- Residential proxy `gw.dataimpulse.com:823/824` per-request (`DI_USER`/`DI_PASS`/`DI_COUNTRY`/`DI_CITY`/`DI_SESSION`/`DI_USE_SOCKS`) with actionable `407/503/403` hints.
- Headless pure `new` + `STEALTH_JS` `evaluate_on_new_document` + warm-up `goto /` + `human_scroll` + persistent login.
- `old.reddit/search.json` + proxy + `page.get_cookies()` JSON bypasses `shreddit` WAF. HTML `shreddit` still `theme-beta` even with all mitigations.

## Still Blocked
- `www.reddit/search` `shreddit-post` HTML via browser+proxy+login+stealth still `theme-beta` for all `cr`. Not needed now that JSON works.

## Data Cost
- `shreddit` attempt `~240KB` + `JSON` `~60KB` = `~300KB/query` → `3 queries/run` `~0.9MB/run` → `48 runs/day ~43MB` (`30 runs with night pause ~27MB`). JSON-only without browser would be `~60KB/query` `~5MB/day`.
