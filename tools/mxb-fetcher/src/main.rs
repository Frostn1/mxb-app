//! mxb-fetcher: the mod mirror's hands on a normal machine.
//!
//! mxb-mods.com and MediaFire answer Cloudflare Workers 403, and mxb-mods.com answers any
//! datacenter address 403 as well. This runs on our own Linux box (MediaFire and the rest) and on
//! a home connection, Windows or Linux (mxb-mods.com), opens no ports, and pulls work from the
//! control plane (`/v1/mirror/fetcher/*`): it leases
//! a couple of jobs, fetches each politely (a browser's User-Agent, at least 2 s between
//! requests to one site, at most 2 jobs at once, backing a site off on 403/429), hands page HTML
//! back to be parsed, uploads files to R2 through the control plane in parts, and reports.
//!
//! Configuration, from the environment or a `KEY=VALUE` file named by the first argument
//! (or `MXB_FETCHER_CONFIG`):
//!   MXB_FETCHER_API     the control plane, e.g. https://api.mxbsecure.com
//!   MXB_FETCHER_TOKEN   the control plane's MIRROR_FETCHER_TOKEN
//!   MXB_FETCHER_HOSTS   which hosts this fetcher serves: names, `*`, `-name` (default: all).
//!                       A datacenter box: `* -mxb-mods.com`. A home machine: `mxb-mods.com`.
//!   MXB_FETCHER_TMP     where files are spooled while hashed (default: the system temp dir)
//!   MXB_FETCHER_IDLE    seconds to wait when there is no work (default 30)

mod api;
mod config;
mod jobs;
mod mediafire;
mod pacing;

use anyhow::{Context, Result};
use jobs::{Ctx, Outcome, BROWSER_UA};
use std::time::Duration;

/// Jobs in flight at once.
const CONCURRENCY: usize = 2;
/// Requests to one site at least this far apart.
const SITE_SPACING: Duration = Duration::from_secs(2);

fn build() -> Result<(Ctx, Duration, Vec<String>)> {
    let cfg = config::Config::load()?;
    let env = |name: &str| cfg.get(name);
    let base = api::require(env("MXB_FETCHER_API"), "MXB_FETCHER_API")?;
    let token = api::require(env("MXB_FETCHER_TOKEN"), "MXB_FETCHER_TOKEN")?;
    let hosts = config::hosts(env("MXB_FETCHER_HOSTS").as_deref());
    let tmp = env("MXB_FETCHER_TMP")
        .map(Into::into)
        .unwrap_or_else(|| std::env::temp_dir().join("mxb-fetcher"));
    std::fs::create_dir_all(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
    let idle = Duration::from_secs(
        env("MXB_FETCHER_IDLE")
            .and_then(|s| s.parse().ok())
            .unwrap_or(30),
    );

    let web = reqwest::Client::builder()
        .user_agent(BROWSER_UA)
        .cookie_store(true)
        .gzip(true)
        .brotli(true)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(120))
        .build()?;
    let files = reqwest::Client::builder()
        .user_agent(BROWSER_UA)
        .cookie_store(true)
        .no_gzip()
        .no_brotli()
        .connect_timeout(Duration::from_secs(20))
        // A body may take a long time; a stalled one may not.
        .read_timeout(Duration::from_secs(120))
        .build()?;
    let ctx = Ctx {
        api: api::Api::new(&base, &token)?,
        web,
        files,
        pacer: pacing::Pacer::new(SITE_SPACING),
        tmp,
    };
    Ok((ctx, idle, hosts))
}

fn log(job: &api::Job, outcome: &Outcome) {
    let (state, note) = match outcome {
        Outcome::Done(n) => ("done", n.clone()),
        Outcome::Failed(n) => ("failed", n.clone()),
        Outcome::Deferred(d) => ("deferred", format!("{}s", d.as_secs())),
    };
    println!(
        "{}",
        serde_json::json!({ "job": job.id, "kind": job.kind, "url": job.url, "state": state, "note": note })
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    let (ctx, idle, hosts) = build()?;
    println!(
        "{}",
        serde_json::json!({ "msg": "mxb-fetcher started", "version": env!("CARGO_PKG_VERSION"), "hosts": hosts })
    );
    // No shutdown handling to speak of: a job cut off by a stop is leased out again once its
    // lease runs out, and nothing on the box is left half-written but a temp file.
    loop {
        let jobs = match ctx.api.lease(CONCURRENCY, &hosts).await {
            Ok(j) => j,
            Err(e) => {
                eprintln!(
                    "{}",
                    serde_json::json!({ "msg": "lease failed", "error": format!("{e:#}") })
                );
                tokio::time::sleep(idle).await;
                continue;
            }
        };
        if jobs.is_empty() {
            tokio::time::sleep(idle).await;
            continue;
        }
        // At most CONCURRENCY at once: the lease never hands out more than that.
        let outcomes = futures_util::future::join_all(jobs.iter().map(|j| ctx.run(j))).await;
        let mut pause = Duration::ZERO;
        for (job, outcome) in jobs.iter().zip(&outcomes) {
            log(job, outcome);
            if let Outcome::Deferred(d) = outcome {
                pause = pause.max((*d).min(Duration::from_secs(60)));
            }
        }
        // Everything handed back because a site is backing us off: don't spin on leases.
        if outcomes.iter().all(|o| matches!(o, Outcome::Deferred(_))) {
            tokio::time::sleep(pause.max(Duration::from_secs(5))).await;
        }
    }
}
