import { apiRequest } from "../client";

export function getPendingClientRecoveries() {
  return apiRequest("/api/system/reconciliation/recoveries");
}

export function retryPendingClientRecovery(clientId) {
  return apiRequest(`/api/system/reconciliation/recoveries/${clientId}/retry`, {
    method: "POST",
  });
}
