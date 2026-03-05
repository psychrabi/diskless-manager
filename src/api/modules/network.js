import { apiRequest } from "../client";

export async function getNetworkInterfaces() {
  return apiRequest("/api/system/network/interfaces");
}

export async function getInterfaceIp(interfaceName) {
  return apiRequest(
    `/api/system/network/interfaces/${encodeURIComponent(interfaceName)}/ip`
  );
}

export async function detectServerNetwork() {
  return apiRequest("/api/system/network/detect", { method: "POST" });
}

export async function applyNetworkSettings(settings) {
  return apiRequest("/api/system/network/apply", {
    method: "POST",
    body: JSON.stringify(settings),
  });
}
