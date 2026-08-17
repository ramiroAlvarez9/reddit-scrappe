# AGENTS - reddit-scrappe

> Scraper Rust que simula humano anon con login persistente para buscar posts donde hablar de OrchardRun. Solo STDOUT local por ahora, irregular humano anti-bloqueo.

## Stack

- **Rust 1.97** `tokio 1` + `chromiumoxide 0.9` headless `new` + `scraper 0.27` + `reqwest 0.13 rustls` fallback
- `serde_yaml 0.9` `clap 4` `tracing 0.1` `rand 0.8` `dirs 5` `anyhow 1`
- Chrome 151 via `brew install --cask google-chrome` (Mac) / `apt install chromium` (VPS)

## Estructura

```
src/
  main.rs      # CLI --login/--logout/--once/--loop --config --seen, loop 25-45m irregular shuffle (plan)
  browser.rs   # launch chromiumoxide hide()+new_headless+UA Chrome/151 + user_data_dir ~/.cache/reddit-scrappe/profile
  login.rs     # login_flow() headed + ENTER, logout_flow() rm profile, profile_dir()
  reddit.rs    # search_human() goto search?sort=new&t=week + human_scroll + parse shreddit-post + is_captcha + fallback old.reddit
  human.rs     # sleep_jitter 800-2000ms, human_scroll 2-3 steps
  filter.rs    # filter_posts score/comments/age/nsfw/dedup
  config.rs    # serde_yaml config.yaml queries/filters/schedule_minutes
  notifier.rs  # console STDOUT (telegram desactivado)
tests/
  fixtures/reddit_search_sample.html  # golden shreddit-post + old.reddit thing
  e2e_browser.rs  # #[ignore] smoke example.com
  e2e_reddit.rs   # #[ignore] REDIDT_E2E=1 real reddit (fixture-based)
config.yaml    # queries 3 temáticas, filters min_score 2 max_age 48h, schedule 30
```

## Agentes / Roles

- **browser agent**: lanza 1 Browser reuse, 1 Page secuencial por query, limpia SingletonLock (`/tmp/chromiumoxide-runner` + `~/.cache/reddit-scrappe/profile`), stealth `hide()` + `--disable-blink-features=AutomationControlled` + `new_headless_mode` + UA real
- **human agent**: delays `800-4000ms` jitter, `human_scroll` wheel 500-700 x2-3, warm-up `goto reddit.com/` antes de search (plan)
- **reddit agent**: poll `shreddit-post` 15s (1s interval) + fallback `old.reddit` via `reqwest` si timeout/captcha, detecta `Prove your humanity|blocked by network security|cf-challenge|hcaptcha|recaptcha` -> fallback ligero sin hammer
- **filter agent**: `filter_posts` + `seen.json` dedup
- **notifier agent**: `notify_console` max 5/query STDOUT

## Comandos

```bash
# setup
brew install --cask google-chrome
cargo build

# login terminal dispara headed (1 vez)
cargo run -- --login
# -> logueate en ventana, ENTER -> guarda ~/.cache/reddit-scrappe/profile (11 cookies reddit)

# run
cargo run -- --once                        # headless new con perfil persistente
cargo run -- --loop                        # cada 30m + jitter ±60s (plan irregular 25-45m)
RUST_LOG=debug cargo run -- --once
HEADLESS=0 cargo run -- --once             # headed debug
cargo run -- --logout                      # rm profile + revoke: reddit.com/settings/account

# test
cargo test -- --nocapture                  # 8 unit passed (filter/config/parse/captcha/human/login)
cargo test -- --ignored --nocapture        # e2e (requiere Chrome)
REDIDT_E2E=1 cargo test e2e_reddit -- --ignored --nocapture

# util
ls ~/Library/Caches/reddit-scrappe/profile/Default/Cookies  # 11 cookies reddit tras login
cat /tmp/reddit_debug.html | grep -o "<title>.*</title>"     # debug blocked page
```

## Env

Ninguna obligatoria local. Opcionales: `RUST_LOG=debug` `HEADLESS=0` `CAPTCHA_WAIT_SECS=30` `CHROME_PATH`

## Anti-bloqueo (research 2026-08-15)

- Reddit WAF 4 capas: L1 ASN datacenter, L2 TLS JA3, L3 conductual (intervalos fijos, burst), L4 OAuth 100/min. `old.reddit/.json` anon `403` hoy.
- `blocked by network security` = CDN/IP reputación, auto 1-24h. Datacenter IP + `requests` JA3 bloquea en ms aunque rate 30m.
- Plan irregular humano: `25-45m random` + `shuffle queries` + `sleep 5-15s` entre queries + pausa noche `0-7am` + warm-up `goto /` + `stealth JS` `navigator.webdriver=undefined` (future) + `hide()+new_headless+UA` ya + perfil persistente login. `sort=new t=week` + `max_age 48h` óptimo para OrchardRun (index delay minutos-horas, ventana Hot 1-3h).

## Notas

- Python legacy `scraper.py`/`main.py` queda pero Rust es primary (bin `reddit-scrappe`)
- No commitear `profile/`, `cookies.json`, `seen.json` (`.gitignore`)
- VPS headless: `scp -r ~/Library/Caches/reddit-scrappe/profile user@vps:~/.cache/reddit-scrappe/profile` tras login local
- Revocar: `cargo run -- --logout` + `reddit.com/settings/account` change password

## Estado

- Login headed OK (11 cookies), `cargo build 0 warnings`, `cargo test 8 passed`
- `cargo run -- --once` post-login aún `timeout shreddit-post 15s` + html `theme-beta` (no reddit) + fallback `old.reddit 403` -> `0 posts` (bloqueo persistente headless detectado). Próximo: stealth JS + warm-up.
