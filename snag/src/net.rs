//! Snag's own requests: thumbnails, update checks, and downloading yt-dlp,
//! Deno and new versions of Snag. (Downloads of videos are yt-dlp's, and it
//! is handed the same proxy on its command line.)
//!
//! Two things every one of them needs, which is why they all come from here:
//!
//! - The proxy from settings > network. Someone who set one expects all of
//!   Snag's traffic to use it, and the privacy policy says so. When it is set
//!   but cannot be used, the request fails rather than quietly going direct.
//! - A time limit that judges whether a transfer is stuck, not how long it
//!   takes. A limit on the whole request failed a 45 MB Deno download on any
//!   connection slower than about 6 Mbit/s; a limit per read only fails one
//!   that has actually stopped.

use std::sync::RwLock;
use std::time::Duration;

/// The proxy from settings, kept here so the threads that make requests do
/// not each need the settings handed to them. Empty means none.
static PROXY: RwLock<String> = RwLock::new(String::new());

/// How long connecting may take. Generous: a slow network or a VPN can take
/// most of 15 seconds just to connect.
const CONNECT: Duration = Duration::from_secs(30);

/// Use `proxy` for every request from now on. Called when settings are read
/// and whenever they are saved.
pub fn set_proxy(proxy: &str) {
    if let Ok(mut current) = PROXY.write() {
        let proxy = proxy.trim();
        if *current != proxy {
            *current = proxy.to_string();
        }
    }
}

/// A client for one request, failing only when the other end goes quiet for
/// `idle`. Errors when a proxy is set that cannot be used.
pub fn agent(idle: Duration) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new()
        .user_agent("Snag")
        .timeout_connect(CONNECT)
        .timeout_read(idle)
        .timeout_write(idle);
    let proxy = PROXY.read().map(|p| p.clone()).unwrap_or_default();
    if !proxy.is_empty() {
        let proxy = ureq::Proxy::new(&proxy)
            .map_err(|e| format!("the proxy in settings > network is not usable ({e})"))?;
        builder = builder.proxy(proxy);
    }
    Ok(builder.build())
}

/// GET `url`, for a small reply: an API answer, a checksum file, a thumbnail.
pub fn get(url: &str) -> Result<ureq::Response, String> {
    agent(Duration::from_secs(30))?
        .get(url)
        .call()
        .map_err(|e| e.to_string())
}

/// GET `url` for a download, which may take as long as it likes so long as
/// data keeps arriving.
pub fn download(url: &str) -> Result<ureq::Response, String> {
    agent(Duration::from_secs(60))?
        .get(url)
        .call()
        .map_err(|e| e.to_string())
}

/// The size a download says it is, if it says, capped so that a wrong or
/// hostile header cannot make Snag reserve memory it does not need.
pub fn declared_length(resp: &ureq::Response) -> u64 {
    resp.header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// The most worth reserving up front for a download held in memory.
pub const MAX_RESERVE: u64 = 256 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, not two: the proxy is process-wide, and tests run in
    /// parallel.
    #[test]
    fn a_proxy_in_settings_is_used_and_never_skipped() {
        // Port 9 on this machine refuses connections: if the request went
        // through the proxy it fails; if it went direct it would reach github.
        set_proxy("http://127.0.0.1:9");
        assert!(get("https://api.github.com/").is_err());

        // ureq takes even nonsense as a proxy host. What matters is that the
        // request then fails, rather than going around it.
        set_proxy("not a proxy at all");
        assert!(get("https://api.github.com/").is_err());

        set_proxy("");
        assert!(agent(Duration::from_secs(1)).is_ok());
    }
}
