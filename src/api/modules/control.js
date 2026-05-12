import { apiRequest } from "../client";

export async function shutdownClient(clientId, options = {}) {
  return apiRequest(`/api/clients/${clientId}/shutdown`, {
    method: "POST",
    body: JSON.stringify({
      force: options.force || false,
      delay_minutes: options.delay_minutes || null,
    }),
  });
}

export async function rebootClient(clientId, options = {}) {
  return apiRequest(`/api/clients/${clientId}/reboot`, {
    method: "POST",
    body: JSON.stringify({
      force: options.force || false,
      delay_minutes: options.delay_minutes || null,
    }),
  });
}

export async function remoteDesktopClient(clientId, credentials = {}) {
  return apiRequest(`/api/clients/${clientId}/remote-desktop`, {
    method: "POST",
    body: JSON.stringify({
      username: credentials.username || "diskless",
      password: credentials.password || "1",
    }),
  });
}

export async function cancelScheduledOperation(operationId) {
  return apiRequest(`/api/operations/${operationId}/cancel`, {
    method: "POST",
    body: JSON.stringify({}),
  });
}

export async function getAuditLogs(filters = {}) {
  const params = new URLSearchParams();
  if (filters.client_id) params.append("client_id", filters.client_id);
  if (filters.operation_type)
    params.append("operation_type", filters.operation_type);
  if (filters.start_date) params.append("start_date", filters.start_date);
  if (filters.end_date) params.append("end_date", filters.end_date);

  return apiRequest(`/api/audit-logs?${params.toString()}`);
}

export async function getScheduledOperations(filters = {}) {
  const params = new URLSearchParams();
  if (filters.client_id) params.append("client_id", filters.client_id);
  if (filters.status) params.append("status", filters.status);

  return apiRequest(`/api/scheduled-operations?${params.toString()}`);
}
