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

thread_local! {
    /// A proxy this thread uses in place of the one from settings: the
    /// system's, once going direct has failed. See [`through`].
    static OVERRIDE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// The proxy from settings, empty when none is set.
pub fn settings_proxy() -> String {
    PROXY.read().map(|p| p.clone()).unwrap_or_default()
}

/// Run `f` with every request this thread makes going through `proxy`.
pub fn through<T>(proxy: &str, f: impl FnOnce() -> T) -> T {
    OVERRIDE.with(|o| *o.borrow_mut() = Some(proxy.to_string()));
    let out = f();
    OVERRIDE.with(|o| *o.borrow_mut() = None);
    out
}

/// The proxies this machine is set to use, best guess first, for when Snag's
/// own direct connection is blocked: a workplace or school network often lets
/// nothing out except through its proxy, which the browser knows about and
/// Snag, with nothing in its settings, does not.
///
/// The environment (HTTPS_PROXY and friends) and, on Windows, the proxy set in
/// Settings > Network > Proxy. A setup script (PAC) is not read: running it
/// needs a JavaScript engine.
pub fn system_proxies() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for var in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(v) = std::env::var(var) {
            push_proxy(&mut found, &v);
        }
    }
    #[cfg(windows)]
    if let Some(server) = windows_proxy() {
        for p in parse_proxy_server(&server) {
            push_proxy(&mut found, &p);
        }
    }
    found
}

fn push_proxy(found: &mut Vec<String>, p: &str) {
    let p = p.trim();
    if !p.is_empty() && !found.iter().any(|f| f == p) {
        found.push(p.to_string());
    }
}

/// Windows' `ProxyServer` value: either one `host:port` for everything, or
/// `scheme=host:port` pairs split by semicolons. https first, since every
/// request Snag makes is https; then http; then socks.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_proxy_server(value: &str) -> Vec<String> {
    let value = value.trim();
    if value.is_empty() {
        return Vec::new();
    }
    if !value.contains('=') {
        let only = value.split(';').next().unwrap_or(value).trim();
        return vec![with_scheme(only, "http")];
    }
    let mut out = Vec::new();
    for want in ["https", "http", "socks"] {
        for part in value.split(';') {
            if let Some((scheme, addr)) = part.split_once('=') {
                if scheme.trim().eq_ignore_ascii_case(want) && !addr.trim().is_empty() {
                    // An https= proxy is still spoken to in plain http (it is
                    // the target that is https); socks= means socks4/5.
                    let speak = if want == "socks" { "socks5" } else { "http" };
                    out.push(with_scheme(addr.trim(), speak));
                }
            }
        }
    }
    out
}

#[cfg_attr(not(windows), allow(dead_code))]
fn with_scheme(addr: &str, scheme: &str) -> String {
    if addr.contains("://") {
        addr.to_string()
    } else {
        format!("{scheme}://{addr}")
    }
}

/// The proxy set in Windows' settings, when it is switched on.
#[cfg(windows)]
fn windows_proxy() -> Option<String> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            key: isize,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut core::ffi::c_void,
            len: *mut u32,
        ) -> i32;
    }
    const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
    const RRF_RT_REG_DWORD: u32 = 0x10;
    const RRF_RT_REG_SZ: u32 = 0x02;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings");

    let mut enabled: u32 = 0;
    let mut len = 4u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            wide("ProxyEnable").as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut enabled as *mut u32).cast(),
            &mut len,
        )
    };
    if ok != 0 || enabled == 0 {
        return None;
    }

    let mut buf = vec![0u16; 2048];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            wide("ProxyServer").as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if ok != 0 {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..chars]))
}

/// A client for one request, failing only when the other end goes quiet for
/// `idle`. Errors when a proxy is set that cannot be used.
pub fn agent(idle: Duration) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new()
        .user_agent("Snag")
        .timeout_connect(CONNECT)
        .timeout_read(idle)
        .timeout_write(idle);
    let proxy = OVERRIDE
        .with(|o| o.borrow().clone())
        .unwrap_or_else(settings_proxy);
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

        // An override is used in place of settings, and only inside it.
        through("http://127.0.0.1:9", || {
            assert!(get("https://api.github.com/").is_err());
        });
        assert!(OVERRIDE.with(|o| o.borrow().is_none()));
    }

    #[test]
    fn windows_proxy_server_values() {
        assert_eq!(
            parse_proxy_server("proxy.corp:8080"),
            ["http://proxy.corp:8080"]
        );
        assert_eq!(
            parse_proxy_server("http=a:80;https=b:443;socks=c:1080"),
            ["http://b:443", "http://a:80", "socks5://c:1080"]
        );
        assert_eq!(parse_proxy_server("ftp=x:21"), Vec::<String>::new());
        assert_eq!(parse_proxy_server(""), Vec::<String>::new());
    }
}
