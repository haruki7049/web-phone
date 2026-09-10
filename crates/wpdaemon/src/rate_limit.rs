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

/// Extract IP address from request headers or socket ConnectInfo.
pub fn extract_ip(headers: &HeaderMap, req_ip: Option<IpAddr>) -> IpAddr {
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
    req_ip.unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
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
        warn!("Rate limit exceeded for IP: {}", ip);
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
        let ip = extract_ip(&headers, None);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)));
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
}
