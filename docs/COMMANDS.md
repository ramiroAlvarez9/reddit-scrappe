# Commands — reddit-scrappe

> Complete guide for humans and agents. All commands, flags and parameters explained.

**Binary:** `reddit-scrappe` (`cargo run -- ...`). Rust 1.97, Chrome 151.

---

## 1. Quick overview

| Audience | Base command | Format | Purpose |
|---|---|---|---|
| **Humans** | `cargo run -- --once` / `cargo run -- search 'q: rust, limit: 5'` | ANSI table `┌─┬─┐` + full links below `↳` | Quick terminal view of opportunities |
| **Agents / Automation** | `cargo run -- search 'q: rust, limit: 10' --format json` / `cargo run -- --once --no-browser` | JSONL (1 JSON per line) | Machine-parseable, light VPS, no ANSI |
| **Setup** | `cargo run -- --login` | Headed Chrome | Generate `profile` + `cookies.json` once |

---

## 2. Global commands (`src/main.rs:15`)

### 2.1 Main flags

```bash
cargo run -- [FLAGS] [SUBCOMMAND]
```

| Flag | Type | Default | Meaning |
|---|---|---|---|
| `--login` | bool | `false` | Opens **headed** Chrome at `https://www.reddit.com/login`, you log in manually + press `ENTER`, saves `~/.cache/reddit-scrappe/profile` (14 cookies ~3KB) + decrypted `cookies.json`. **Must be run with residential proxy** `DI_COUNTRY=es cargo run -- --login` to bind session to residential IP. |
| `--logout` | bool | `false` | Removes `profile` + hint `reddit.com/settings/account` to revoke. |
| `--once` | bool | `false` (implicit default if not `--loop`) | Single sequential run over all queries in `config.yaml`. With browser: `chromiumoxide` headless `new`. Without: see `--no-browser`. |
| `--loop` | bool (`--loop`) | `false` | Infinite loop every `schedule_minutes` (default 30) + jitter `±60s` + `sleep 2-4s` between queries. Sequential per query. Saves fresh cookies each iteration. |
| `--no-browser` | bool | `false` | **Light mode**: skips Chromium, uses only `reqwest::Proxy` + `cookies.json` (28MB RSS / 60KB per query vs 630MB/300KB with browser). Env alias `NO_BROWSER=1`. Requires prior `--login`. ~2s vs ~25s. |
| `--config` | `String` path | `config.yaml` | Path to YAML config with queries/filters/schedule. Eg. `--config /tmp/test.yaml`. |
| `--seen` | `String` path | `seen.json` | Dedup store JSON array of seen IDs. Eg. `--seen /tmp/fresh.json` for isolated run without dedup. |
| `RUST_LOG` | env | `info` | Tracing level: `debug` shows `[warmup]`, `[nav]`, `[fallback json]`. Eg. `RUST_LOG=debug cargo run -- --once`. |
| `HEADLESS` | env | `1` (headless) | `HEADLESS=0 cargo run -- --once` opens headed for debug. |
| `CAPTCHA_WAIT_SECS` | env | `30` | Wait seconds on login if captcha appears. |
| `CHROME_PATH` | env | auto | Override Chrome binary path. |

### 2.2 Global examples

```bash
# setup
brew install --cask google-chrome   # Mac
sudo apt install -y chromium-browser # VPS
cargo build

# login once (residential)
DI_COUNTRY=es DI_CITY=madrid DI_SESSION=reddit-1 cargo run -- --login
# -> headed window, log in, ENTER

# runs
cargo run -- --once                         # headless with browser
cargo run -- --once --no-browser            # light: reqwest + cookies
DI_COUNTRY=es cargo run -- --once --no-browser
cargo run -- --once --config /tmp/a.yaml --seen /tmp/b.json

cargo run -- --loop                         # loop 30m ±60s
cargo run -- --loop --no-browser
NO_BROWSER=1 cargo run -- --loop

# debug
RUST_LOG=debug cargo run -- --once
HEADLESS=0 cargo run -- --once
cargo run -- --logout

# utils
ls ~/Library/Caches/reddit-scrappe/profile/Default/Cookies
cat /tmp/reddit_debug.html | grep -o "<title>.*</title>"
cargo test -- --nocapture          # 45 unit
cargo test -- --ignored --nocapture # e2e (requires Chrome)
```

---

## 3. Subcommand `search` — direct declarative search

### 3.1 Syntax

```bash
cargo run -- search '<inline YAML>' [--format table|json]
cargo run -- search 'q: rust, subreddits: [rust], sort: new, limit: 10' --format json
```

- `yaml: String` `src/main.rs:51` — **single argument** with inline YAML.
- Tolerates 3 forms `src/config.rs:93`:
  1. One-liner flow: `q: rust, subreddits: [rust], limit: 10` (auto-wrapped in `{}`)
  2. Explicit braced: `'{q: rust, limit: 5}'`
  3. Block multiline: `"q: rust\nsubreddits: [rust]\nlimit: 10"` or `'{q: rust,\nlimit:5}'`
- Always shows exactly `limit` posts (no `seen.json` dedup), uses permissive filters if none provided.
- **Requires valid cookies** `src/main.rs:241` — if `~/.cache/reddit-scrappe/profile/cookies.json` missing/expired, exits 1 with message `DI_COUNTRY=... cargo run -- --login`.
- Internally uses `search_no_browser` `src/reddit.rs:198` -> `build_search_json_url` `src/reddit.rs:554` -> `https://old.reddit.com/search.json` via proxy+cookies.

### 3.2 `search` parameters (`src/config.rs:68` `SearchArgs`)

| Param | Type | Required | Default | Meaning | Example |
|---|---|---|---|---|---|
| `q` | `String` | **yes** | — | **Reddit global search query** — same as `reddit.com/search`. Free text, supports `OR` `AND` `"-exclude"` `"exact phrase"`. URL-encoded `space -> +` `src/reddit.rs:602`. Eg. `"AI infrastructure OR MLOps"`. | `q: rust`, `q: "rust lang, async"`, `q: "speech-to-text"` |
| `subreddits` | `Vec<String>` | no | `[]` = whole Reddit | Restricts search. Empty = `https://old.reddit.com/search.json`, with values = `https://old.reddit.com/r/{sub}/search.json?...&restrict_sr=on` per sub `src/reddit.rs:206`. | `subreddits: [rust]`, `subreddits: [MachineLearning, LocalLLaMA]` , `subreddits: []` |
| `sort` | `String` | no | `new` | Order: `new` (recent) \| `relevance` \| `top`. Maps to `&sort=new&t=week` in URL. `new` + `t=week` optimal for OrchardRun (Reddit index delay). | `sort: new`, `sort: relevance` |
| `limit` | `u32` | no | `20` `src/config.rs:46` | How many posts to fetch (URL `&limit=`) and display (`take(limit)` `src/notifier.rs:89`). `0` allowed (shows 1 via `limit.max(1)`). | `limit: 10`, `limit: 3` |
| `filters` | `Filters?` | no | `None` -> permissive `min_score:0, min_comments:0, max_age_hours:720, exclude_nsfw:false` `src/config.rs:83` | Inline filters. If specified, **all 4 fields are required** (no defaults) `src/config.rs:29`. | `filters: {min_score:5, min_comments:2, max_age_hours:24, exclude_nsfw:true}` |

**`Filters` detail** `src/config.rs:29`:

| Field | Type | Meaning |
|---|---|---|
| `min_score` | `i32` | Minimum score (upvotes). `0` = all. |
| `min_comments` | `u32` | Minimum comments. |
| `max_age_hours` | `u64` | Maximum age in hours. Post with `now - created_utc > max_age` filtered `src/filter.rs:11`. `720h = 30 days`. |
| `exclude_nsfw` | `bool` | `true` filters `over_18`. `false` keeps NSFW `src/filter.rs:14`. |

**`--format`** `src/main.rs:53` `ValueEnum`:

| Value | Audience | Output | Notes |
|---|---|---|---|
| `table` (default) | Humans | ANSI table `┌───┬────────────────┬───────┬────────┬...` + `│ # │ subreddit │ score │ comments │ title │` + line below `↳ https://...` full (not truncated `src/notifier.rs:168`) | Cyan `r/sub`, green `>=100`, yellow `>=20`, full copyable links. |
| `json` | Agents | JSONL `1 JSON per line` `src/notifier.rs:183` `serde_json::to_string(Post)` | Each line is `Post {id,title,subreddit,author,score,num_comments,created_utc,url,selftext,over_18}`. Logs go to stderr, posts to stdout for piping. |

### 3.3 `search` examples

```bash
# human — table, 3 posts any sub
cargo run -- search 'q: rust, limit: 3'

# human — 10 posts only r/rust
cargo run -- search 'q: rust, subreddits: [rust], limit: 10'

# agent — JSONL for parsing
cargo run -- search 'q: rust, limit: 10' --format json | jq .

# full inline filters
cargo run -- search '{q: rust, subreddits: [rust], sort: new, limit: 5, filters: {min_score: 3, min_comments: 1, max_age_hours:24, exclude_nsfw:true}}'

# multiline block (bash $'...')
cargo run -- search $'q: rust\nsubreddits: [rust, programming]\nlimit: 10\nsort: new'

# q with OR and phrases (double quotes inside single quotes)
cargo run -- search 'q: "AI infrastructure OR MLOps OR vector database", subreddits: [MachineLearning, LocalLLaMA], limit: 20'
cargo run -- search 'q: "seed round OR fundraising OR VC", subreddits: [startups, vc], limit: 20'

# no filters = permissive (always returns)
cargo run -- search 'q: orchard, limit: 10'  # -> min_score 0, 720h, nsfw false

# error without cookies (clear)
# cargo run -- search 'q: rust, limit: 3'
# Error: no valid login cookies found at .../cookies.json
# You must log in once first. Run: DI_COUNTRY=<iso2> cargo run -- --login
```

---

## 4. `config.yaml` — `--once`/`--loop` mode (persistent queries)

```yaml
# src/config.rs:4
queries:  # Vec<Query>
  - name: "orchard ai infra"                          # String: label for logs [query:name]
    q: "AI infrastructure OR MLOps OR vector database" # String: query (as above)
    subreddits: [MachineLearning, artificial, LocalLLaMA, datascience] # Vec<String>, []=all
    sort: "new"                                        # String default new
    limit: 20                                          # u32 default 20

  - name: "startup fundraising"
    q: "seed round OR fundraising OR VC"
    subreddits: [startups, Entrepreneur, vc]
    sort: "new"
    limit: 20

  - name: "orchardrun directo"
    q: "orchard"
    subreddits: [] # empty = whole reddit
    sort: "new"
    limit: 10

filters: # Global Filters for --once/--loop src/config.rs:36 default_filters
  min_score: 2        # i32 default 2
  min_comments: 0     # u32 default 0
  max_age_hours: 48   # u64 default 48 (only last 48h)
  exclude_nsfw: true  # bool default true

schedule_minutes: 30  # u64 default 30 src/config.rs:44 (loop every 30m ±60s jitter)
notifier: "console"   # String default console (only console active)
```

Difference `search` vs `config.yaml`: `search` is 1 ad-hoc query without `name` and without dedup; `config.yaml` has N queries with `name` + dedup `seen.json`.

---

## 5. Environment variables — proxy and debug

### 5.1 Residential proxy DataImpulse (`src/browser.rs:36`, `src/reddit.rs:303`, `AGENTS.md:90`)

**Hard rule:** never `HTTP_PROXY/HTTPS_PROXY` globally — burns GB, leaks API tokens, adds latency. Only `reqwest::Proxy` and `--proxy-server`.

| Var | Required | Default | Meaning |
|---|---|---|---|
| `DI_USER` | proxy yes | — | DataImpulse login |
| `DI_PASS` | proxy yes | — | DataImpulse password (percent-encoded internally) |
| `DI_COUNTRY` | no | — | ISO2 `ar,es,us,de` -> `cr.es` |
| `DI_CITY` | no | — | City `madrid`, requires `DI_COUNTRY`, **costs double** |
| `DI_SESSION` | no | — | Sticky `sessid.myTag` -> same IP 30m per task |
| `DI_USE_SOCKS` | no | `0` (http) | `1` -> `socks5://gw.dataimpulse.com:824` else `http://:823` |

Internal syntax: `login__cr.es;city.madrid;sessid.myTag` (`__` opens params, `;` separates, `.` key/value) -> `http(s)://user:pass@gw.dataimpulse.com:823/824`.

Storage: `~/.config/opencode/opencode.jsonc` `mcp.di-proxy.environment` (MCP) or `.env` (auto-loaded `src/main.rs:57` from `./.env` or `/Users/ramiro/reddit-scrappe/.env` without overwriting existing). Never hardcoded.

DataImpulse errors: `407 TRAFFIC_EXHAUSTED` out of GB, `407 THREADS_EXHAUSTED` >2000 conns, `503 NO_RAY` no IPs for that targeting (drop `DI_CITY`), `403` change country/session.

### 5.2 Debug / runtime

| Var | Meaning |
|---|---|
| `RUST_LOG=debug` | `tracing_subscriber` verbose `src/main.rs:163` |
| `HEADLESS=0` | Headed debug (vs `new_headless_mode`) |
| `NO_BROWSER=1` | Alias for `--no-browser` `src/main.rs:99` |
| `CAPTCHA_WAIT_SECS` | Wait on login if captcha |
| `CHROME_PATH` | Override Chrome binary |

`.env` example:

```bash
DI_USER=your_login
DI_PASS=your_password
DI_COUNTRY=es
DI_CITY=madrid
DI_SESSION=reddit-1
# DI_USE_SOCKS=1
```

---

## 6. Agents (automation) vs humans

### Humans

- `cargo run -- search 'q: rust, limit: 5'` -> ANSI table with colors + links `↳` below row, fully copyable (fix `src/notifier.rs:168`, previously truncated to 60).
- `cargo run -- --once` / `--loop` with browser to see `theme-beta` etc. in `RUST_LOG=debug`.
- `HEADLESS=0` to see window.

### Agents

- `cargo run -- search 'q: rust, limit: 10' --format json` -> JSONL stdout (stderr is logs). Ideal for `jq`, piping, `seen.json` not applied.
- `cargo run -- --once --no-browser --config ... --seen /tmp/fresh.json` -> light VPS, 2s, 28MB, parseable.
- `fetch_page(url, country?, city?, session?, raw?)` via MCP `di-proxy` for public pages that block/are geo-dependent (not `WebFetch`), `check_exit_ip(country)` to verify IP. `AGENTS.md:90`.

---

## 7. Full flow `q` -> real values

1. CLI `search '<yaml>'` -> `parse_search_args` `src/config.rs:93` (flow wrapping, `serde_yaml`, validates `q` non-empty) -> `SearchArgs` -> `effective_filters` (permissive if `filters:None`).
2. `search_no_browser` `src/reddit.rs:198` iterates `subreddits` (if `[]` -> `[""]` = all), `build_search_json_url` `src/reddit.rs:554` with `urlencoding` `replace(' ','+')` -> `GET https://old.reddit.com/search.json?...` via `reqwest::Proxy` + `Cookie` header from `cookies.json`.
3. `parse_reddit_json` `src/reddit.rs:516` maps `data.children[].data.{id,title,subreddit,author,score,num_comments,permalink->url,over_18,created_utc,selftext}` to `Post`.
4. `filter_posts` `src/filter.rs:5` filters by `Filters` + dedup + sort `score+comments*2`.
5. `notify` `src/notifier.rs:13` `Table` (human) or `Json` (agent) with `take(limit)`.

---

## 8. Tests

```bash
cargo test -- --nocapture                    # 45 unit (config/reddit/filter/notifier/cookies/human)
cargo test -- --ignored --nocapture          # e2e (requires Chrome)
REDIDT_E2E=1 cargo test e2e_reddit -- --ignored --nocapture # e2e reddit fixture/real
```

New coverage: `config::parse_search_args_*` (braced, multiline, partial filters, wrapping edge, load_config tempfile), `reddit::urlencoding_*, url_build_*, parse_reddit_json_*`, `filter::max_age_boundary` etc.

---

## 9. Troubleshooting

- `Error: no valid login cookies` -> `DI_COUNTRY=es cargo run -- --login` + `ENTER` in window.
- `timeout shreddit-post 15s` + `theme-beta 189k` -> WAF block, fallback JSON should give `parsed 25 posts` (if `0`, change `DI_COUNTRY/DI_SESSION`, check `cookies::cookies_status()` `src/cookies.rs:108`).
- `old.reddit 403` anon -> use proxy+cookies.
- `407 TRAFFIC_EXHAUSTED` -> refill DataImpulse.
- `503 NO_RAY` -> drop `DI_CITY`, leave only `DI_COUNTRY`.
- `Cookies` sqlite empty is normal (Chrome encrypts), use `page.get_cookies()` via CDP.

---

## 10. Notes

- Python `scraper.py` legacy, Rust primary.
- Never commit `profile/`, `cookies.json`, `seen.json`, `.env` (`.gitignore`).
- VPS: `scp -r ~/Library/Caches/reddit-scrappe/profile user@vps:~/.cache/reddit-scrappe/profile` after local login.
- Revoke: `cargo run -- --logout` + `reddit.com/settings/account` change password.
- Remote repository: `https://github.com/ramiroAlvarez9/reddit-scrappe` (branches `master`, `feat/no-browser-mode`).
