//! HTTP gateway for webhook and pairing endpoints.
//!
//! Provides webhook ingestion (WhatsApp, Slack, etc.), device pairing,
//! rate limiting, and idempotency tracking. Gated behind the `gateway` feature.

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Maximum request body size (64 KB).
pub const MAX_BODY_SIZE: usize = 65_536;

/// Request timeout in seconds.
pub const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Rate limit window in seconds.
pub const RATE_LIMIT_WINDOW_SECS: u64 = 60;

type HmacSha256 = Hmac<Sha256>;

/// Verify a WhatsApp webhook signature.
///
/// WhatsApp sends `X-Hub-Signature-256: sha256=<hex>` on each webhook delivery.
/// This function verifies the HMAC-SHA256 signature using the app secret.
pub fn verify_whatsapp_signature(secret: &str, body: &[u8], header: &str) -> bool {
    let hex_sig = match header.strip_prefix("sha256=") {
        Some(s) => s,
        None => return false,
    };

    let expected_bytes = match hex::decode(hex_sig) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };

    mac.update(body);

    mac.verify_slice(&expected_bytes).is_ok()
}

// ── Rate limiter ─────────────────────────────────────────────────

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Sliding-window rate limiter keyed by client identifier.
#[derive(Clone)]
pub struct RateLimiter {
    /// Per-client request timestamps within the current window.
    buckets: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    /// Maximum requests per window.
    max_requests: u32,
    /// Window duration in seconds.
    window_secs: u64,
    /// Maximum distinct client keys to track (bounds memory).
    max_keys: usize,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64, max_keys: usize) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_secs,
            max_keys,
        }
    }

    /// Check if a request from `client_id` is allowed. Returns true if allowed.
    pub fn check(&self, client_id: &str) -> bool {
        let now = Instant::now();
        let cutoff = now
            .checked_sub(std::time::Duration::from_secs(self.window_secs))
            .unwrap_or(now);
        let mut buckets = self.buckets.lock();

        // Prune if over capacity
        if buckets.len() >= self.max_keys {
            buckets.retain(|_, timestamps| {
                timestamps.retain(|t| *t > cutoff);
                !timestamps.is_empty()
            });
        }

        let timestamps = buckets.entry(client_id.to_string()).or_default();
        timestamps.retain(|t| *t > cutoff);

        if timestamps.len() >= self.max_requests as usize {
            return false;
        }

        timestamps.push(now);
        true
    }
}

// ── Idempotency store ────────────────────────────────────────────

/// Tracks webhook idempotency keys to prevent duplicate processing.
#[derive(Clone)]
pub struct IdempotencyStore {
    /// Map of idempotency key → insertion time.
    keys: Arc<Mutex<HashMap<String, Instant>>>,
    /// TTL for keys in seconds.
    ttl_secs: u64,
    /// Maximum keys to retain.
    max_keys: usize,
}

impl IdempotencyStore {
    pub fn new(ttl_secs: u64, max_keys: usize) -> Self {
        Self {
            keys: Arc::new(Mutex::new(HashMap::new())),
            ttl_secs,
            max_keys,
        }
    }

    /// Check if an idempotency key has been seen. Returns true if it's a duplicate.
    pub fn is_duplicate(&self, key: &str) -> bool {
        let now = Instant::now();
        let cutoff = now
            .checked_sub(std::time::Duration::from_secs(self.ttl_secs))
            .unwrap_or(now);
        let mut keys = self.keys.lock();

        // Prune expired keys if over capacity
        if keys.len() >= self.max_keys {
            keys.retain(|_, inserted| *inserted > cutoff);
        }

        if let Some(inserted) = keys.get(key) {
            if *inserted > cutoff {
                return true; // Still within TTL — duplicate
            }
        }

        keys.insert(key.to_string(), now);
        false
    }
}

// ── Gateway server (axum) ────────────────────────────────────────

/// Start the gateway HTTP server.
///
/// Binds to `config.host:config.port` and serves pairing and webhook endpoints.
/// Requires the `gateway` feature (axum).
#[cfg(feature = "gateway")]
pub async fn run_gateway(config: &crate::config::GatewayConfig) -> anyhow::Result<()> {
    use axum::routing::{get, post};
    use axum::Router;
    use std::net::SocketAddr;

    use crate::security::pairing::PairingGuard;

    if !config.allow_public_bind && config.host != "127.0.0.1" && config.host != "localhost" {
        anyhow::bail!(
            "Gateway host '{}' is not localhost and allow_public_bind=false. \
             Set allow_public_bind=true or use a tunnel.",
            config.host
        );
    }

    let pairing = PairingGuard::new(config.require_pairing, &config.paired_tokens);
    if let Some(code) = pairing.pairing_code() {
        tracing::info!("Gateway pairing code: {code}");
        tracing::info!("Send POST /pair with X-Pairing-Code: {code}");
    }

    let pair_limiter = RateLimiter::new(
        config.pair_rate_limit_per_minute,
        RATE_LIMIT_WINDOW_SECS,
        config.rate_limit_max_keys,
    );
    let webhook_limiter = RateLimiter::new(
        config.webhook_rate_limit_per_minute,
        RATE_LIMIT_WINDOW_SECS,
        config.rate_limit_max_keys,
    );
    let idempotency =
        IdempotencyStore::new(config.idempotency_ttl_secs, config.idempotency_max_keys);

    let state = GatewayState {
        pairing,
        pair_limiter,
        webhook_limiter,
        idempotency,
    };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/pair", post(pair_handler))
        .route("/webhook", post(webhook_handler))
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    tracing::info!("Gateway listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(feature = "gateway")]
#[derive(Clone)]
struct GatewayState {
    pairing: crate::security::pairing::PairingGuard,
    pair_limiter: RateLimiter,
    webhook_limiter: RateLimiter,
    idempotency: IdempotencyStore,
}

#[cfg(feature = "gateway")]
async fn health_handler() -> &'static str {
    "ok"
}

#[cfg(feature = "gateway")]
async fn pair_handler(
    axum::extract::State(state): axum::extract::State<GatewayState>,
    headers: axum::http::HeaderMap,
) -> impl axum::response::IntoResponse {
    use axum::http::StatusCode;

    let client_id = extract_client_id(&headers);

    if !state.pair_limiter.check(&client_id) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded".to_string(),
        );
    }

    let code = match headers.get("x-pairing-code").and_then(|v| v.to_str().ok()) {
        Some(c) => c,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Missing X-Pairing-Code header".to_string(),
            )
        }
    };

    match state.pairing.try_pair(code, &client_id).await {
        Ok(Some(token)) => (
            StatusCode::OK,
            serde_json::json!({"token": token}).to_string(),
        ),
        Ok(None) => (StatusCode::UNAUTHORIZED, "Invalid pairing code".to_string()),
        Err(lockout_secs) => (
            StatusCode::TOO_MANY_REQUESTS,
            format!("Locked out for {lockout_secs} seconds"),
        ),
    }
}

#[cfg(feature = "gateway")]
async fn webhook_handler(
    axum::extract::State(state): axum::extract::State<GatewayState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl axum::response::IntoResponse {
    use axum::http::StatusCode;

    if body.len() > MAX_BODY_SIZE {
        return (StatusCode::PAYLOAD_TOO_LARGE, "Body too large".to_string());
    }

    // Auth check
    if state.pairing.require_pairing() {
        let token = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        match token {
            Some(t) if state.pairing.is_authenticated(t) => {}
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    "Invalid or missing bearer token".to_string(),
                )
            }
        }
    }

    let client_id = extract_client_id(&headers);
    if !state.webhook_limiter.check(&client_id) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded".to_string(),
        );
    }

    // Idempotency check
    if let Some(idem_key) = headers.get("idempotency-key").and_then(|v| v.to_str().ok()) {
        if state.idempotency.is_duplicate(idem_key) {
            return (StatusCode::OK, "Already processed".to_string());
        }
    }

    // TODO: Route webhook payload to appropriate channel handler
    tracing::info!(
        client = %client_id,
        size = body.len(),
        "Webhook received"
    );

    (StatusCode::ACCEPTED, "Accepted".to_string())
}

#[cfg(feature = "gateway")]
fn extract_client_id(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .split(',')
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_expected() {
        assert_eq!(MAX_BODY_SIZE, 65_536);
        assert_eq!(REQUEST_TIMEOUT_SECS, 30);
        assert_eq!(RATE_LIMIT_WINDOW_SECS, 60);
    }

    #[test]
    fn valid_whatsapp_signature() {
        let secret = "mysecret";
        let body = b"hello";
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("sha256={sig}");
        assert!(verify_whatsapp_signature(secret, body, &header));
    }

    #[test]
    fn invalid_whatsapp_signature() {
        assert!(!verify_whatsapp_signature("secret", b"body", "sha256=0000"));
    }

    #[test]
    fn missing_prefix_rejected() {
        assert!(!verify_whatsapp_signature("secret", b"body", "abcdef"));
    }

    #[test]
    fn empty_header_rejected() {
        assert!(!verify_whatsapp_signature("secret", b"body", ""));
    }

    // ── Rate limiter ──────────────────────────────────────────

    #[test]
    fn rate_limiter_allows_under_limit() {
        let limiter = RateLimiter::new(3, 60, 100);
        assert!(limiter.check("client1"));
        assert!(limiter.check("client1"));
        assert!(limiter.check("client1"));
    }

    #[test]
    fn rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(2, 60, 100);
        assert!(limiter.check("client1"));
        assert!(limiter.check("client1"));
        assert!(!limiter.check("client1"));
    }

    #[test]
    fn rate_limiter_per_client() {
        let limiter = RateLimiter::new(1, 60, 100);
        assert!(limiter.check("client1"));
        assert!(limiter.check("client2"));
        assert!(!limiter.check("client1"));
    }

    // ── Idempotency store ─────────────────────────────────────

    #[test]
    fn idempotency_detects_duplicate() {
        let store = IdempotencyStore::new(300, 100);
        assert!(!store.is_duplicate("key1"));
        assert!(store.is_duplicate("key1"));
    }

    #[test]
    fn idempotency_different_keys_independent() {
        let store = IdempotencyStore::new(300, 100);
        assert!(!store.is_duplicate("key1"));
        assert!(!store.is_duplicate("key2"));
    }
}
