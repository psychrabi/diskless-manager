import { apiRequest } from "../client";

export function listAlarms() {
  return apiRequest("/api/alarms");
}

export function acknowledgeAlarm(id) {
  return apiRequest(`/api/alarms/${encodeURIComponent(id)}/acknowledge`, {
    method: "POST",
  });
}
