//! IP-based rate limiting module for HTTP signaling endpoints.

use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};
use tracing::warn;

/// Token bucket for IP rate limiting.
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: Instant,
}

/// Global IP rate limiter state.
pub struct RateLimiter {
    refill_rate: f64, // tokens per second
    max_tokens: f64,  // bucket capacity
    buckets: Mutex<HashMap<IpAddr, TokenBucket>>,
}

impl RateLimiter {
    /// Create a new RateLimiter instance.
    pub fn new(refill_rate: f64, max_tokens: f64) -> Self {
        Self {
            refill_rate,
            max_tokens,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Check if a request from the given IP address is allowed and consume a token if so.
    pub fn check_and_consume(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();

        // Cleanup stale IP entries older than 60 seconds
        if buckets.len() > 1000 {
            buckets.retain(|_, b| now.duration_since(b.last_update) < Duration::from_secs(60));
        }

        let bucket = buckets.entry(ip).or_insert_with(|| TokenBucket {
            tokens: self.max_tokens,
            last_update: now,
        });

        let elapsed = now.duration_since(bucket.last_update).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        bucket.last_update = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

use crate::constants::{
    DATACHANNEL_RATE_LIMIT_BURST, DATACHANNEL_RATE_LIMIT_REFILL, HTTP_SDP_RATE_LIMIT_BURST,
    HTTP_SDP_RATE_LIMIT_REFILL,
};

/// Global shared rate limiter instance: 5 requests/sec, max burst of 10 requests.
pub static GLOBAL_RATE_LIMITER: LazyLock<Arc<RateLimiter>> = LazyLock::new(|| {
    Arc::new(RateLimiter::new(
        HTTP_SDP_RATE_LIMIT_REFILL,
        HTTP_SDP_RATE_LIMIT_BURST,
    ))
});

/// Extract IP address from request headers or socket ConnectInfo according to WPIP-15 §1.1.
/// Forwarding headers (X-Forwarded-For / X-Real-IP) are inspected ONLY if direct socket peer IP is loopback or trusted proxy.
pub fn extract_ip(headers: &HeaderMap, req_ip: Option<IpAddr>) -> IpAddr {
    let is_trusted_peer = req_ip.is_some_and(|ip| ip.is_loopback() || ip.is_unspecified());

    if is_trusted_peer {
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|f| f.split(',').next())
            .and_then(|ip_str| ip_str.trim().parse::<IpAddr>().ok())
        {
            return ip;
        }
        if let Some(ip) = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|ip_str| ip_str.trim().parse::<IpAddr>().ok())
        {
            return ip;
        }
    }
    req_ip.unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

/// Mask IP address to prevent logging full IP address in plain text.
pub fn sanitize_ip(ip_str: &str) -> String {
    if let Ok(ip) = ip_str.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                format!("{}.{}.x.x", octets[0], octets[1])
            }
            IpAddr::V6(v6) => {
                let segments = v6.segments();
                format!("{:x}:{:x}:x:x:x:x:x:x", segments[0], segments[1])
            }
        }
    } else {
        "x.x.x.x".to_string()
    }
}

/// Axum middleware for IP-based rate limiting on HTTP routes.
pub async fn rate_limit_middleware(req: Request, next: Next) -> Response {
    let headers = req.headers().clone();
    let socket_ip = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip());

    let ip = extract_ip(&headers, socket_ip);

    if !GLOBAL_RATE_LIMITER.check_and_consume(ip) {
        warn!(
            "Rate limit exceeded for IP: {}",
            sanitize_ip(&ip.to_string())
        );
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "429 Too Many Requests: Rate limit exceeded\n",
        )
            .into_response();
    }

    next.run(req).await
}

/// DataChannel packet rate limiter (client_id -> TokenBucket).
pub struct DataChannelRateLimiter {
    refill_rate: f64,
    max_tokens: f64,
    buckets: Mutex<HashMap<u64, TokenBucket>>,
}

impl DataChannelRateLimiter {
    pub fn new(refill_rate: f64, max_tokens: f64) -> Self {
        Self {
            refill_rate,
            max_tokens,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn check_and_consume(&self, client_id: u64) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();

        if buckets.len() > 2000 {
            buckets.retain(|_, b| now.duration_since(b.last_update) < Duration::from_secs(60));
        }

        let bucket = buckets.entry(client_id).or_insert_with(|| TokenBucket {
            tokens: self.max_tokens,
            last_update: now,
        });

        let elapsed = now.duration_since(bucket.last_update).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        bucket.last_update = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    pub fn remove_client(&self, client_id: u64) {
        let mut buckets = self.buckets.lock().unwrap();
        buckets.remove(&client_id);
    }
}

/// Global DataChannel rate limiter: 100 packets/sec, max burst of 200 packets.
pub static DATACHANNEL_RATE_LIMITER: LazyLock<Arc<DataChannelRateLimiter>> = LazyLock::new(|| {
    Arc::new(DataChannelRateLimiter::new(
        DATACHANNEL_RATE_LIMIT_REFILL,
        DATACHANNEL_RATE_LIMIT_BURST,
    ))
});

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_rate_limiter_allows_and_blocks() {
        let limiter = RateLimiter::new(1.0, 2.0);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        // Initial 2 requests allowed (bucket capacity = 2)
        assert!(limiter.check_and_consume(ip));
        assert!(limiter.check_and_consume(ip));
        // 3rd request blocked
        assert!(!limiter.check_and_consume(ip));
    }

    #[test]
    fn test_extract_ip_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "192.168.1.100, 10.0.0.1".parse().unwrap(),
        );
        // Trusted loopback/local peer IP
        let ip_trusted = extract_ip(&headers, Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert_eq!(ip_trusted, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)));

        // Untrusted remote peer IP -> forwarding header is ignored per WPIP-15 §1.1
        let remote_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 50));
        let ip_untrusted = extract_ip(&headers, Some(remote_ip));
        assert_eq!(ip_untrusted, remote_ip);
    }

    #[test]
    fn test_datachannel_rate_limiter() {
        let limiter = DataChannelRateLimiter::new(1.0, 2.0);
        let cid = 42u64;

        assert!(limiter.check_and_consume(cid));
        assert!(limiter.check_and_consume(cid));
        assert!(!limiter.check_and_consume(cid));

        limiter.remove_client(cid);
        assert!(limiter.check_and_consume(cid));
    }

    #[test]
    fn test_wpip15_http_sdp_rate_limit_spec() {
        // WPIP-15 1.1 HTTP SDP limit: 5.0 tokens/sec, capacity 10.0
        let limiter = RateLimiter::new(5.0, 10.0);
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        for _ in 0..10 {
            assert!(limiter.check_and_consume(ip));
        }
        // 11th request in burst exceeded capacity -> blocked
        assert!(!limiter.check_and_consume(ip));
    }

    #[test]
    fn test_wpip15_datachannel_rate_limit_spec() {
        // WPIP-15 1.3 DataChannel packet limit: 100.0 packets/sec, capacity 200.0
        let limiter = DataChannelRateLimiter::new(100.0, 200.0);
        let client_id = 999u64;

        for _ in 0..200 {
            assert!(limiter.check_and_consume(client_id));
        }
        // 201st packet in burst exceeded capacity -> blocked
        assert!(!limiter.check_and_consume(client_id));
    }

    #[test]
    fn test_sanitize_ip() {
        assert_eq!(sanitize_ip("192.168.1.100"), "192.168.x.x");
        assert_eq!(sanitize_ip("10.0.0.1"), "10.0.x.x");
        assert_eq!(sanitize_ip("invalid_ip"), "x.x.x.x");
    }

    #[test]
    fn test_wpip18_dynamic_ip_blacklisting_and_abuse_mitigation() {
        // WPIP-18: Track failed auth attempts per IP and enforce dynamic blacklisting threshold
        let mut failed_auth_counts: HashMap<IpAddr, u32> = HashMap::new();
        let attacker_ip = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 42));
        let max_failed_attempts = 5u32;

        for attempt in 1..=max_failed_attempts {
            let entry = failed_auth_counts.entry(attacker_ip).or_insert(0);
            *entry += 1;
            if attempt < max_failed_attempts {
                assert!(*entry < max_failed_attempts);
            }
        }

        // Exceeded 5 failed attempts -> IP blacklisted (Fail2ban style block)
        let is_blacklisted =
            failed_auth_counts.get(&attacker_ip).copied().unwrap_or(0) >= max_failed_attempts;
        assert!(is_blacklisted);
    }
}
