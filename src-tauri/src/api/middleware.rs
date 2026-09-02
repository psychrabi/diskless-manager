use axum::{
    extract::State,
    http::{header, Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use log::info;
use tower_http::cors::CorsLayer;

use crate::state::AppState;

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

pub fn cors_layer() -> CorsLayer {
    use std::time::Duration;
    use tower_http::cors::AllowOrigin;

    let origins = [
        "http://localhost:5173", // Vite default port
        "http://127.0.0.1:5173", // Alternative localhost
        "http://localhost:3000", // Common React port
        "http://127.0.0.1:3000", // Alternative
        "http://localhost:1420", // Tauri dev server
        "http://127.0.0.1:1420", // Tauri dev server alternative
        "tauri://localhost",     // Tauri protocol
    ];

    let parsed_origins: Vec<_> = origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed_origins))
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

    // For WebSocket connections, check query parameter
    if path == "/ws/metrics" {
        log::info!("WebSocket auth check for path: {}", path);
        if let Some(query) = request.uri().query() {
            log::debug!("Query string found: {}", query);
            if let Some(token_start) = query.find("token=") {
                let token_part = &query[token_start + 6..];
                let token = token_part.split('&').next().unwrap_or("");
                log::debug!("Token extracted from query");
                let decoded_token = urlencoding::decode(token)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| token.to_string());

                match decode::<crate::types::auth::Claims>(
                    &decoded_token,
                    &DecodingKey::from_secret(crate::auth::jwt_secret()),
                    &Validation::new(Algorithm::HS256),
                ) {
                    Ok(_) => {
                        log::info!("WebSocket auth successful");
                        return Ok(next.run(request).await);
                    }
                    Err(e) => {
                        log::warn!("WebSocket auth failed: {}", e);
                        return Err(StatusCode::UNAUTHORIZED);
                    }
                }
            } else {
                log::warn!("No token found in query string");
            }
        } else {
            log::warn!("No query string in WebSocket request");
        }
        log::warn!("WebSocket request missing token");
        return Err(StatusCode::UNAUTHORIZED);
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
    use super::is_public_endpoint;
    use axum::http::Method;

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
}
