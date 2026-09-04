use axum::{
    extract::{connect_info::ConnectInfo, State},
    http::{header, Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use log::info;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::{net::IpAddr, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

use crate::state::AppState;

/// Simple in-memory auth attempt limiter (10 attempts per 60 seconds per IP).
#[derive(Clone, Default)]
pub struct AuthRateLimiter {
    attempts: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
}

impl AuthRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the request is allowed, false if rate-limited.
    async fn check(&self, ip: &str) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(60);
        let max_attempts = 10;

        let mut map = self.attempts.write().await;
        let entries = map.entry(ip.to_string()).or_default();

        // Drop failed attempts older than the window.
        entries.retain(|t| now.duration_since(*t) < window);

        if entries.len() >= max_attempts {
            return false;
        }
        entries.push(now);
        true
    }

    /// Purge stale entries to avoid unbounded growth.
    pub async fn cleanup(&self) {
        let now = Instant::now();
        let window = Duration::from_secs(60);
        let mut map = self.attempts.write().await;
        map.retain(|_, v| {
            v.retain(|t| now.duration_since(*t) < window);
            !v.is_empty()
        });
    }
}

/// Use the transport peer address. Forwarded headers are intentionally
/// ignored because Diskless Manager does not configure a trusted proxy.
fn client_ip(request: &Request<axum::body::Body>) -> Option<IpAddr> {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0.ip())
}

/// Rate-limit login endpoints by IP.
pub async fn rate_limit_auth(
    State(limiter): State<AuthRateLimiter>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path();
    if path == "/api/auth/login" || path == "/api/auth/bootstrap" {
        let ip = client_ip(&request).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        if !limiter.check(&ip.to_string()).await {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    Ok(next.run(request).await)
}

pub async fn logger(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = request.method().clone();
    let uri = request.uri().clone();

    info!("{} {}", method, uri);

    let response = next.run(request).await;

    info!("Response status: {}", response.status());

    Ok(response)
}

fn cors_origins(include_development_origins: bool) -> Vec<axum::http::HeaderValue> {
    let mut origins = vec!["tauri://localhost", "http://tauri.localhost"];
    if include_development_origins {
        origins.extend([
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "http://localhost:1420",
            "http://127.0.0.1:1420",
        ]);
    }

    origins
        .into_iter()
        .filter_map(|origin| origin.parse().ok())
        .collect()
}

pub fn cors_layer() -> CorsLayer {
    use std::time::Duration;
    use tower_http::cors::AllowOrigin;

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_origins(cfg!(debug_assertions))))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .allow_credentials(true)
        .max_age(Duration::from_secs(86400)) // Cache preflight requests for 24 hours
}

fn websocket_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let protocols = headers.get(header::SEC_WEBSOCKET_PROTOCOL)?.to_str().ok()?;
    let mut protocols = protocols.split(',').map(str::trim);
    match (protocols.next(), protocols.next()) {
        (Some("diskless-auth"), Some(token)) if !token.is_empty() => Some(token),
        _ => None,
    }
}

fn is_public_endpoint(method: &Method, path: &str) -> bool {
    matches!(
        (method, path),
        (&Method::GET, "/health")
            | (&Method::POST, "/api/auth/login")
            | (&Method::POST, "/api/auth/validate")
            | (&Method::GET, "/api/auth/admin/exists")
            | (&Method::POST, "/api/auth/bootstrap")
    )
}

pub async fn require_auth(
    State(_state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // ✅ Always allow OPTIONS (CORS preflight)
    if request.method() == axum::http::Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let path = request.uri().path();

    // Skip auth for public endpoints
    if is_public_endpoint(request.method(), path) {
        return Ok(next.run(request).await);
    }

    // Browsers cannot attach an Authorization header to WebSocket handshakes.
    // Carry the token in the subprotocol header so it is not exposed in URLs,
    // browser history, proxy logs, or request logging.
    if path == "/ws/metrics" {
        let token = websocket_token(request.headers()).ok_or(StatusCode::UNAUTHORIZED)?;
        decode::<crate::types::auth::Claims>(
            token,
            &DecodingKey::from_secret(crate::auth::jwt_secret()),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
        return Ok(next.run(request).await);
    }

    let headers = request.headers();
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let token = match auth_header {
        Some(t) => t,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let token_data = decode::<crate::types::auth::Claims>(
        token,
        &DecodingKey::from_secret(crate::auth::jwt_secret()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let mut request = request;
    request.extensions_mut().insert(token_data.claims);

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::{client_ip, cors_origins, is_public_endpoint, websocket_token};
    use axum::{
        body::Body,
        extract::connect_info::ConnectInfo,
        http::{header, HeaderMap, HeaderValue, Method, Request},
    };
    use std::net::SocketAddr;

    #[test]
    fn only_bootstrap_and_session_endpoints_are_public() {
        assert!(is_public_endpoint(&Method::GET, "/health"));
        assert!(is_public_endpoint(&Method::POST, "/api/auth/login"));
        assert!(is_public_endpoint(&Method::POST, "/api/auth/bootstrap"));

        assert!(!is_public_endpoint(&Method::GET, "/api/config"));
        assert!(!is_public_endpoint(
            &Method::GET,
            "/api/system/dependencies"
        ));
        assert!(!is_public_endpoint(
            &Method::PUT,
            "/api/auth/admin/password"
        ));
    }

    #[test]
    fn rate_limit_identity_uses_peer_address_not_forwarded_header() {
        let mut request = Request::builder()
            .header("x-forwarded-for", "203.0.113.99")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:43120".parse::<SocketAddr>().unwrap(),
        ));

        assert_eq!(client_ip(&request).unwrap().to_string(), "127.0.0.1");
    }

    #[test]
    fn rate_limit_identity_requires_peer_address() {
        let request = Request::new(Body::empty());
        assert!(client_ip(&request).is_none());
    }

    #[test]
    fn production_cors_origins_exclude_development_servers() {
        let origins: Vec<_> = cors_origins(false)
            .into_iter()
            .map(|origin| origin.to_str().unwrap().to_string())
            .collect();
        assert_eq!(origins, ["tauri://localhost", "http://tauri.localhost"]);
    }

    #[test]
    fn websocket_token_comes_from_subprotocol_not_query_string() {
        let mut headers = HeaderMap::new();
        assert!(websocket_token(&headers).is_none());

        headers.insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static("diskless-auth, header.payload.signature"),
        );
        assert_eq!(websocket_token(&headers), Some("header.payload.signature"));
    }
}
