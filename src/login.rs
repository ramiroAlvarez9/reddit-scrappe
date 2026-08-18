use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use std::io::{self, Write};
use std::path::PathBuf;

pub fn profile_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("reddit-scrappe")
        .join("profile")
}

pub async fn login_flow() -> anyhow::Result<()> {
    let dir = profile_dir();
    std::fs::create_dir_all(&dir)?;
    tracing::info!("[login] perfil persistente: {}", dir.display());
    tracing::info!("[login] abriendo Chrome headed en https://www.reddit.com/login");
    tracing::info!("[login] logueate manualmente en la ventana (captcha/2FA si pide)");

    let mut builder = BrowserConfig::builder()
        .chrome_executable(crate::browser::detect_chrome())
        .user_data_dir(&dir)
        .with_head()
        .arg("--no-sandbox")
        .arg("--disable-dev-shm-usage")
        .arg("--window-size=1280,720")
        .arg("--lang=en-US,en");
    if let Some(p) = crate::browser::proxy_server_arg() {
        builder = builder.arg(format!("--proxy-server={}", p)).arg("--proxy-bypass-list=<-loopback>");
        tracing::info!("[proxy] login headed via {}", "gw.dataimpulse.com");
    }
    let config = builder.build().map_err(|e| anyhow::anyhow!(e))?;

    let (mut browser, mut handler) = Browser::launch(config).await?;
    let handle = tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if let Err(e) = h {
                tracing::warn!("[login] handler err: {:?}", e);
                break;
            }
        }
    });

    let _page = browser.new_page("https://www.reddit.com/login").await?;
    tracing::info!("[login] ventana abierta - completa login humano");
    println!("\n[login] >>> Logueate en la ventana de Chrome que se abrió <<<");
    println!("[login] Cuando veas tu feed logueado (avatar arriba), presiona ENTER aquí para guardar sesión...");
    print!("> ");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;

    // verify by checking cookies
    let cookies = browser.get_cookies().await.unwrap_or_default();
    let reddit_cookies: Vec<_> = cookies.iter().filter(|c| c.domain.contains("reddit")).collect();
    tracing::info!("[login] {} cookies totales, {} de reddit.com", cookies.len(), reddit_cookies.len());
    if reddit_cookies.is_empty() {
        tracing::warn!("[login] no se detectaron cookies de reddit - ¿logueste bien? Igual guardo perfil");
    } else {
        tracing::info!("[login] sesión parece logueada ({} cookies reddit)", reddit_cookies.len());
    }

    // save cookies json (decrypted via CDP) for --no-browser reuse
    let cookies_json = serde_json::to_value(&cookies).unwrap_or(serde_json::Value::Null);
    if let Err(e) = crate::cookies::save_cookies(&cookies_json) {
        tracing::warn!("[login] failed to save cookies via cookies module: {:?}", e);
        // fallback direct write
        let cookies_path = dir.join("cookies.json");
        let _ = std::fs::write(&cookies_path, serde_json::to_string_pretty(&cookies)?);
        tracing::info!("[login] cookies guardadas (fallback) en {}", cookies_path.display());
    }
    tracing::info!("[login] perfil guardado en {} - ahora `cargo run -- --once` usará sesión sin captcha", dir.display());

    browser.close().await?;
    handle.abort();
    Ok(())
}

pub fn logout_flow() -> anyhow::Result<()> {
    let dir = profile_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
        tracing::info!("[logout] perfil borrado: {}", dir.display());
    } else {
        tracing::info!("[logout] no había perfil en {}", dir.display());
    }
    let cookies_json = dir.join("cookies.json");
    let _ = std::fs::remove_file(cookies_json);
    println!("[logout] Para revocar en Reddit: https://www.reddit.com/prefs/apps o cambia password en https://www.reddit.com/settings/account -> 'Change password' invalida todas las sesiones");
    println!("[logout] En VPS también corre: rm -rf ~/.cache/reddit-scrappe/profile");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_dir_exists() {
        let dir = profile_dir();
        assert!(dir.to_string_lossy().contains("reddit-scrappe"));
    }
}
