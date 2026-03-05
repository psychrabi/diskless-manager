import { apiRequest } from "../client";

export async function getDefaultImageOverview() {
  return apiRequest("/api/dashboard/default-image");
}

export async function getClientOverview() {
  return apiRequest("/api/dashboard/clients");
}

export async function getClientIOMetrics() {
  return apiRequest("/api/dashboard/clients/io-metrics");
}
