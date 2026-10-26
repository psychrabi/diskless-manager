use axum::http::StatusCode;
use serde::{ser::Serializer, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Serialize)]
struct ApiErrorBody<'a> {
    code: &'static str,
    message: &'a str,
    operation_id: String,
    details: Value,
}

fn error_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "invalid_request",
        StatusCode::PAYLOAD_TOO_LARGE => "invalid_request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::REQUEST_TIMEOUT => "request_timeout",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::METHOD_NOT_ALLOWED => "method_not_allowed",
        StatusCode::CONFLICT => "state_conflict",
        StatusCode::UNPROCESSABLE_ENTITY => "policy_violation",
        StatusCode::SERVICE_UNAVAILABLE => "dependency_unavailable",
        _ => "internal_error",
    }
}

/// Serialize typed handler errors using the common HTTP error contract.
pub fn serialize_api_error<S>(
    status: u16,
    message: &str,
    details: Value,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    ApiErrorBody {
        code: error_code(status),
        message,
        operation_id: Uuid::new_v4().to_string(),
        details,
    }
    .serialize(serializer)
}

/// Give bare statuses and legacy API errors the same wire contract.
pub async fn normalize_api_errors(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let api = request.uri().path() == "/api" || request.uri().path().starts_with("/api/");
    let response = next.run(request).await;
    let status = response.status();
    if !api || !(status.is_client_error() || status.is_server_error()) {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 1024 * 1024)
        .await
        .unwrap_or_default();
    let existing = serde_json::from_slice::<Value>(&bytes).ok();
    if existing.as_ref().is_some_and(|value| {
        ["code", "message", "operation_id", "details"]
            .iter()
            .all(|key| value.get(key).is_some())
    }) {
        return axum::response::Response::from_parts(parts, axum::body::Body::from(bytes));
    }
    let message = existing
        .as_ref()
        .and_then(|value| value.get("message").or_else(|| value.get("error")))
        .and_then(Value::as_str)
        .unwrap_or_else(|| status.canonical_reason().unwrap_or("Request failed"));
    let body = serde_json::to_vec(&ApiErrorBody {
        code: error_code(status),
        message,
        operation_id: Uuid::new_v4().to_string(),
        details: existing
            .as_ref()
            .and_then(|value| value.get("details"))
            .cloned()
            .unwrap_or(Value::Null),
    })
    .expect("API error body contains only JSON values");
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    parts.headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    axum::response::Response::from_parts(parts, axum::body::Body::from(body))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::StatusCode,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn bare_status_errors_have_the_shared_contract() {
        let app = Router::new()
            .route("/api/test", get(|| async { StatusCode::FORBIDDEN }))
            .layer(axum::middleware::from_fn(super::normalize_api_errors));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(body["code"], "forbidden");
        assert!(body["operation_id"].as_str().is_some());
        assert!(body.get("details").is_some());
    }

    #[tokio::test]
    async fn oversized_request_errors_have_the_shared_contract() {
        let app = Router::new()
            .route("/api/test", get(|| async { StatusCode::PAYLOAD_TOO_LARGE }))
            .layer(axum::middleware::from_fn(super::normalize_api_errors));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(body["code"], "invalid_request");
        assert!(body["operation_id"].as_str().is_some());
        assert!(body.get("details").is_some());
    }
    #[tokio::test]
    async fn existing_operation_ids_and_details_survive_normalization() {
        let expected = serde_json::json!({"code":"state_conflict", "message":"Recovery pending", "operation_id":"existing-operation", "details":{"client_id":"fixture"}});
        let body = expected.clone();
        let app = Router::new()
            .route(
                "/api/test",
                get(move || {
                    let body = body.clone();
                    async move { (StatusCode::CONFLICT, axum::Json(body)) }
                }),
            )
            .layer(axum::middleware::from_fn(super::normalize_api_errors));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(body, expected);
    }
}
