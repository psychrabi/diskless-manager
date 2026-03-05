import { apiRequest } from "../client";

export async function getLogs(unit = null, lines = 50) {
  const params = new URLSearchParams();
  if (unit) {
    params.append("unit", unit);
  }
  params.append("lines", lines);
  return apiRequest(`/api/logs?${params.toString()}`);
}

export async function clearLogs() {
  return apiRequest("/api/logs", { method: "DELETE" });
}
