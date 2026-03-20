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

/// Start the gateway HTTP server.
///
/// Binds to `config.host:config.port` and serves pairing and webhook endpoints.
/// Requires the `gateway` feature (axum).
#[cfg(feature = "gateway")]
pub async fn run_gateway(config: &crate::config::GatewayConfig) -> anyhow::Result<()> {
    use axum::routing::get;
    use axum::Router;
    use std::net::SocketAddr;

    if !config.allow_public_bind && config.host != "127.0.0.1" && config.host != "localhost" {
        anyhow::bail!(
            "Gateway host '{}' is not localhost and allow_public_bind=false. \
             Set allow_public_bind=true or use a tunnel.",
            config.host
        );
    }

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/pair", get(pair_placeholder));

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    tracing::info!("Gateway listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(feature = "gateway")]
async fn health_handler() -> &'static str {
    "ok"
}

#[cfg(feature = "gateway")]
async fn pair_placeholder() -> &'static str {
    "pairing not yet implemented"
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
}
