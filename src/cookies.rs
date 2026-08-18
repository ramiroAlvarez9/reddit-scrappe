use std::path::PathBuf;

/// Path where decrypted cookies are persisted (written by --login via CDP get_cookies).
/// This file contains plain-text values (unencrypted), unlike the sqlite Cookies DB.
pub fn cookies_file_path() -> PathBuf {
    crate::login::profile_dir().join("cookies.json")
}

/// Save cookies obtained via `Browser::get_cookies()` (CDP) to disk.
/// The input type is the raw chromiumoxide Cookie; we serialize via serde_json Value
/// to avoid tight coupling to the chromiumoxide type at deserialization.
pub fn save_cookies(cookies: &serde_json::Value) -> anyhow::Result<()> {
    let path = cookies_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(cookies)?)?;
    tracing::info!("[cookies] saved {} bytes to {}", std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0), path.display());
    Ok(())
}

/// Load reddit cookie header from the decrypted JSON file.
/// Returns `None` if file missing, empty, or no valid reddit cookies.
/// Filters out expired cookies (expires > 0 && expires < now).
pub fn load_cookie_header() -> Option<String> {
    let path = cookies_file_path();
    if !path.exists() {
        tracing::debug!("[cookies] no cookies file at {}", path.display());
        return None;
    }
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("[cookies] failed to read {}: {:?}", path.display(), e);
            return None;
        }
    };
    let v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("[cookies] invalid json {}: {:?}", path.display(), e);
            return None;
        }
    };
    // Expect array of cookie objects. Support both array root and object with array (defensive).
    let arr = if let Some(a) = v.as_array() {
        a
    } else if let Some(a) = v.get("cookies").and_then(|c| c.as_array()) {
        a
    } else {
        tracing::warn!("[cookies] unexpected json shape, expected array");
        return None;
    };
    let now = chrono::Utc::now().timestamp() as f64;
    let mut parts: Vec<String> = Vec::new();
    let mut expired = 0usize;
    let mut total_reddit = 0usize;
    for c in arr {
        let domain = c.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        if !domain.contains("reddit") {
            continue;
        }
        total_reddit += 1;
        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let value = c.get("value").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() || value.is_empty() {
            continue;
        }
        // expires can be f64 or i64; treat missing as session (-1)
        let expires = c
            .get("expires")
            .or_else(|| c.get("expiry"))
            .or_else(|| c.get("expirationDate"))
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
            .unwrap_or(-1.0);
        if expires > 0.0 && expires < now {
            expired += 1;
            tracing::debug!("[cookies] skipping expired {} (expires {} < now {})", name, expires, now);
            continue;
        }
        parts.push(format!("{}={}", name, value));
    }
    if expired > 0 {
        tracing::info!("[cookies] filtered {} expired reddit cookies ({} total reddit in file)", expired, total_reddit);
    }
    if parts.is_empty() {
        if total_reddit == 0 {
            tracing::warn!("[cookies] file exists but no reddit cookies found — run `cargo run -- --login` with DI_COUNTRY/DI_SESSION");
        } else if expired == total_reddit {
            tracing::warn!("[cookies] all {} reddit cookies expired — run `cargo run -- --login` to refresh", total_reddit);
        } else {
            tracing::warn!("[cookies] no valid reddit cookies (empty values?) — re-login required");
        }
        return None;
    }
    let header = parts.join("; ");
    tracing::info!("[cookies] loaded {} reddit cookies ({} bytes) from {}", parts.len(), header.len(), path.display());
    Some(header)
}

/// Check if the cookies file exists and contains at least one non-expired reddit cookie.
/// Used to give actionable error messages for --no-browser.
pub fn has_valid_cookies() -> bool {
    load_cookie_header().is_some()
}

/// Return human-readable status for diagnostics.
pub fn cookies_status() -> String {
    let path = cookies_file_path();
    if !path.exists() {
        return format!("missing {}", path.display());
    }
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            let v: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(j) => j,
                Err(e) => return format!("invalid json {}: {}", path.display(), e),
            };
            let arr = v.as_array().or_else(|| v.get("cookies").and_then(|c| c.as_array()));
            match arr {
                Some(a) => {
                    let now = chrono::Utc::now().timestamp() as f64;
                    let mut total = 0;
                    let mut valid = 0;
                    let mut expired = 0;
                    for c in a {
                        let domain = c.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                        if !domain.contains("reddit") { continue; }
                        total += 1;
                        let expires = c.get("expires").and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))).unwrap_or(-1.0);
                        if expires > 0.0 && expires < now { expired += 1; } else {
                            let value = c.get("value").and_then(|v| v.as_str()).unwrap_or("");
                            if !value.is_empty() { valid += 1; }
                        }
                    }
                    format!("{} total reddit, {} valid, {} expired at {}", total, valid, expired, path.display())
                }
                None => format!("unexpected shape at {}", path.display()),
            }
        }
        Err(e) => format!("read error {}: {:?}", path.display(), e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_contains_cookies_json() {
        let p = cookies_file_path();
        assert!(p.to_string_lossy().ends_with("cookies.json"));
        assert!(p.to_string_lossy().contains("reddit-scrappe"));
    }
}
