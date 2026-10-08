//! Politeness: requests to one site at least `spacing` apart, whatever the concurrency, and a
//! site that says 403 or 429 left alone for a while (Retry-After, else 2 min doubling to 2 h).

use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

pub const MIN_BACKOFF: Duration = Duration::from_secs(120);
pub const MAX_BACKOFF: Duration = Duration::from_secs(2 * 3600);

/// The site a URL belongs to: its last two labels, so `download1234.mediafire.com` and
/// `www.mediafire.com` are one site and share one pace.
pub fn site_of(url: &str) -> String {
    let host = url
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() <= 2 {
        return labels.join(".");
    }
    labels[labels.len() - 2..].join(".")
}

/// How long to stay away after the `strikes`-th refusal in a row (0 = first).
pub fn backoff(strikes: u32, retry_after: Option<Duration>) -> Duration {
    let doubled = MIN_BACKOFF.saturating_mul(1u32 << strikes.min(10));
    let base = doubled.min(MAX_BACKOFF);
    match retry_after {
        Some(r) if r > base => r.min(Duration::from_secs(24 * 3600)),
        _ => base,
    }
}

/// Retry-After in seconds (the HTTP-date form is rare enough to treat as absent).
pub fn retry_after(header: Option<&str>) -> Option<Duration> {
    header?.trim().parse::<u64>().ok().map(Duration::from_secs)
}

#[derive(Default)]
struct Site {
    next: Option<Instant>,
    blocked_until: Option<Instant>,
    strikes: u32,
}

pub struct Pacer {
    spacing: Duration,
    sites: Mutex<HashMap<String, Site>>,
}

impl Pacer {
    pub fn new(spacing: Duration) -> Self {
        Self {
            spacing,
            sites: Mutex::new(HashMap::new()),
        }
    }

    /// Wait for this site's turn. `Err` with the time left when the site is backing us off.
    pub async fn turn(&self, url: &str) -> Result<(), Duration> {
        let wait = {
            let mut sites = self.sites.lock().await;
            let s = sites.entry(site_of(url)).or_default();
            let now = Instant::now();
            if let Some(until) = s.blocked_until.filter(|u| *u > now) {
                return Err(until - now);
            }
            let at = s.next.filter(|n| *n > now).unwrap_or(now);
            s.next = Some(at + self.spacing);
            at - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        Ok(())
    }

    /// The site said 403/429: stay away, longer each time it keeps happening.
    pub async fn refused(&self, url: &str, retry_after: Option<Duration>) -> Duration {
        let mut sites = self.sites.lock().await;
        let s = sites.entry(site_of(url)).or_default();
        let wait = backoff(s.strikes, retry_after);
        s.strikes = s.strikes.saturating_add(1);
        s.blocked_until = Some(Instant::now() + wait);
        wait
    }

    /// The site answered normally: the next refusal starts from the bottom again.
    pub async fn answered(&self, url: &str) {
        let mut sites = self.sites.lock().await;
        if let Some(s) = sites.get_mut(&site_of(url)) {
            s.strikes = 0;
        }
    }

    /// Time left on a site's backoff, if any.
    pub async fn blocked(&self, url: &str) -> Option<Duration> {
        let sites = self.sites.lock().await;
        let now = Instant::now();
        sites
            .get(&site_of(url))
            .and_then(|s| s.blocked_until)
            .filter(|u| *u > now)
            .map(|u| u - now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_group_subdomains() {
        assert_eq!(
            site_of("https://download1234.mediafire.com/a/b.pkz"),
            "mediafire.com"
        );
        assert_eq!(site_of("https://www.mediafire.com/file/x"), "mediafire.com");
        assert_eq!(
            site_of("https://mxb-mods.com/careless-beta/"),
            "mxb-mods.com"
        );
        assert_eq!(
            site_of("https://user@Files.Example.net:8443/x"),
            "example.net"
        );
    }

    #[test]
    fn backoff_doubles_to_a_ceiling_and_honours_retry_after() {
        assert_eq!(backoff(0, None), Duration::from_secs(120));
        assert_eq!(backoff(1, None), Duration::from_secs(240));
        assert_eq!(backoff(20, None), MAX_BACKOFF);
        assert_eq!(
            backoff(0, Some(Duration::from_secs(900))),
            Duration::from_secs(900)
        );
        assert_eq!(
            backoff(0, Some(Duration::from_secs(5))),
            Duration::from_secs(120)
        );
        assert_eq!(retry_after(Some(" 30 ")), Some(Duration::from_secs(30)));
        assert_eq!(retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")), None);
    }

    #[tokio::test(start_paused = true)]
    async fn one_site_is_spaced_and_others_are_not() {
        let p = Pacer::new(Duration::from_secs(2));
        let t0 = Instant::now();
        p.turn("https://mxb-mods.com/a/").await.unwrap();
        p.turn("https://www.mediafire.com/x").await.unwrap();
        assert!(t0.elapsed() < Duration::from_millis(10));
        p.turn("https://mxb-mods.com/b/").await.unwrap();
        assert!(t0.elapsed() >= Duration::from_secs(2));
        p.turn("https://mxb-mods.com/c/").await.unwrap();
        assert!(t0.elapsed() >= Duration::from_secs(4));
    }

    #[tokio::test(start_paused = true)]
    async fn a_refusal_blocks_the_site_until_it_passes() {
        let p = Pacer::new(Duration::from_secs(2));
        let wait = p.refused("https://download9.mediafire.com/x", None).await;
        assert_eq!(wait, MIN_BACKOFF);
        assert!(p.turn("https://www.mediafire.com/y").await.is_err());
        assert!(p.turn("https://mxb-mods.com/").await.is_ok());
        assert_eq!(
            p.refused("https://www.mediafire.com/y", None).await,
            MIN_BACKOFF * 2
        );
        p.answered("https://www.mediafire.com/y").await;
        tokio::time::advance(MAX_BACKOFF).await;
        assert!(p.turn("https://www.mediafire.com/y").await.is_ok());
        assert_eq!(
            p.refused("https://www.mediafire.com/y", None).await,
            MIN_BACKOFF
        );
    }
}
