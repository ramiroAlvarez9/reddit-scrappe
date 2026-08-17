import requests
import time
import os
from datetime import datetime, timezone

# Reddit hoy (2024+) bloquea el .json sin auth -> necesitamos OAuth
# Gratis, 2 minutos crear app en https://www.reddit.com/prefs/apps

_token_cache = {"token": None, "expires": 0}

def get_oauth_token():
    cid = os.getenv("REDDIT_CLIENT_ID")
    secret = os.getenv("REDDIT_CLIENT_SECRET")
    if not cid or not secret:
        return None
    
    # si token aún válido, reusar
    if _token_cache["token"] and time.time() < _token_cache["expires"] - 60:
        return _token_cache["token"]

    auth = requests.auth.HTTPBasicAuth(cid, secret)
    headers = {"User-Agent": os.getenv("REDDIT_USER_AGENT", "orchardrun-scraper:v1.0 by u/test")}
    data = {"grant_type": "client_credentials"}
    r = requests.post("https://www.reddit.com/api/v1/access_token", auth=auth, data=data, headers=headers, timeout=15)
    r.raise_for_status()
    j = r.json()
    _token_cache["token"] = j["access_token"]
    _token_cache["expires"] = time.time() + j.get("expires_in", 3600)
    return _token_cache["token"]

def search_reddit(query: str, subreddits: list, sort="new", limit=20):
    token = get_oauth_token()
    if not token:
        print("  [!] Sin REDDIT_CLIENT_ID/SECRET -> Reddit bloquea requests anónimos desde 2024.")
        print("      Crea app en https://www.reddit.com/prefs/apps (tipo 'script') y pon los datos en .env")
        print("      Mientras tanto intento fallback sin auth (probablemente 403)...")
        return search_no_auth_fallback(query, subreddits, sort, limit)
    
    headers = {
        "Authorization": f"bearer {token}",
        "User-Agent": os.getenv("REDDIT_USER_AGENT", "orchardrun-scraper:v1.0 by u/test")
    }
    results = []
    targets = subreddits if subreddits else [None]
    
    for sub in targets:
        if sub:
            url = f"https://oauth.reddit.com/r/{sub}/search"
        else:
            url = "https://oauth.reddit.com/search"
        
        params = {"q": query, "sort": sort, "limit": limit, "t": "week", "restrict_sr": "on" if sub else "off", "raw_json": 1}
        try:
            r = requests.get(url, params=params, headers=headers, timeout=15)
            if r.status_code == 429:
                print(f"  [!] Rate limited r/{sub}, esperando 10s...")
                time.sleep(10)
                continue
            r.raise_for_status()
            data = r.json()
            for child in data.get("data", {}).get("children", []):
                d = child["data"]
                results.append({
                    "id": d["id"],
                    "title": d["title"],
                    "subreddit": d["subreddit"],
                    "author": d["author"],
                    "score": d["score"],
                    "num_comments": d["num_comments"],
                    "created_utc": d["created_utc"],
                    "url": "https://www.reddit.com" + d["permalink"],
                    "selftext": (d.get("selftext") or "")[:300],
                    "over_18": d.get("over_18", False),
                })
            time.sleep(1.5)
        except Exception as e:
            print(f"  [x] Error r/{sub}: {e}")
            if hasattr(e, 'response') and e.response is not None:
                print(e.response.text[:500])
    return results

def search_no_auth_fallback(query, subreddits, sort, limit):
    headers = {"User-Agent": "Mozilla/5.0 (orchardrun-scraper)"}
    results = []
    targets = subreddits if subreddits else [None]
    for sub in targets:
        url = f"https://www.reddit.com/r/{sub}/search.json" if sub else "https://www.reddit.com/search.json"
        params = {"q": query, "sort": sort, "limit": limit}
        try:
            r = requests.get(url, params=params, headers=headers, timeout=15)
            r.raise_for_status()
            for child in r.json().get("data", {}).get("children", []):
                d = child["data"]
                results.append({"id": d["id"], "title": d["title"], "subreddit": d["subreddit"], "author": d["author"], "score": d["score"], "num_comments": d["num_comments"], "created_utc": d["created_utc"], "url": "https://www.reddit.com"+d["permalink"], "selftext": (d.get("selftext") or "")[:300], "over_18": d.get("over_18", False)})
        except Exception as e:
            print(f"  [x] fallback r/{sub}: {e}")
    return results

def search_reddit_no_auth(*a, **kw): # compat
    return search_reddit(*a, **kw)

def filter_posts(posts, filters):
    now = datetime.now(timezone.utc).timestamp()
    max_age = filters.get("max_age_hours", 48) * 3600
    out = []
    for p in posts:
        age = now - p["created_utc"]
        if age > max_age: continue
        if p["score"] < filters.get("min_score", 0): continue
        if p["num_comments"] < filters.get("min_comments", 0): continue
        if filters.get("exclude_nsfw") and p["over_18"]: continue
        out.append(p)
    seen = set()
    uniq = []
    for p in out:
        if p["id"] not in seen:
            seen.add(p["id"])
            uniq.append(p)
    uniq.sort(key=lambda x: (x["score"] + x["num_comments"]*2), reverse=True)
    return uniq
