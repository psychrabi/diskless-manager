use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::{
    core::dhcp_reconciliation::{
        inspect_dhcp, repair_client_dhcp, DhcpReconciliationEntry, DhcpReconciliationSummary,
    },
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct DhcpReconciliationApiError {
    pub error: String,
}

impl IntoResponse for DhcpReconciliationApiError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

/// Inspect enabled clients against the current DHCP configuration.
///
/// GET /api/system/reconciliation/dhcp
pub async fn inspect_dhcp_reconciliation(
    State(state): State<AppState>,
) -> Result<Json<DhcpReconciliationSummary>, DhcpReconciliationApiError> {
    inspect_dhcp(&state).await.map(Json).map_err(|error| {
        tracing::error!(error = %error, "DHCP reconciliation inspection failed");

        DhcpReconciliationApiError {
            error: error.to_string(),
        }
    })
}

/// Repair one client's DHCP configuration from persisted client data.
///
/// POST /api/system/reconciliation/dhcp/{id}
pub async fn repair_dhcp_reconciliation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DhcpReconciliationEntry>, DhcpReconciliationApiError> {
    repair_client_dhcp(&state, &id)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(
                client_id = %id,
                error = %error,
                "DHCP reconciliation repair failed"
            );

            DhcpReconciliationApiError {
                error: error.to_string(),
            }
        })
}
