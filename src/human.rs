use rand::Rng;
use tokio::time::{sleep, Duration};

pub async fn sleep_jitter(min_ms: u64, max_ms: u64) {
    let mut rng = rand::thread_rng();
    let ms = rng.gen_range(min_ms..=max_ms);
    sleep(Duration::from_millis(ms)).await;
}

pub async fn human_scroll(page: &chromiumoxide::Page) -> anyhow::Result<()> {
    // scroll 600px 2-3 veces con jitter humano
    let mut rng = rand::thread_rng();
    let steps = rng.gen_range(2..=3);
    for i in 0..steps {
        let y = 500 + rng.gen_range(0..200);
        let js = format!("window.scrollBy(0, {})", y);
        let _ = page.evaluate(js).await;
        tracing::debug!("[human] scroll {}/{} y={} + sleep", i+1, steps, y);
        sleep_jitter(800, 2000).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn jitter_in_range() {
        let start = std::time::Instant::now();
        sleep_jitter(10, 20).await;
        assert!(start.elapsed().as_millis() >= 10);
    }
}
