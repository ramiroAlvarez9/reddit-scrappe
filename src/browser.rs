use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;

pub struct BrowserHandle {
    pub browser: Browser,
    _handler: tokio::task::JoinHandle<()>,
}

pub async fn launch_browser() -> anyhow::Result<BrowserHandle> {
    tracing::info!("[browser] launching chromium headless...");
    let chrome = detect_chrome();
    if chrome.is_empty() {
        tracing::warn!("[browser] no system Chrome found at common paths. Install Chrome via 'brew install --cask google-chrome' or enable fetcher feature.");
        tracing::warn!("[browser] attempting launch anyway (chromiumoxide will try to find Chrome in PATH)");
    }
    // clean stale Singleton locks from previous crash (both tmp and persistent profile)
    for base in [std::env::temp_dir().join("chromiumoxide-runner"), persistent_profile_dir()] {
        let _ = std::fs::remove_file(base.join("SingletonLock"));
        let _ = std::fs::remove_file(base.join("SingletonSocket"));
        let _ = std::fs::remove_file(base.join("SingletonCookie"));
    }
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("chromiumoxide-runner"));
    let use_headed = std::env::var("HEADLESS").map(|v| v=="0").unwrap_or(false);
    // use persistent profile so login session survives (fallback to tmp if cache not writable)
    let tmp_dir = persistent_profile_dir();
    let _ = std::fs::create_dir_all(&tmp_dir);
    // if persistent dir creation fails, fallback to tmp per pid
    let tmp_dir = if tmp_dir.exists() { tmp_dir } else {
        let fallback = std::env::temp_dir().join(format!("reddit-scrappe-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&fallback);
        fallback
    };
    let final_config = if use_headed {
        BrowserConfig::builder()
            .chrome_executable(detect_chrome())
            .user_data_dir(&tmp_dir)
            .with_head()
            .hide()
            .arg("--no-sandbox")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--window-size=1280,720")
            .arg("--lang=en-US,en")
            .arg("--user-agent=Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36")
            .build().unwrap()
    } else {
        BrowserConfig::builder()
            .chrome_executable(detect_chrome())
            .user_data_dir(&tmp_dir)
            .new_headless_mode()
            .hide()
            .arg("--no-sandbox")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--window-size=1280,720")
            .arg("--disable-gpu")
            .arg("--lang=en-US,en")
            .arg("--user-agent=Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36")
            .build().unwrap()
    };

    let (browser, mut handler) = Browser::launch(final_config).await?;
    let handle = tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if let Err(e) = h {
                tracing::warn!("[browser] handler error: {:?}", e);
                break;
            }
        }
    });
    tracing::info!("[browser] chromium launched pid(headed={})", use_headed);
    Ok(BrowserHandle { browser, _handler: handle })
}

pub fn detect_chrome() -> String {
    // try common paths, else let chromiumoxide auto-detect via empty string -> will use fetcher
    for p in ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/usr/bin/chromium", "/usr/bin/google-chrome", "/Applications/Chromium.app/Contents/MacOS/Chromium"] {
        if std::path::Path::new(p).exists() {
            tracing::info!("[browser] found chrome at {}", p);
            return p.to_string();
        }
    }
    // return empty to trigger auto-detect/fetcher
    "".into()
}

pub fn persistent_profile_dir() -> std::path::PathBuf {
    crate::login::profile_dir()
}
