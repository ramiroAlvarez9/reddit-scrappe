import yaml, json, os, time
from pathlib import Path
from dotenv import load_dotenv
from scraper import search_reddit, filter_posts
from notifier import notify_telegram, notify_webhook, notify_console

load_dotenv()
CONFIG = Path("config.yaml")
SEEN_FILE = Path("seen.json")

def load_seen():
    if SEEN_FILE.exists():
        return set(json.loads(SEEN_FILE.read_text()))
    return set()

def save_seen(seen):
    SEEN_FILE.write_text(json.dumps(list(seen)))

def run_once():
    cfg = yaml.safe_load(CONFIG.read_text())
    seen = load_seen()
    new_seen = set(seen)
    
    for q in cfg["queries"]:
        name = q["name"]
        print(f"\n[>] Buscando: {name} -> \"{q['q']}\" en {q['subreddits'] or 'todo reddit'}")
        posts = search_reddit(q["q"], q["subreddits"], q.get("sort","new"), q.get("limit", 20))
        filtered = filter_posts(posts, cfg["filters"])
        
        # solo nuevos
        fresh = [p for p in filtered if p["id"] not in seen]
        for p in fresh: new_seen.add(p["id"])
        
        if not fresh:
            notify_console([], name)
            continue
        
        # notifica
        notifier = cfg.get("notifier", "console")
        if notifier == "telegram":
            notify_telegram(fresh, name, os.getenv("TELEGRAM_BOT_TOKEN"), os.getenv("TELEGRAM_CHAT_ID"))
            notify_console(fresh, name) # siempre log en consola también
        elif notifier == "webhook":
            notify_webhook(fresh, name, os.getenv("WEBHOOK_URL"))
            notify_console(fresh, name)
        else:
            notify_console(fresh, name)
    
    save_seen(new_seen)
    print(f"\n[✓] Total vistos: {len(new_seen)} | Nuevos en esta corrida: {len(new_seen - seen)}")

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--loop", action="store_true", help="corre cada X minutos según config.yaml")
    parser.add_argument("--once", action="store_true", help="una sola corrida (default)")
    args = parser.parse_args()

    if args.loop:
        cfg = yaml.safe_load(CONFIG.read_text())
        interval = cfg.get("schedule_minutes", 30) * 60
        print(f"[loop] Corriendo cada {cfg.get('schedule_minutes')} min. Ctrl+C para parar.")
        while True:
            run_once()
            print(f"\n[...] durmiendo {interval/60:.0f} min...")
            time.sleep(interval)
    else:
        run_once()
