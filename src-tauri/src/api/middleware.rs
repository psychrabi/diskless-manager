use axum::{
    extract::State,
    http::{header, Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use log::info;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::state::AppState;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
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

pub fn cors_layer() -> CorsLayer {
    use std::time::Duration;
    use tower_http::cors::AllowOrigin;

    let origins = [
        "http://localhost:5173", // Vite default port
        "http://127.0.0.1:5173", // Alternative localhost
        "http://localhost:3000", // Common React port
        "http://127.0.0.1:3000", // Alternative
    ];

    let parsed_origins: Vec<_> = origins
        .iter()
        .map(|origin| origin.parse().expect("Failed to parse CORS origin"))
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

pub async fn require_auth(
    State(_state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // ✅ Always allow OPTIONS (CORS preflight)
    if request.method() == axum::http::Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    // Skip auth for login endpoint
    if request.uri().path() == "/api/auth/login" {
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

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        eprintln!("WARNING: JWT_SECRET environment variable not set!");
        eprintln!("Using fallback secret for development only.");
        eprintln!(
            "For production, set JWT_SECRET environment variable with a secure random string."
        );
        "default_secret_key".to_string()
    });

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let mut request = request;
    request.extensions_mut().insert(token_data.claims);

    Ok(next.run(request).await)
}
