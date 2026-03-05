import { apiRequest } from "../client";

export async function getSystemInfo() {
  return apiRequest("/api/system/info");
}

export async function getServerStatus() {
  return apiRequest("/api/system/status");
}

export async function initializeServer() {
  return apiRequest("/api/system/initialize", { method: "POST" });
}

export async function checkDependencies() {
  return apiRequest("/api/system/dependencies");
}

export async function clearRamCache() {
  return apiRequest("/api/system/cache/clear", { method: "POST" });
}

export async function getSystemSettings() {
  return apiRequest("/api/system/settings");
}

export async function saveSystemSettings(settings) {
  return apiRequest("/api/system/settings", {
    method: "PUT",
    body: JSON.stringify(settings),
  });
}

export async function setupPrivilegedAccess(config) {
  return apiRequest("/api/system/privileged-access", {
    method: "POST",
    body: JSON.stringify(config),
  });
}

export async function getSettings() {
  return apiRequest("/api/system/settings");
}

export async function saveSettings(settings) {
  return apiRequest("/api/system/settings", {
    method: "PUT",
    body: JSON.stringify(settings),
  });
}

export async function getRamUsage() {
  return apiRequest("/api/system/ram-usage");
}

export async function getZfsArcstat() {
  return apiRequest("/api/system/zfs-arcstat");
}
