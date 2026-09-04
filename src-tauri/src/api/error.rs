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
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::CONFLICT => "state_conflict",
        StatusCode::UNPROCESSABLE_ENTITY => "policy_violation",
        StatusCode::SERVICE_UNAVAILABLE => "dependency_unavailable",
        _ => "internal_error",
    }
}

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
