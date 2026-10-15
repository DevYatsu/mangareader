//! Reader image proxy (spec Slice R): fetch page images server-side with the
//! source's `Referer`, keep them in a size-capped disk cache, and serve them
//! from our own origin so the reader never hotlinks.
//!
//! The fetcher is an SSRF boundary: page URLs come from scraped HTML, which a
//! hostile or compromised source controls. Every hop is resolved once, vetted
//! against non-public address ranges, and the connection is pinned to the
//! vetted addresses so a second DNS answer cannot swap in an internal host
//! (DNS rebinding).

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// `site_settings` key for the cache cap, in MiB.
pub const CACHE_MAX_MB_KEY: &str = "image_cache_max_mb";
/// Default cap: 5 GiB (spec open question 1).
pub const DEFAULT_CACHE_MAX_MB: u64 = 5120;
/// Largest image accepted from an origin.
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
/// Redirect hops followed, each re-vetted.
const MAX_REDIRECTS: usize = 3;
/// Concurrent origin fetches, so one reader cannot fan out against a source.
const MAX_CONCURRENT_FETCHES: usize = 8;
/// A hit refreshes the LRU timestamp at most this often, to avoid a metadata
/// write on every request.
const TOUCH_INTERVAL: Duration = Duration::from_secs(3600);
/// Eviction trims to this fraction of the cap, so the next few writes do not
/// each trigger a full directory scan.
const EVICT_TARGET: f64 = 0.9;
/// Some image CDNs reject non-browser agents outright.
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("blocked: {0}")]
    Blocked(String),
    #[error("upstream: {0}")]
    Upstream(String),
    #[error("not an image")]
    NotAnImage,
    #[error("too large")]
    TooLarge,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// One served image.
pub struct Image {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    /// Strong validator: the cache key, quoted.
    pub etag: String,
}

pub struct ImageProxy {
    dir: PathBuf,
    /// Source name → `Referer` value.
    referers: HashMap<String, String>,
    fetch_permits: tokio::sync::Semaphore,
    used_bytes: Arc<AtomicU64>,
    evicting: Arc<std::sync::Mutex<()>>,
    /// Tests fetch from a loopback server; production never sets this.
    allow_private: bool,
}

impl ImageProxy {
    pub fn new(dir: PathBuf, referers: HashMap<String, String>) -> Self {
        ImageProxy {
            dir,
            referers,
            fetch_permits: tokio::sync::Semaphore::new(MAX_CONCURRENT_FETCHES),
            used_bytes: Arc::new(AtomicU64::new(0)),
            evicting: Arc::new(std::sync::Mutex::new(())),
            allow_private: false,
        }
    }

    /// Cache under `MANGAREAD_CACHE_DIR`, else `cache/` beside the database,
    /// so the Docker data volume holds it without extra configuration.
    pub fn from_env() -> Self {
        let dir = std::env::var("MANGAREAD_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let db = PathBuf::from(mangaread_core::db::database_path());
                db.parent().unwrap_or(Path::new(".")).join("cache")
            });
        let referers = crate::scraper_helper::source_list()
            .into_iter()
            .map(|(name, base)| (name, format!("{}/", base.trim_end_matches('/'))))
            .collect();
        Self::new(dir, referers)
    }

    #[cfg(test)]
    pub fn allowing_private(mut self) -> Self {
        self.allow_private = true;
        self
    }

    /// Where `url` is cached, so tests can seed the cache without a network.
    #[cfg(test)]
    pub fn cached_path(&self, url: &str) -> PathBuf {
        self.path_for(&url_key(url))
    }

    /// Recount the bytes on disk. Called once at boot, off the request path.
    pub fn scan_usage(&self) {
        let total: u64 = cache_files(&self.dir).iter().map(|f| f.len).sum();
        self.used_bytes.store(total, Ordering::Relaxed);
    }

    pub fn used_bytes(&self) -> u64 {
        self.used_bytes.load(Ordering::Relaxed)
    }

    /// Serve `url` from the cache, fetching it from the origin on a miss.
    pub async fn get(&self, url: &str, source: &str, cap_bytes: u64) -> Result<Image, ProxyError> {
        let key = url_key(url);
        let path = self.path_for(&key);
        let etag = format!("\"{key}\"");

        if let Some(bytes) = read_and_touch(path.clone()).await? {
            // A cached file passed the sniff when written; re-sniffing is a
            // cheap guard against a hand-edited or truncated cache.
            if let Some(content_type) = sniff(&bytes) {
                return Ok(Image {
                    bytes,
                    content_type,
                    etag,
                });
            }
        }

        let _permit = self
            .fetch_permits
            .acquire()
            .await
            .map_err(|_| ProxyError::Upstream("shutting down".into()))?;
        let bytes = self
            .fetch(url, self.referers.get(source).map(String::as_str))
            .await?;
        let content_type = sniff(&bytes).ok_or(ProxyError::NotAnImage)?;

        self.store(&path, &bytes).await?;
        let used = self
            .used_bytes
            .fetch_add(bytes.len() as u64, Ordering::Relaxed)
            + bytes.len() as u64;
        if used > cap_bytes {
            self.evict_in_background(cap_bytes);
        }
        Ok(Image {
            bytes,
            content_type,
            etag,
        })
    }

    fn path_for(&self, key: &str) -> PathBuf {
        // Two-character fan-out keeps any one directory small.
        self.dir.join(&key[..2]).join(key)
    }

    /// Write via a temp file and rename, so a reader never sees half a file.
    async fn store(&self, path: &Path, bytes: &[u8]) -> Result<(), ProxyError> {
        let parent = path.parent().expect("cache path has a parent");
        tokio::fs::create_dir_all(parent).await?;
        let tmp = parent.join(format!(".tmp-{}", uuid::Uuid::new_v4()));
        tokio::fs::write(&tmp, bytes).await?;
        if let Err(e) = tokio::fs::rename(&tmp, path).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(e.into());
        }
        Ok(())
    }

    fn evict_in_background(&self, cap_bytes: u64) {
        let dir = self.dir.clone();
        let used = self.used_bytes.clone();
        let evicting = self.evicting.clone();
        tokio::task::spawn_blocking(move || {
            // One pass at a time; a writer that loses the race relies on the
            // running pass, whose rescan also counts its file.
            let Ok(_guard) = evicting.try_lock() else {
                return;
            };
            let remaining = evict(&dir, (cap_bytes as f64 * EVICT_TARGET) as u64);
            used.store(remaining, Ordering::Relaxed);
        });
    }

    /// Fetch with manual redirects, vetting and pinning every hop.
    async fn fetch(&self, url: &str, referer: Option<&str>) -> Result<Vec<u8>, ProxyError> {
        let mut current =
            reqwest::Url::parse(url).map_err(|e| ProxyError::Blocked(e.to_string()))?;
        for _ in 0..=MAX_REDIRECTS {
            let client = self.pinned_client(&current).await?;
            let mut req = client
                .get(current.clone())
                .header(reqwest::header::ACCEPT, "image/*");
            if let Some(r) = referer {
                req = req.header(reqwest::header::REFERER, r);
            }
            let mut res = req
                .send()
                .await
                .map_err(|e| ProxyError::Upstream(e.to_string()))?;
            let status = res.status();
            if status.is_redirection() {
                let location = res
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| ProxyError::Upstream(format!("{status} without Location")))?;
                current = current
                    .join(location)
                    .map_err(|e| ProxyError::Blocked(e.to_string()))?;
                continue;
            }
            if !status.is_success() {
                return Err(ProxyError::Upstream(status.to_string()));
            }
            if res
                .content_length()
                .is_some_and(|n| n as usize > MAX_IMAGE_BYTES)
            {
                return Err(ProxyError::TooLarge);
            }
            let mut body = Vec::new();
            while let Some(chunk) = res
                .chunk()
                .await
                .map_err(|e| ProxyError::Upstream(e.to_string()))?
            {
                if body.len() + chunk.len() > MAX_IMAGE_BYTES {
                    return Err(ProxyError::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            return Ok(body);
        }
        Err(ProxyError::Upstream("too many redirects".into()))
    }

    /// A client that can only reach `url`'s host at addresses vetted here.
    async fn pinned_client(&self, url: &reqwest::Url) -> Result<reqwest::Client, ProxyError> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ProxyError::Blocked(format!("scheme {}", url.scheme())));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ProxyError::Blocked("credentials in URL".into()));
        }
        let builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .user_agent(USER_AGENT)
            // A system proxy would make the vetted address meaningless.
            .no_proxy();
        let builder = match url.host() {
            Some(url::Host::Ipv4(ip)) => {
                self.vet(IpAddr::V4(ip))?;
                builder
            }
            Some(url::Host::Ipv6(ip)) => {
                self.vet(IpAddr::V6(ip))?;
                builder
            }
            Some(url::Host::Domain(domain)) => {
                let port = url.port_or_known_default().unwrap_or(443);
                let addrs: Vec<SocketAddr> = tokio::net::lookup_host((domain, port))
                    .await
                    .map_err(|e| ProxyError::Upstream(format!("resolve {domain}: {e}")))?
                    .collect();
                if addrs.is_empty() {
                    return Err(ProxyError::Upstream(format!(
                        "resolve {domain}: no addresses"
                    )));
                }
                // Every answer must be public: reqwest may try any of them.
                for a in &addrs {
                    self.vet(a.ip())?;
                }
                builder.resolve_to_addrs(domain, &addrs)
            }
            None => return Err(ProxyError::Blocked("no host".into())),
        };
        builder
            .build()
            .map_err(|e| ProxyError::Upstream(e.to_string()))
    }

    fn vet(&self, ip: IpAddr) -> Result<(), ProxyError> {
        if self.allow_private || is_public(ip) {
            Ok(())
        } else {
            Err(ProxyError::Blocked(format!("non-public address {ip}")))
        }
    }
}

/// Hex SHA-256 of the origin URL: the cache file name and ETag.
pub fn url_key(url: &str) -> String {
    Sha256::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Reader-facing URL for a page. The `v` query changes when a re-sync
/// changes the origin URL, which keeps `immutable` caching correct.
pub fn page_src(page: &mangaread_core::Page) -> String {
    format!("/img/{}?v={}", page.id, &url_key(&page.image_url)[..12])
}

/// Identify an image by magic bytes. The origin's `Content-Type` is never
/// trusted: serving its HTML from our origin would be stored XSS.
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else if b.len() >= 12 && &b[4..8] == b"ftyp" && (&b[8..12] == b"avif" || &b[8..12] == b"avis")
    {
        Some("image/avif")
    } else {
        None
    }
}

/// Whether `ip` is globally routable unicast. Everything an attacker could
/// use to reach the host or its LAN is refused.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            // IPv4-mapped (::ffff:a.b.c.d) reaches the embedded IPv4 host.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let seg = v6.segments();
            // 6to4 (2002::/16) embeds an IPv4 address in segments 1–2.
            if seg[0] == 0x2002 {
                let v4 = Ipv4Addr::new(
                    (seg[1] >> 8) as u8,
                    seg[1] as u8,
                    (seg[2] >> 8) as u8,
                    seg[2] as u8,
                );
                return is_public_v4(v4);
            }
            let global_unicast = (seg[0] & 0xE000) == 0x2000; // 2000::/3
            let documentation = seg[0] == 0x2001 && seg[1] == 0x0db8; // 2001:db8::/32
            global_unicast && !documentation && v6 != Ipv6Addr::UNSPECIFIED
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || o[0] == 0 // 0.0.0.0/8 "this network"
        || (o[0] == 100 && (o[1] & 0xC0) == 64) // 100.64.0.0/10 CGNAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24 IETF
        || (o[0] == 198 && (o[1] & 0xFE) == 18) // 198.18.0.0/15 benchmarking
        || o[0] >= 240) // 240.0.0.0/4 reserved
}

/// Read a cached file and refresh its LRU timestamp. `None` on a miss.
async fn read_and_touch(path: PathBuf) -> Result<Option<Vec<u8>>, ProxyError> {
    tokio::task::spawn_blocking(move || {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let stale = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .map(|t| t.elapsed().unwrap_or_default() > TOUCH_INTERVAL)
            .unwrap_or(false);
        if stale {
            // Best effort: a failed touch only makes eviction slightly less fair.
            let _ = std::fs::File::options()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_modified(SystemTime::now()));
        }
        Ok(Some(bytes))
    })
    .await
    .map_err(|e| ProxyError::Io(std::io::Error::other(e)))?
}

struct CacheFile {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
}

/// Every cached image under `dir` (two levels: fan-out dir, then file).
fn cache_files(dir: &Path) -> Vec<CacheFile> {
    let mut out = Vec::new();
    let Ok(buckets) = std::fs::read_dir(dir) else {
        return out;
    };
    for bucket in buckets.flatten() {
        let Ok(files) = std::fs::read_dir(bucket.path()) else {
            continue;
        };
        for f in files.flatten() {
            let Ok(meta) = f.metadata() else { continue };
            if meta.is_file() {
                out.push(CacheFile {
                    path: f.path(),
                    len: meta.len(),
                    modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                });
            }
        }
    }
    out
}

/// Delete least-recently-used files until at most `target` bytes remain.
/// Returns the bytes left on disk.
fn evict(dir: &Path, target: u64) -> u64 {
    let mut files = cache_files(dir);
    let mut total: u64 = files.iter().map(|f| f.len).sum();
    files.sort_by_key(|f| f.modified);
    for f in files {
        if total <= target {
            break;
        }
        if std::fs::remove_file(&f.path).is_ok() {
            total -= f.len;
        }
    }
    total
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("mangaread-img-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn public_addresses_pass_and_internal_ones_do_not() {
        for ok in ["93.184.216.34", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(is_public(ok.parse().unwrap()), "{ok} should be public");
        }
        for bad in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "198.18.0.1",
            "192.0.0.8",
            "240.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.1.2.3",
            "2002:7f00:0001::1",
            "64:ff9b::7f00:1",
        ] {
            assert!(!is_public(bad.parse().unwrap()), "{bad} should be blocked");
        }
    }

    #[test]
    fn sniff_accepts_images_and_rejects_markup() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(sniff(b"GIF89a.."), Some("image/gif"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"\0\0\0\x1cftypavif"), Some("image/avif"));
        assert_eq!(sniff(b"<html><script>"), None);
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn eviction_removes_oldest_first() {
        let dir = temp_dir();
        std::fs::create_dir_all(dir.join("aa")).unwrap();
        let now = SystemTime::now();
        for (i, name) in ["old", "mid", "new"].iter().enumerate() {
            let p = dir.join("aa").join(name);
            std::fs::write(&p, vec![0u8; 100]).unwrap();
            let age = Duration::from_secs(1000 * (3 - i as u64));
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(now - age)
                .unwrap();
        }
        let left = evict(&dir, 150);
        assert_eq!(left, 100);
        assert!(!dir.join("aa/old").exists());
        assert!(!dir.join("aa/mid").exists());
        assert!(dir.join("aa/new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn private_targets_are_refused_before_connecting() {
        let proxy = ImageProxy::new(temp_dir(), HashMap::new());
        for url in [
            "http://127.0.0.1/a.jpg",
            "http://[::1]/a.jpg",
            "http://169.254.169.254/latest/meta-data",
            "http://localhost/a.jpg",
            "file:///etc/passwd",
            "http://user:pw@example.com/a.jpg",
        ] {
            let err = proxy.get(url, "x", u64::MAX).await.err();
            assert!(
                matches!(err, Some(ProxyError::Blocked(_))),
                "{url}: {err:?}"
            );
        }
    }

    /// A loopback origin that redirects once, then serves a PNG only to
    /// requests carrying the expected Referer.
    async fn origin() -> (String, std::sync::Arc<AtomicU64>) {
        use axum::http::{HeaderMap, StatusCode, header};
        use axum::response::IntoResponse;
        let hits = std::sync::Arc::new(AtomicU64::new(0));
        let counter = hits.clone();
        let app = axum::Router::new()
            .route(
                "/r",
                axum::routing::get(|| async { axum::response::Redirect::temporary("/p.png") }),
            )
            .route(
                "/p.png",
                axum::routing::get(move |h: HeaderMap| {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::Relaxed);
                        if h.get(header::REFERER).and_then(|v| v.to_str().ok())
                            != Some("https://src.example/")
                        {
                            return StatusCode::FORBIDDEN.into_response();
                        }
                        (
                            [(header::CONTENT_TYPE, "text/html")],
                            b"\x89PNG\r\n\x1a\nbody".to_vec(),
                        )
                            .into_response()
                    }
                }),
            )
            .route(
                "/html",
                axum::routing::get(|| async { "<script>alert(1)</script>" }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), hits)
    }

    #[tokio::test]
    async fn fetches_with_referer_follows_redirects_and_caches() {
        let (base, hits) = origin().await;
        let referers = HashMap::from([("src".to_string(), "https://src.example/".to_string())]);
        let dir = temp_dir();
        let proxy = ImageProxy::new(dir.clone(), referers).allowing_private();
        let url = format!("{base}/r");

        let first = proxy.get(&url, "src", u64::MAX).await.unwrap();
        assert_eq!(first.content_type, "image/png");
        assert!(first.bytes.starts_with(b"\x89PNG"));
        let second = proxy.get(&url, "src", u64::MAX).await.unwrap();
        assert_eq!(second.etag, first.etag);
        assert_eq!(
            hits.load(Ordering::Relaxed),
            1,
            "second read comes from disk"
        );
        assert_eq!(proxy.used_bytes(), first.bytes.len() as u64);

        // Without the Referer the origin refuses, and nothing is cached.
        let err = proxy
            .get(&format!("{base}/p.png?x"), "unknown", u64::MAX)
            .await
            .err();
        assert!(matches!(err, Some(ProxyError::Upstream(_))), "{err:?}");
        let err = proxy
            .get(&format!("{base}/html"), "src", u64::MAX)
            .await
            .err();
        assert!(matches!(err, Some(ProxyError::NotAnImage)), "{err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
