use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;

const PROXY_HOST: &str = "gw.dataimpulse.com";
const PROXY_PORT_HTTP: u16 = 823;
const PROXY_PORT_SOCKS: u16 = 824;

pub struct BrowserHandle {
    pub browser: Browser,
    _handler: tokio::task::JoinHandle<()>,
}

/// Build DataImpulse proxy username with optional targeting (same syntax as dataimpulse-mcp)
fn build_proxy_user(base: &str, country: Option<&str>, city: Option<&str>, session: Option<&str>) -> Result<String, String> {
    let mut params: Vec<String> = Vec::new();
    if let Some(c) = country {
        let c = c.trim().to_lowercase();
        if c.len() != 2 {
            return Err(format!("country must be ISO2, got '{}'", c));
        }
        params.push(format!("cr.{}", c));
    }
    if let Some(city) = city {
        let city = city.trim();
        if city.is_empty() { return Err("city cannot be empty".to_string()); }
        if country.is_none() { return Err("city requires country".to_string()); }
        params.push(format!("city.{}", city.to_lowercase()));
    }
    if let Some(sess) = session {
        let sess = sess.trim().replace(' ', "_");
        if sess.is_empty() { return Err("session cannot be empty".to_string()); }
        params.push(format!("sessid.{}", sess));
    }
    if params.is_empty() { Ok(base.to_string()) } else { Ok(format!("{}__{}", base, params.join(";"))) }
}

pub fn proxy_server_arg() -> Option<String> {
    let user = std::env::var("DI_USER").ok()?;
    let pass = std::env::var("DI_PASS").ok()?;
    if user.trim().is_empty() || pass.trim().is_empty() { return None; }
    let country = std::env::var("DI_COUNTRY").ok();
    let city = std::env::var("DI_CITY").ok();
    let session = std::env::var("DI_SESSION").ok();
    let use_socks = std::env::var("DI_USE_SOCKS").map(|v| v=="1" || v.to_lowercase()=="true").unwrap_or(false);
    let scheme = if use_socks { "socks5" } else { "http" };
    let port = if use_socks { PROXY_PORT_SOCKS } else { PROXY_PORT_HTTP };
    let proxy_user = match build_proxy_user(&user, country.as_deref(), city.as_deref(), session.as_deref()) {
        Ok(u) => u,
        Err(e) => { tracing::warn!("[proxy] build_proxy_user error: {}", e); return None; }
    };
    // encode password, encode user only for @ : space %
    let mut encoded_user = proxy_user.clone();
    if proxy_user.contains('@') || proxy_user.contains(':') || proxy_user.contains(' ') || proxy_user.contains('%') {
        encoded_user = proxy_user.replace('%', "%25").replace('@', "%40").replace(':', "%3A").replace(' ', "%20");
    }
    // simple percent-encode password
    let encoded_pass = {
        let mut out = String::new();
        for b in pass.bytes() {
            let c = b as char;
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') { out.push(c); }
            else { out.push_str(&format!("%{:02X}", b)); }
        }
        out
    };
    let proxy_url = format!("{}://{}:{}@{}:{}", scheme, encoded_user, encoded_pass, PROXY_HOST, port);
    let country_log = country.as_deref().unwrap_or("-");
    let city_log = city.as_deref().unwrap_or("-");
    let session_log = session.as_deref().unwrap_or("-");
    tracing::info!("[proxy] enabled via {}:{} country={} city={} session={} scheme={} (creds hidden)", PROXY_HOST, port, country_log, city_log, session_log, scheme);
    Some(proxy_url)
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
    let proxy_url = proxy_server_arg();
    if proxy_url.is_none() {
        tracing::info!("[proxy] disabled (set DI_USER/DI_PASS to enable residential proxy)");
    }
    let final_config = if use_headed {
        let mut b = BrowserConfig::builder()
            .chrome_executable(detect_chrome())
            .user_data_dir(&tmp_dir)
            .with_head()
            .hide()
            .arg("--no-sandbox")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--window-size=1280,720")
            .arg("--lang=en-US,en")
            .arg("--user-agent=Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36");
        if let Some(ref p) = proxy_url {
            b = b.arg(format!("--proxy-server={}", p)).arg("--proxy-bypass-list=<-loopback>");
            tracing::info!("[proxy] headed browser will use proxy {}", PROXY_HOST);
        }
        b.build().unwrap()
    } else {
        let mut b = BrowserConfig::builder()
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
            .arg("--user-agent=Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36");
        if let Some(ref p) = proxy_url {
            b = b.arg(format!("--proxy-server={}", p)).arg("--proxy-bypass-list=<-loopback>");
            tracing::info!("[proxy] headless browser will use proxy {}", PROXY_HOST);
        }
        b.build().unwrap()
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
