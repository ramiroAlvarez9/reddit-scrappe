import requests
import os

def notify_telegram(posts, query_name, token, chat_id):
    if not token or not chat_id:
        print("[!] Telegram no configurado")
        return
    for p in posts[:5]: # máx 5 por query para no spamear
        text = f"""🔍 *{query_name}* | r/{p['subreddit']}

*{p['title']}*
👍 {p['score']} | 💬 {p['num_comments']} | u/{p['author']}

{p['selftext'][:200]}...

🔗 {p['url']}"""
        requests.post(f"https://api.telegram.org/bot{token}/sendMessage", json={
            "chat_id": chat_id, "text": text, "parse_mode": "Markdown", "disable_web_page_preview": False
        })

def notify_webhook(posts, query_name, url):
    if not url: return
    for p in posts[:5]:
        requests.post(url, json={"content": f"**{query_name}** r/{p['subreddit']}: {p['title']}\n{p['url']}", "username": "OrchardRun Scraper"})

def notify_console(posts, query_name):
    if not posts:
        print(f"  - {query_name}: sin resultados nuevos")
        return
    print(f"\n  >> {query_name} ({len(posts)} nuevos):")
    for p in posts[:5]:
        print(f"     r/{p['subreddit']} | {p['score']}↑ {p['num_comments']}💬 | {p['title']}")
        print(f"     {p['url']}\n")
