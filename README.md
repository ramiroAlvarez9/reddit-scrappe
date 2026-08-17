# reddit-scrappe (Rust, humano anónimo, STDOUT)

Scraper que simula humano anónimo buscando en Reddit para encontrar posts donde hablar de OrchardRun. Sin API oficial - usa `chromiumoxide 0.9` headless secuencial. Solo STDOUT local por ahora.

## Requisitos

- Rust 1.97+ (`rustup`)
- Chrome/Chromium: `brew install --cask google-chrome` (Mac) o `apt install chromium` (Linux)

## Config

Edita `config.yaml` (queries, filtros, `schedule_minutes`):

```yaml
queries:
  - name: "orchard ai infra"
    q: "AI infrastructure OR MLOps"
    subreddits: ["MachineLearning", "LocalLLaMA"]
    sort: "new"
filters:
  min_score: 2
  max_age_hours: 48
schedule_minutes: 30
```

## Uso local (solo STDOUT)

```bash
# 1. Login una vez desde terminal (dispara Chrome headed) - guarda sesión en ~/.cache/reddit-scrappe/profile
cargo run -- --login
# -> logueate en la ventana, presiona ENTER en terminal cuando veas feed

cargo run -- --once              # una corrida secuencial con sesión persistente (headless new, sin captcha)
cargo run -- --loop              # cada 30m + jitter ±60s
RUST_LOG=debug cargo run -- --once  # traza scroll/extract
HEADLESS=0 cargo run -- --once   # con ventana Chrome visible (debug)
```

STDOUT ejemplo:
```
[INFO] [browser] launching chromium headless...
[INFO] [query:orchard ai infra] goto https://www.reddit.com/search/?q=AI+infrastructure...
[INFO] [extract] shreddit-post found
[INFO] [extract] found 12 raw posts for r/LocalLLaMA
[INFO] [query:orchard ai infra] 3 after filter
[INFO] [notify:orchard ai infra] 3 nuevos:
  >> r/LocalLLaMA | 120↑ 45💬 | Vector DB discussion
     https://www.reddit.com/r/LocalLLaMA/comments/...
```

- Captcha: `WARN [captcha] detected! waiting 10m before retry` + retry 1 vez, luego skip.

## Env vars (ninguna obligatoria local)

- `RUST_LOG=debug|info` (default `info`)
- `HEADLESS=0` para debug visible
- `CHROME_PATH` auto-detecta (`/Applications/Google Chrome.app/...`)

## Tests

```bash
cargo test -- --nocapture              # unit 6 tests (filter/config/parse/human, sin browser)
cargo test -- --ignored --nocapture    # e2e ignored (requiere Chrome)
REDIDT_E2E=1 cargo test e2e_reddit -- --ignored --nocapture  # hit real Reddit (fixture-based por ahora)
```

E2E:
- `tests/e2e_browser.rs` smoke example.com
- `tests/e2e_reddit.rs` fixture `tests/fixtures/reddit_search_sample.html` con `shreddit-post` (golden parse)
- Captcha detector unit en `reddit::tests::captcha_detect`

## Estructura

```
src/main.rs      # CLI --once/--loop, loop secuencial, seen.json
src/browser.rs   # launch chromiumoxide stealth anon
src/reddit.rs    # search_human + parse shreddit-post + is_captcha
src/human.rs     # sleep_jitter + human_scroll
src/filter.rs    # filter_posts (score/age/nsfw/dedup)
src/config.rs    # serde_yaml config.yaml
src/notifier.rs  # console STDOUT
```

VPS después: mismo binario `cargo build --release` (~15MB) + `chromium` (~150MB) total ~180MB vs Python playwright ~500MB.
