import { apiRequest } from "../client";

export async function listNetworkDrivers() {
  return apiRequest("/api/pxe/network-drivers");
}

export async function getNetworkDriverStatus() {
  return apiRequest("/api/pxe/network-drivers/status");
}
