use std::time::Duration;

use rand::Rng;

#[derive(Debug, Clone)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
    pub current: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(30),
            current: Duration::from_millis(500),
        }
    }
}

impl Backoff {
    pub fn reset(&mut self) {
        self.current = self.initial;
    }

    pub fn next_delay(&mut self) -> Duration {
        let jitter_ms = rand::thread_rng().gen_range(0..250u64);
        let delay = self.current + Duration::from_millis(jitter_ms);
        self.current = (self.current * 2).min(self.max);
        delay
    }
}

pub fn redact_url(url: &str) -> String {
    if let Ok(mut parsed) = url::Url::parse(url) {
        if !parsed.username().is_empty() {
            let _ = parsed.set_username("***");
        }
        if parsed.password().is_some() {
            let _ = parsed.set_password(Some("***"));
        }
        let mut s = parsed.to_string();
        if let Some(pos) = s.find("/v2/") {
            let rest = &s[pos + 4..];
            if rest.len() > 8 {
                s = format!("{}{}", &s[..pos + 4], "***");
            }
        }
        return s;
    }
    let mut out = url.to_string();
    if let Some(idx) = out.find("/v2/") {
        out.replace_range(idx + 4.., "***");
    }
    out
}
